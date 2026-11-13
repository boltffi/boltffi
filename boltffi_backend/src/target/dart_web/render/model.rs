use super::*;

pub struct Record {
    source: String,
}

impl Record {
    pub fn from_declaration(
        decl: &RecordDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        if !decl.initializers().is_empty() || !decl.methods().is_empty() {
            return Err(unsupported("record with inherent initializers/methods"));
        }
        let name = Name::new(decl.name()).dart_type_name();
        let fields: Vec<DataField> = match decl {
            RecordDecl::Direct(record) => record
                .fields()
                .iter()
                .map(|field| {
                    let ty = TypeRef::Primitive(field.ty().primitive());
                    DataField::new(field.key(), ty).with_default(field.meta().default())
                })
                .collect(),
            RecordDecl::Encoded(record) => record
                .fields()
                .iter()
                .map(|field| {
                    DataField::new(field.key(), field.ty().clone())
                        .with_default(field.meta().default())
                })
                .collect(),
            _ => return Err(unsupported("record declaration")),
        };

        let source = render_data_class(&name, None, &fields, decl.is_error_payload(), context)?;
        Ok(Self { source })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}

struct DataField {
    dart_name: String,
    wire_key: String,
    ty: TypeRef,
    default: Option<DefaultValue>,
}

impl DataField {
    fn with_default(mut self, value: Option<&DefaultValue>) -> Self {
        self.default = value.cloned();
        self
    }
    fn new(key: &boltffi_binding::FieldKey, ty: TypeRef) -> Self {
        Self {
            dart_name: field_dart_name(key),
            wire_key: field_wire_key(key),
            ty,
            default: None,
        }
    }
}

fn render_data_class(
    name: &str,
    extends: Option<(&str, &str)>,
    fields: &[DataField],
    implements_exception: bool,
    context: &RenderContext<Wasm32>,
) -> Result<String> {
    let mut field_decls = Vec::new();
    let mut ctor_params = Vec::new();
    let mut ctor_initializers = Vec::new();
    let mut to_js_entries = Vec::new();
    let mut from_js_args = Vec::new();

    if let Some((_, tag)) = extends {
        to_js_entries.push(format!("    result.setProperty('tag'.toJS, '{tag}'.toJS);"));
    }

    let immutable = extends.is_some();
    let field_keyword = if immutable { "final " } else { "" };
    let ctor_keyword = if immutable { "const " } else { "" };

    for field in fields {
        let DataField {
            dart_name,
            wire_key,
            ty,
            default,
        } = field;
        let dart_type = interop::dart_type(ty, context)?;
        field_decls.push(format!("  {field_keyword}{dart_type} {dart_name};"));
        ctor_params.push(match default {
            Some(value) => {
                use crate::target::dart::default_value::DefaultExpression;
                match web_default(ty, value, context)? {
                    DefaultExpression::Constant(value) => format!("this.{dart_name} = {value}"),
                    DefaultExpression::Runtime(value) if !immutable => {
                        ctor_initializers
                            .push(format!("this.{dart_name} = {dart_name} ?? {value}"));
                        format!("{dart_type}? {dart_name}")
                    }
                    _ => return Err(unsupported("runtime enum variant default")),
                }
            }
            None => format!("required this.{dart_name}"),
        });
        // `this.` qualification keeps a field named e.g. `result` from
        // resolving to the toJS accumulator local instead of the field.
        let to_js = interop::to_js(&format!("this.{dart_name}"), ty, context)?;
        to_js_entries.push(format!(
            "    result.setProperty('{wire_key}'.toJS, {to_js});"
        ));
        let from_js = interop::from_js(&format!("js.getProperty('{wire_key}'.toJS)"), ty, context)?;
        from_js_args.push(format!("{dart_name}: {from_js}"));
    }

    let (header, override_kw, extra_ctor) = match extends {
        Some((base, _)) => (
            format!("final class {name} extends {base}"),
            "@override\n  ",
            " : super._()".to_owned(),
        ),
        None => {
            // A data-enum variant class inherits "implements Exception"
            // through `extends` instead of declaring it again here.
            let exception_clause = if implements_exception {
                " implements Exception"
            } else {
                ""
            };
            (
                format!("final class {name}{exception_clause}"),
                "",
                String::new(),
            )
        }
    };
    let extra_ctor = if ctor_initializers.is_empty() {
        extra_ctor
    } else {
        format!(" : {}", ctor_initializers.join(", "))
    };
    let ctor_params_decl = if ctor_params.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", ctor_params.join(", "))
    };

    let comparisons = fields
        .iter()
        .map(|f| {
            Ok(
                crate::target::dart::value_semantics::ValueSemantics::for_type(&f.ty)?.equality(
                    &format!("this.{}", f.dart_name),
                    &format!("other.{}", f.dart_name),
                ),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let hashes = fields
        .iter()
        .map(|f| {
            Ok(
                crate::target::dart::value_semantics::ValueSemantics::for_type(&f.ty)?
                    .hash(&format!("this.{}", f.dart_name)),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let equality = if comparisons.is_empty() {
        "true".to_owned()
    } else {
        comparisons.join(" && ")
    };
    let hash = if hashes.is_empty() {
        "int get hashCode => runtimeType.hashCode;".to_owned()
    } else {
        let fields = hashes
            .iter()
            .map(|hash| format!("    result = 31 * result + {hash};"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("int get hashCode {{\n    var result = 1;\n{fields}\n    return result;\n  }}")
    };
    let semantics = format!(
        "  @override\n  bool operator ==(Object other) => identical(this, other) || other is {name} && {equality};\n\n  @override\n  {hash}\n"
    );
    let update = if immutable {
        String::new()
    } else {
        let statements = fields
            .iter()
            .map(|field| {
                let value = interop::from_js(
                    &format!("js.getProperty('{}'.toJS)", field.wire_key),
                    &field.ty,
                    context,
                )?;
                Ok(format!("    this.{} = {value};", field.dart_name))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        format!("  void _updateFromJS(JSObject js) {{\n{statements}\n  }}\n")
    };
    Ok(format!(
        "{header} {{\n{fields}\n\n  {ctor_keyword}{name}({ctor_params_decl}){extra_ctor};\n\n  {override_kw}JSObject toJS() {{\n    final result = JSObject();\n{to_js}\n    return result;\n  }}\n\n  static {name} fromJS(JSObject js) {{\n    return {name}({from_js});\n  }}\n{update}\n{semantics}}}\n\n",
        fields = field_decls.join("\n"),
        to_js = to_js_entries.join("\n"),
        from_js = from_js_args.join(", "),
    ))
}

pub(super) fn field_dart_name(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => Name::new(name).dart_identifier(),
        boltffi_binding::FieldKey::Position(position) => format!("value{position}"),
        _ => "field".to_owned(),
    }
}

fn field_wire_key(key: &boltffi_binding::FieldKey) -> String {
    match key {
        boltffi_binding::FieldKey::Named(name) => Name::new(name).js_export_name(),
        boltffi_binding::FieldKey::Position(position) => format!("value{position}"),
        _ => "field".to_owned(),
    }
}

pub struct Enumeration {
    source: String,
}

impl Enumeration {
    pub fn from_declaration(
        decl: &EnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let (initializers, methods) = match decl {
            EnumDecl::CStyle(cstyle) => (cstyle.initializers(), cstyle.methods()),
            EnumDecl::Data(data) => (data.initializers(), data.methods()),
            _ => return Err(unsupported("enum declaration")),
        };
        if !initializers.is_empty() || !methods.is_empty() {
            return Err(unsupported("enum with inherent initializers/methods"));
        }
        let source = match decl {
            EnumDecl::CStyle(cstyle) => Self::c_style(cstyle)?,
            EnumDecl::Data(data) => Self::data(data, context)?,
            _ => return Err(unsupported("enum declaration")),
        };
        Ok(Self { source })
    }

    fn c_style(decl: &boltffi_binding::CStyleEnumDecl<Wasm32>) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let variant_entries = decl
            .variants()
            .iter()
            .map(|variant| {
                let variant_name = Name::new(variant.name()).dart_identifier();
                let value = variant.discriminant().get();
                format!("  {variant_name}({value})")
            })
            .collect::<Vec<_>>()
            .join(",\n");

        let exception_clause = if decl.is_error_payload() {
            " implements Exception"
        } else {
            ""
        };
        if decl.variants().iter().any(|v| {
            v.discriminant().get() < -9007199254740991 || v.discriminant().get() > 9007199254740991
        }) {
            return Err(unsupported(
                "enum discriminant outside the exact web integer range",
            ));
        }
        let cases = decl
            .variants()
            .iter()
            .map(|variant| {
                format!(
                    "    {} => {},",
                    variant.discriminant().get(),
                    Name::new(variant.name()).dart_identifier()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "enum {name}{exception_clause} {{\n{variant_entries};\n\n  final int value;\n  const {name}(this.value);\n\n  JSAny toJS() => value.toJS;\n\n  static {name} fromJS(JSAny js) => _fromRaw((js as JSNumber).toDartInt);\n\n  static {name} _fromRaw(int value) => switch (value) {{\n{cases}\n    _ => throw ArgumentError.value(value, 'value', 'unknown {name} value'),\n  }};\n}}\n\n",
        ))
    }

    fn data(
        decl: &boltffi_binding::DataEnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        let name = Name::new(decl.name()).dart_type_name();
        let mut variant_classes = Vec::new();
        let mut from_js_cases = Vec::new();
        let mut factory_constructors = Vec::new();

        for variant in decl.variants() {
            let variant_name = Name::new(variant.name()).dart_identifier();
            let variant_dart_name = Name::new(variant.name()).dart_type_name();
            // Must match target::typescript's wire tag spelling exactly.
            let tag = variant_dart_name.clone();
            let variant_type = format!("{name}${variant_dart_name}");
            let fields: Vec<DataField> = variant
                .payload()
                .fields()
                .iter()
                .map(|field| {
                    DataField::new(field.key(), field.ty().clone())
                        .with_default(field.meta().default())
                })
                .collect();

            variant_classes.push(render_data_class(
                &variant_type,
                Some((&name, &tag)),
                &fields,
                false,
                context,
            )?);

            let from_js_args = fields
                .iter()
                .map(|field| {
                    let from_js = interop::from_js(
                        &format!("js.getProperty('{}'.toJS)", field.wire_key),
                        &field.ty,
                        context,
                    )?;
                    Ok(format!("{}: {from_js}", field.dart_name))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");

            from_js_cases.push(format!(
                "      case '{tag}': return {variant_type}({from_js_args});"
            ));

            let params = if fields.is_empty() {
                String::new()
            } else {
                let required = fields
                    .iter()
                    .map(|field| {
                        let dart_type = interop::dart_type(&field.ty, context)?;
                        Ok(match &field.default {
                            Some(value) => format!(
                                "{dart_type} {} = {}",
                                field.dart_name,
                                web_default(&field.ty, value, context)?.into_constant()?
                            ),
                            None => format!("required {dart_type} {}", field.dart_name),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                format!("{{{required}}}")
            };
            factory_constructors.push(format!(
                "  const factory {name}.{variant_name}({params}) = {variant_type};"
            ));
        }

        let exception_clause = if decl.is_error_payload() {
            " implements Exception"
        } else {
            ""
        };
        Ok(format!(
            "sealed class {name}{exception_clause} {{\n  const {name}._();\n\n{factories}\n\n  JSObject toJS();\n\n  static {name} fromJS(JSObject js) {{\n    final tag = (js.getProperty('tag'.toJS) as JSString).toDart;\n    switch (tag) {{\n{cases}\n      default: throw StateError('Unknown {name} tag: $tag');\n    }}\n  }}\n}}\n\n{variants}",
            factories = factory_constructors.join("\n"),
            cases = from_js_cases.join("\n"),
            variants = variant_classes.join(""),
        ))
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(self.source.clone()))
    }
}
