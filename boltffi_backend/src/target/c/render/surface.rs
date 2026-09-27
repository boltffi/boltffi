//! Package-prefixed C value types and the private wire codec used by the facade.

use boltffi_binding::{Bindings, BuiltinType, EnumDecl, Native, Primitive, RecordDecl, TypeRef};

use crate::{
    bridge::c,
    core::{Error, RenderContext, Result},
    target::c::name_style::Name,
};

use super::prefix::PackagePrefix;

pub fn render(
    bindings: &Bindings<Native>,
    context: &RenderContext<Native>,
    rendered: &std::collections::HashSet<boltffi_binding::DeclarationId>,
    record_definitions: &std::collections::BTreeMap<boltffi_binding::RecordId, String>,
) -> Result<String> {
    let record_ids = rendered
        .iter()
        .filter_map(|declaration| match declaration {
            boltffi_binding::DeclarationId::Record(record) => Some(*record),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let is_rendered = |decl: &boltffi_binding::Decl<Native>| {
        rendered.contains(&boltffi_binding::DeclarationRef::from(decl).id())
    };
    let prefix = PackagePrefix::from_context(context);
    let mut out = String::new();
    out.push_str("#include <stdlib.h>\n#include <string.h>\n#include <math.h>\n\n");
    out.push_str(&format!(
        "typedef struct {{ const uint8_t *ptr; uintptr_t len; }} {p}BytesView;\n\
         typedef struct {{ const char *ptr; uintptr_t len; }} {p}StringView;\n\
         typedef struct {{ uint8_t *ptr; uintptr_t len; }} {p}Bytes;\n\
         typedef struct {{ char *ptr; uintptr_t len; }} {p}String;\n\
         typedef struct {{ bool has_value; {p}StringView value; }} {p}OptionStringView;\n\
         typedef struct {{ bool has_value; {p}String value; }} {p}OptionString;\n\
         typedef struct {{ const {p}StringView *ptr; uintptr_t len; }} {p}StringSlice;\n\
         typedef struct {{ {p}String *ptr; uintptr_t len; }} {p}StringSequence;\n",
        p = package_pascal(context)
    ));
    all_primitives()
        .iter()
        .try_for_each(|primitive| -> Result<()> {
            let value_type = c::TypeFragment::anonymous(&c::Type::primitive(*primitive)?)?;
            let option_type = format!(
                "{}Option{}",
                package_pascal(context),
                primitive_stem(*primitive)
            );
            out.push_str(&format!(
                "typedef struct {{ bool has_value; {value_type} value; }} {option_type};\n"
            ));
            Ok(())
        })?;

    // Raw bridge declarations precede this facade, so direct aliases are safe here.
    for decl in bindings.decls() {
        match boltffi_binding::DeclarationRef::from(decl) {
            boltffi_binding::DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => {
                if !is_rendered(decl) {
                    continue;
                }
                out.push_str(&format!(
                    "typedef ___{} {};\n",
                    Name::new(enumeration.name()).r#type(),
                    prefix.type_name(enumeration.name())
                ));
                let value_type = prefix.type_name(enumeration.name());
                let option_type = self::value_type(
                    &TypeRef::Optional(Box::new(TypeRef::Enum(enumeration.id()))),
                    context,
                    ValueUse::Field,
                )?;
                out.push_str(&format!(
                    "typedef struct {{ bool has_value; {value_type} value; }} {option_type};\n"
                ));
                enumeration.variants().iter().for_each(|variant| {
                    let constant = format!(
                        "{}_{}",
                        Name::new(enumeration.name()).constant(),
                        Name::new(variant.name()).constant()
                    );
                    let public_constant = prefix.constant(&constant);
                    if public_constant != constant {
                        out.push_str(&format!("#define {public_constant} {constant}\n"));
                    }
                });
            }
            boltffi_binding::DeclarationRef::Record(RecordDecl::Direct(record)) => {
                if !is_rendered(decl) {
                    continue;
                }
                out.push_str(&format!(
                    "typedef ___{} {};\n",
                    Name::new(record.name()).r#type(),
                    prefix.type_name(record.name())
                ));
                let value_type = prefix.type_name(record.name());
                let option_type = self::value_type(
                    &TypeRef::Optional(Box::new(TypeRef::Record(record.id()))),
                    context,
                    ValueUse::Field,
                )?;
                out.push_str(&format!(
                    "typedef struct {{ bool has_value; {value_type} value; }} {option_type};\n"
                ));
            }
            boltffi_binding::DeclarationRef::Class(class) => {
                if !is_rendered(decl) {
                    continue;
                }
                let ty = prefix.type_name(class.name());
                out.push_str(&format!(
                    "typedef struct {{ uint64_t _boltffi_handle; }} {ty};\n"
                ));
            }
            _ => {}
        }
    }

    bindings
        .decls()
        .iter()
        .filter(|declaration| is_rendered(declaration))
        .for_each(|declaration| {
            if let boltffi_binding::DeclarationRef::Enum(EnumDecl::Data(enumeration)) =
                boltffi_binding::DeclarationRef::from(declaration)
            {
                let name = prefix.type_name(enumeration.name());
                out.push_str(&format!("typedef struct {name} {name};\n"));
                if type_owns(&TypeRef::Enum(enumeration.id()), context) {
                    out.push_str(&format!("typedef struct {name}View {name}View;\n"));
                }
            }
        });

    // Encoded records are named structs so mutually-referenced slice declarations work.
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = prefix.type_name(record.name());
            if type_owns(&TypeRef::Record(record.id()), context) {
                out.push_str(&format!(
                    "typedef struct {ty}View {ty}View;\ntypedef struct {ty} {ty};\n"
                ));
            } else {
                out.push_str(&format!(
                    "typedef struct {ty} {ty};\ntypedef {ty} {ty}View;\n"
                ));
            }
        }
    }

    // Public primitive views and owned sequences. A Slice is borrowed; a Sequence is
    // returned by value and released by its package-prefixed free helper.
    all_primitives()
        .iter()
        .try_for_each(|primitive| -> Result<()> {
            let stem = primitive_stem(*primitive);
            let c = c::TypeFragment::anonymous(&c::Type::primitive(*primitive)?)?;
            out.push_str(&format!(
                "typedef struct {{ const {c} *ptr; uintptr_t len; }} {p}{stem}Slice;\n\
             typedef struct {{ {c} *ptr; uintptr_t len; }} {p}{stem}MutSlice;\n\
             typedef struct {{ {c} *ptr; uintptr_t len; }} {p}{stem}Sequence;\n",
                p = package_pascal(context)
            ));
            Ok(())
        })?;
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(record) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = prefix.type_name(record.name());
            let slice_element = if matches!(record, RecordDecl::Encoded(_)) {
                format!("{ty}View")
            } else {
                ty.clone()
            };
            out.push_str(&format!(
                "typedef struct {{ const {slice_element} *ptr; uintptr_t len; }} {ty}Slice;\n\
                 typedef struct {{ {ty} *ptr; uintptr_t len; }} {ty}MutSlice;\n\
                 typedef struct {{ {ty} *ptr; uintptr_t len; }} {ty}Sequence;\n"
            ));
        }
    }

    let value_types = crate::target::c::values::ValueTypes::render(context, rendered)?;
    out.push_str(value_types.definitions());

    out.push_str(&wire_runtime(context));
    bindings.decls().iter().try_for_each(|declaration| -> Result<()> {
        let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(declaration)
        else {
            return Ok(());
        };
        if record_ids.contains(&record.id()) {
            let record_type = prefix.type_name(record.name());
            let size = record_helper_name(record.id(), context, "size")?;
            let encode = record_helper_name(record.id(), context, "encode")?;
            let owned_size = record_helper_name(record.id(), context, "size_owned")?;
            let owned_encode = record_helper_name(record.id(), context, "encode_owned")?;
            out.push_str(&format!("static inline uintptr_t {owned_size}(const {record_type} *value);\nstatic inline bool {owned_encode}(BoltFFICWireWriter *writer, const {record_type} *value);\n"));
            let decode = record_helper_name(record.id(), context, "decode")?;
            let free = record_helper_name(record.id(), context, "free")?;
            out.push_str(&format!(
                "static inline uintptr_t {size}(const {record_type}View *value);\n\
                 static inline bool {encode}(BoltFFICWireWriter *writer, const {record_type}View *value);\n\
                 static inline bool {decode}(BoltFFICWireReader *reader, {record_type} *value);\n\
                 static inline void {free}({record_type} *value);\n"
            ));
        }
        Ok(())
    })?;
    bindings.decls().iter().filter(|declaration| is_rendered(declaration)).try_for_each(|declaration| -> Result<()> {
        if let boltffi_binding::DeclarationRef::Enum(EnumDecl::Data(enumeration)) = boltffi_binding::DeclarationRef::from(declaration) {
            let ty = TypeRef::Enum(enumeration.id());
            let name = value_type(&ty, context, ValueUse::Return)?;
            let view = value_type(&ty, context, ValueUse::Param)?;
            let size = super::enumeration::helper_name(enumeration.id(), context, "size")?;
            let encode = super::enumeration::helper_name(enumeration.id(), context, "encode")?;
            let owned_size = super::enumeration::helper_name(enumeration.id(), context, "size_owned")?;
            let owned_encode = super::enumeration::helper_name(enumeration.id(), context, "encode_owned")?;
            out.push_str(&format!("static inline uintptr_t {owned_size}(const {name} *value);\nstatic inline bool {owned_encode}(BoltFFICWireWriter *writer, const {name} *value);\n"));
            let decode = super::enumeration::helper_name(enumeration.id(), context, "decode")?;
            let free = super::enumeration::helper_name(enumeration.id(), context, "free")?;
            out.push_str(&format!("static inline uintptr_t {size}(const {view} *value);\nstatic inline bool {encode}(BoltFFICWireWriter *writer, const {view} *value);\nstatic inline bool {decode}(BoltFFICWireReader *reader, {name} *value);\nstatic inline void {free}({name} *value);\n"));
        }
        Ok(())
    })?;
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(RecordDecl::Encoded(record)) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            out.push_str(&record_definitions[&record.id()]);
        }
    }
    out.push_str(&free_helpers(bindings, context, &record_ids)?);
    out.push_str(value_types.destructors());
    Ok(out)
}

#[derive(Clone, Copy)]
pub enum ValueUse {
    Param,
    /// Borrowed but mutable direct-vector parameter (`&mut [T]`).
    ParamMut,
    Field,
    Return,
}

impl ValueUse {
    pub fn codec_operation(self, operation: &str) -> String {
        match self {
            Self::Param => operation.to_owned(),
            Self::ParamMut | Self::Field | Self::Return => format!("{operation}_owned"),
        }
    }
}

pub fn value_type(ty: &TypeRef, context: &RenderContext<Native>, use_: ValueUse) -> Result<String> {
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match ty {
        TypeRef::Primitive(primitive) => {
            Ok(c::TypeFragment::anonymous(&c::Type::primitive(*primitive)?)?.to_string())
        }
        TypeRef::String => Ok(format!(
            "{p}String{}",
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        TypeRef::Bytes => Ok(format!(
            "{p}Bytes{}",
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|record| {
                let name = prefix.type_name(record.name());
                if matches!(use_, ValueUse::Param) && type_owns(ty, context) {
                    format!("{name}View")
                } else {
                    name
                }
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing record declaration for semantic C type",
            }),
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|enumeration| {
                let name = prefix.type_name(enumeration.name());
                if matches!(use_, ValueUse::Param) && type_owns(ty, context) {
                    format!("{name}View")
                } else {
                    name
                }
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing enum declaration for semantic C type",
            }),
        TypeRef::Class(id) => context
            .class(*id)
            .map(|class| prefix.type_name(class.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing class declaration for semantic C type",
            }),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Primitive(primitive) => Ok(format!("{p}Option{}", primitive_stem(*primitive))),
            TypeRef::String => Ok(format!(
                "{p}OptionString{}",
                if matches!(use_, ValueUse::Param) {
                    "View"
                } else {
                    ""
                }
            )),
            _ => {
                let inner = value_type(inner, context, use_)?;
                Ok(format!(
                    "{p}Option{}{}",
                    if matches!(ty, TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Record(_) | TypeRef::Enum(_)))
                    {
                        ""
                    } else {
                        "Of"
                    },
                    inner.strip_prefix(&p).unwrap_or(&inner)
                ))
            }
        },
        TypeRef::Builtin(kind) => match kind {
            BuiltinType::Url => value_type(&TypeRef::String, context, use_),
            BuiltinType::Duration => Ok(format!("{p}Duration")),
            BuiltinType::SystemTime => Ok(format!("{p}SystemTime")),
            BuiltinType::Uuid => Ok(format!("{p}Uuid")),
        },
        TypeRef::Custom(id) => {
            let custom = context
                .custom_type(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing custom C type",
                })?;
            let name = prefix.type_name(custom.name());
            Ok(
                if matches!(use_, ValueUse::Param) && type_owns(custom.representation(), context) {
                    format!("{name}View")
                } else {
                    name
                },
            )
        }
        TypeRef::Sequence(element) => sequence_type(element, context, use_),
        TypeRef::Tuple(elements) => {
            let elements = elements
                .iter()
                .map(|element| value_stem(element, context, use_))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("{p}Tuple{}", elements.join("")))
        }
        TypeRef::Result { ok, err } => Ok(format!(
            "{p}Result{}Or{}",
            value_stem(ok, context, use_)?,
            value_stem(err, context, use_)?
        )),
        TypeRef::Map { key, value } => Ok(format!(
            "{p}Map{}To{}{}",
            value_stem(key, context, use_)?,
            value_stem(value, context, use_)?,
            if matches!(use_, ValueUse::Param) {
                "View"
            } else {
                ""
            }
        )),
        _ => unsupported("encoded value type"),
    }
}

fn value_stem(ty: &TypeRef, context: &RenderContext<Native>, usage: ValueUse) -> Result<String> {
    if let TypeRef::Primitive(primitive) = ty {
        return Ok(primitive_stem(*primitive).to_owned());
    }
    let name = value_type(ty, context, usage)?;
    Ok(name
        .strip_prefix(&package_pascal(context))
        .unwrap_or(&name)
        .to_owned())
}

pub fn sequence_type(
    element: &TypeRef,
    context: &RenderContext<Native>,
    use_: ValueUse,
) -> Result<String> {
    let suffix = match use_ {
        ValueUse::Param | ValueUse::ParamMut => "Slice",
        _ => "Sequence",
    };
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match element {
        TypeRef::String => Ok(format!("{p}String{suffix}")),
        TypeRef::Primitive(primitive) => Ok(format!("{p}{}{suffix}", primitive_stem(*primitive))),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|record| format!("{}{suffix}", prefix.type_name(record.name())))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing sequence record",
            }),
        _ => {
            let element = value_type(element, context, use_)?;
            Ok(format!(
                "{p}{suffix}Of{}",
                element.strip_prefix(&p).unwrap_or(&element)
            ))
        }
    }
}

pub fn value_free_name(ty: &TypeRef, context: &RenderContext<Native>) -> Result<String> {
    let package = package_member(context);
    let prefix = PackagePrefix::from_context(context);
    let member = match ty {
        TypeRef::Primitive(primitive) => primitive_stem(*primitive).to_ascii_lowercase(),
        TypeRef::String => "string".to_owned(),
        TypeRef::Bytes => "bytes".to_owned(),
        TypeRef::Record(id) => Name::new(
            context
                .record(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing record for C destructor",
                })?
                .name(),
        )
        .member(),
        TypeRef::Enum(id) => Name::new(
            context
                .enumeration(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing enum for C destructor",
                })?
                .name(),
        )
        .member(),
        TypeRef::Optional(inner) | TypeRef::Sequence(inner) => {
            let inner_free = value_free_name(inner, context)?;
            let inner_member = inner_free
                .strip_prefix(&format!("{package}_"))
                .unwrap_or(&inner_free);
            let inner_member = inner_member.strip_suffix("_free").unwrap_or(inner_member);
            match ty {
                TypeRef::Optional(_)
                    if matches!(
                        inner.as_ref(),
                        TypeRef::Primitive(_)
                            | TypeRef::String
                            | TypeRef::Record(_)
                            | TypeRef::Enum(_)
                    ) =>
                {
                    format!("option_{inner_member}")
                }
                TypeRef::Optional(_) => format!("option_of_{inner_member}"),
                TypeRef::Sequence(_)
                    if matches!(
                        inner.as_ref(),
                        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Record(_)
                    ) =>
                {
                    format!("{inner_member}_sequence")
                }
                _ => format!("sequence_of_{inner_member}"),
            }
        }
        TypeRef::Tuple(elements) => {
            let members = elements
                .iter()
                .map(|element| {
                    value_free_name(element, context).map(|name| {
                        name.trim_start_matches(&format!("{package}_"))
                            .trim_end_matches("_free")
                            .to_owned()
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            format!("tuple_{}", members.join("_"))
        }
        TypeRef::Result {
            ok: key,
            err: value,
        }
        | TypeRef::Map { key, value } => {
            let key = value_free_name(key, context)?;
            let value = value_free_name(value, context)?;
            let key = key
                .trim_start_matches(&format!("{package}_"))
                .trim_end_matches("_free");
            let value = value
                .trim_start_matches(&format!("{package}_"))
                .trim_end_matches("_free");
            if matches!(ty, TypeRef::Map { .. }) {
                format!("map_{key}_to_{value}")
            } else {
                format!("result_{key}_or_{value}")
            }
        }
        TypeRef::Builtin(BuiltinType::Url) => "string".to_owned(),
        TypeRef::Builtin(BuiltinType::Duration) => "duration".to_owned(),
        TypeRef::Builtin(BuiltinType::SystemTime) => "system_time".to_owned(),
        TypeRef::Builtin(BuiltinType::Uuid) => "uuid".to_owned(),
        TypeRef::Custom(id) => Name::new(
            context
                .custom_type(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing custom C destructor name",
                })?
                .name(),
        )
        .member(),
        _ => return unsupported("C destructor name"),
    };
    Ok(prefix.member(&format!("{member}_free")))
}

pub fn direct_value_type(
    ty: &boltffi_binding::DirectValueType,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    match ty {
        boltffi_binding::DirectValueType::Primitive(p) => {
            Ok(c::TypeFragment::anonymous(&c::Type::primitive(*p)?)?.to_string())
        }
        boltffi_binding::DirectValueType::Record(id) => context
            .record(*id)
            .map(|r| prefix.type_name(r.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing direct record",
            }),
        boltffi_binding::DirectValueType::Enum(id) => context
            .enumeration(*id)
            .map(|e| prefix.type_name(e.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing direct enum",
            }),
        _ => unsupported("direct value type"),
    }
}

pub fn direct_vector_element_type(
    element: &boltffi_binding::DirectVectorElementType,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    match element {
        boltffi_binding::DirectVectorElementType::Primitive(p) => {
            Ok(c::TypeFragment::anonymous(&c::Type::primitive(p.primitive())?)?.to_string())
        }
        boltffi_binding::DirectVectorElementType::Record(id) => context
            .record(*id)
            .map(|r| prefix.type_name(r.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing vector record",
            }),
        _ => unsupported("direct vector element type"),
    }
}
pub fn direct_vector_type(
    element: &boltffi_binding::DirectVectorElementType,
    context: &RenderContext<Native>,
    use_: ValueUse,
) -> Result<String> {
    let suffix = match use_ {
        ValueUse::Param => "Slice",
        ValueUse::ParamMut => "MutSlice",
        _ => "Sequence",
    };
    let p = package_pascal(context);
    let prefix = PackagePrefix::from_context(context);
    match element {
        boltffi_binding::DirectVectorElementType::Primitive(v) => {
            Ok(format!("{p}{}{suffix}", primitive_stem(v.primitive())))
        }
        boltffi_binding::DirectVectorElementType::Record(id) => context
            .record(*id)
            .map(|r| format!("{}{suffix}", prefix.type_name(r.name())))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "missing vector record",
            }),
        _ => unsupported("direct vector type"),
    }
}

pub fn record_helper_name(
    id: boltffi_binding::RecordId,
    context: &RenderContext<Native>,
    op: &str,
) -> Result<String> {
    let record = context.record(id).ok_or(Error::BrokenBridgeContract {
        bridge: "c",
        invariant: "missing encoded record helper declaration",
    })?;
    Ok(format!(
        "boltffi_c_{}_{}_{}",
        package_member(context),
        op,
        Name::new(record.name()).member()
    ))
}

pub fn record_codec(
    record: &boltffi_binding::EncodedRecordDecl<Native>,
    context: &RenderContext<Native>,
) -> Result<String> {
    let prefix = PackagePrefix::from_context(context);
    let ty = prefix.type_name(record.name());
    let member = Name::new(record.name()).member();
    let base = format!("boltffi_c_{}", package_member(context));
    let encoding = [ValueUse::Param, ValueUse::Return]
        .into_iter()
        .map(|usage| -> Result<String> {
            let value_type = value_type(&TypeRef::Record(record.id()), context, usage)?;
            let size_name = record_helper_name(record.id(), context, &usage.codec_operation("size"))?;
            let encode_name = record_helper_name(record.id(), context, &usage.codec_operation("encode"))?;
            let mut size = Code::new();
            size.line("(void)boltffi_value;");
            size.line("uintptr_t boltffi_size = 0;");
            let mut encode = Code::new();
            record.fields().iter().try_for_each(|field| -> Result<()> {
                let field_name = Name::field(field.key())?;
                let value = format!("boltffi_value->{field_name}");
                let mut sizer = crate::target::c::codec::size::Sizer::new(context, &value, usage);
                size.open("{");
                size.line(sizer.measure(&field.read().write_self_value(), "boltffi_size")?);
                size.close("}");
                let mut writer = crate::target::c::codec::write::Writer::new(context, "boltffi_writer", &value, usage);
                encode.line(writer.encode(&field.read().write_self_value())?);
                Ok(())
            })?;
            size.line("return boltffi_size;");
            encode.line("return boltffi_writer->ok;");
            Ok(format!("static inline uintptr_t {size_name}(const {value_type} *boltffi_value) {{\n{size}}}\n\
                static inline bool {encode_name}(BoltFFICWireWriter *boltffi_writer, const {value_type} *boltffi_value) {{\n{encode}}}\n"))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    let mut decode = Code::new();
    decode.line("memset(boltffi_value, 0, sizeof(*boltffi_value));");
    let mut free = Code::new();
    record.fields().iter().try_for_each(|field| -> Result<()> {
        let field_name = Name::field(field.key())?;
        let value = format!("boltffi_value->{field_name}");
        let mut reader = crate::target::c::codec::read::Reader::new(context, "boltffi_reader");
        decode.open("{");
        decode.line(reader.decode(field.read(), &value)?);
        decode.close("}");
        free_value(field.ty(), &value, &mut free, context)?;
        Ok(())
    })?;
    decode.line(format!(
        "if (!boltffi_reader->ok) {{ {base}_free_{member}(boltffi_value); return false; }}"
    ));
    decode.line("return true;");
    free.line("memset(boltffi_value, 0, sizeof(*boltffi_value));");
    Ok(format!(
        "{encoding}\n\
         static inline bool {base}_decode_{member}(BoltFFICWireReader *boltffi_reader, {ty} *boltffi_value) {{\n{decode}}}\n\
         static inline void {base}_free_{member}({ty} *boltffi_value) {{\n    if (boltffi_value == NULL) return;\n{free}}}\n",
    ))
}

fn wire_runtime(context: &RenderContext<Native>) -> String {
    let p = package_member(context);
    format!(
        r#"
typedef struct {{ uint8_t *ptr; uintptr_t len; uintptr_t offset; bool ok; }} BoltFFICWireWriter;
typedef struct {{ const uint8_t *ptr; uintptr_t len; uintptr_t offset; bool ok; }} BoltFFICWireReader;
static inline void boltffi_c_{p}_write(BoltFFICWireWriter *w, const void *src, uintptr_t n) {{
    if (!w->ok || n > w->len - w->offset) {{ w->ok = false; return; }}
    if (n != 0) memcpy(w->ptr + w->offset, src, n);
    w->offset += n;
}}
static inline void boltffi_c_{p}_read(BoltFFICWireReader *r, void *dst, uintptr_t n) {{
    if (!r->ok || n > r->len - r->offset) {{ r->ok = false; return; }}
    if (n != 0) memcpy(dst, r->ptr + r->offset, n);
    r->offset += n;
}}
static inline void boltffi_c_{p}_write_u8(BoltFFICWireWriter *w, uint8_t v) {{ boltffi_c_{p}_write(w, &v, 1); }}
static inline void boltffi_c_{p}_write_u32(BoltFFICWireWriter *w, uint32_t v) {{
    uint8_t b[4] = {{(uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24)}};
    boltffi_c_{p}_write(w, b, 4);
}}
static inline uint8_t boltffi_c_{p}_read_u8(BoltFFICWireReader *r) {{ uint8_t v = 0; boltffi_c_{p}_read(r, &v, 1); return v; }}
static inline uint32_t boltffi_c_{p}_read_u32(BoltFFICWireReader *r) {{
    uint8_t b[4] = {{0,0,0,0}}; boltffi_c_{p}_read(r, b, 4);
    return (uint32_t)b[0] | ((uint32_t)b[1] << 8) | ((uint32_t)b[2] << 16) | ((uint32_t)b[3] << 24);
}}
static inline void boltffi_c_{p}_write_le16(BoltFFICWireWriter *w, uint16_t v) {{
    uint8_t b[2] = {{(uint8_t)v, (uint8_t)(v >> 8)}};
    boltffi_c_{p}_write(w, b, 2);
}}
static inline uint16_t boltffi_c_{p}_read_le16(BoltFFICWireReader *r) {{
    uint8_t b[2] = {{0,0}}; boltffi_c_{p}_read(r, b, 2);
    return (uint16_t)b[0] | ((uint16_t)b[1] << 8);
}}
static inline int8_t boltffi_c_{p}_read_i8(BoltFFICWireReader *r) {{
    uint8_t u = boltffi_c_{p}_read_u8(r); int8_t v; memcpy(&v, &u, 1); return v;
}}
static inline int16_t boltffi_c_{p}_read_i16(BoltFFICWireReader *r) {{
    uint16_t u = boltffi_c_{p}_read_le16(r); int16_t v; memcpy(&v, &u, 2); return v;
}}
static inline int32_t boltffi_c_{p}_read_i32(BoltFFICWireReader *r) {{
    uint32_t u = boltffi_c_{p}_read_u32(r); int32_t v; memcpy(&v, &u, 4); return v;
}}
static inline void boltffi_c_{p}_write_le64(BoltFFICWireWriter *w, uint64_t v) {{
    uint8_t b[8] = {{(uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)(v >> 24),
                     (uint8_t)(v >> 32), (uint8_t)(v >> 40), (uint8_t)(v >> 48), (uint8_t)(v >> 56)}};
    boltffi_c_{p}_write(w, b, 8);
}}
static inline uint64_t boltffi_c_{p}_read_le64(BoltFFICWireReader *r) {{
    uint8_t b[8] = {{0,0,0,0,0,0,0,0}}; boltffi_c_{p}_read(r, b, 8);
    return (uint64_t)b[0] | ((uint64_t)b[1] << 8) | ((uint64_t)b[2] << 16) | ((uint64_t)b[3] << 24)
         | ((uint64_t)b[4] << 32) | ((uint64_t)b[5] << 40) | ((uint64_t)b[6] << 48) | ((uint64_t)b[7] << 56);
}}
static inline int64_t boltffi_c_{p}_read_i64(BoltFFICWireReader *r) {{
    uint64_t u = boltffi_c_{p}_read_le64(r); int64_t v; memcpy(&v, &u, 8); return v;
}}
static inline void boltffi_c_{p}_write_f32(BoltFFICWireWriter *w, float v) {{
    uint32_t u; memcpy(&u, &v, 4); boltffi_c_{p}_write_u32(w, u);
}}
static inline float boltffi_c_{p}_read_f32(BoltFFICWireReader *r) {{
    uint32_t u = boltffi_c_{p}_read_u32(r); float v; memcpy(&v, &u, 4); return v;
}}
static inline void boltffi_c_{p}_write_f64(BoltFFICWireWriter *w, double v) {{
    uint64_t u; memcpy(&u, &v, 8); boltffi_c_{p}_write_le64(w, u);
}}
static inline double boltffi_c_{p}_read_f64(BoltFFICWireReader *r) {{
    uint64_t u = boltffi_c_{p}_read_le64(r); double v; memcpy(&v, &u, 8); return v;
}}
static inline void boltffi_c_{p}_write_usize(BoltFFICWireWriter *w, uintptr_t v) {{
    boltffi_c_{p}_write_le64(w, (uint64_t)v);
}}
static inline uintptr_t boltffi_c_{p}_read_usize(BoltFFICWireReader *r) {{
    return (uintptr_t)boltffi_c_{p}_read_le64(r);
}}
static inline void boltffi_c_{p}_write_isize(BoltFFICWireWriter *w, intptr_t v) {{
    boltffi_c_{p}_write_le64(w, (uint64_t)v);
}}
static inline intptr_t boltffi_c_{p}_read_isize(BoltFFICWireReader *r) {{
    return (intptr_t)boltffi_c_{p}_read_i64(r);
}}
static inline void *boltffi_c_{p}_alloc(uintptr_t count, uintptr_t size) {{
    if (count == 0) return NULL;
    if (size != 0 && count > (uintptr_t)-1 / size) return NULL;
    return calloc((size_t)count, (size_t)size);
}}
static inline bool boltffi_c_{p}_copy_string(BoltFFICWireReader *r, {package_type_prefix}String *out) {{
    uint32_t n = boltffi_c_{p}_read_u32(r); char *copy;
    if (!r->ok || (uintptr_t)n > r->len - r->offset) return false;
    uintptr_t allocation_size = (uintptr_t)n + 1;
    if (allocation_size == 0) {{ r->ok = false; return false; }}
    copy = (char *)boltffi_c_{p}_alloc(allocation_size, 1);
    if (copy == NULL) {{ r->ok = false; return false; }}
    boltffi_c_{p}_read(r, copy, n); copy[n] = '\0'; out->ptr = copy; out->len = n; return r->ok;
}}
static inline bool boltffi_c_{p}_copy_bytes(BoltFFICWireReader *r, {package_type_prefix}Bytes *out) {{
    uint32_t n = boltffi_c_{p}_read_u32(r); uint8_t *copy;
    if (!r->ok || (uintptr_t)n > r->len - r->offset) return false;
    copy = (uint8_t *)boltffi_c_{p}_alloc(n, 1);
    if (n != 0 && copy == NULL) {{ r->ok = false; return false; }}
    boltffi_c_{p}_read(r, copy, n); out->ptr = copy; out->len = n; return r->ok;
}}
"#,
        package_type_prefix = package_pascal(context)
    )
}

struct Code {
    text: String,
    indent: usize,
    next: usize,
}
impl Code {
    fn new() -> Self {
        Self {
            text: String::new(),
            indent: 1,
            next: 0,
        }
    }
    fn line(&mut self, line: impl AsRef<str>) {
        self.text.push_str(&"    ".repeat(self.indent));
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }
    fn open(&mut self, line: impl AsRef<str>) {
        self.line(line);
        self.indent += 1;
    }
    fn close(&mut self, line: impl AsRef<str>) {
        self.indent -= 1;
        self.line(line);
    }
    fn var(&mut self, stem: &str) -> String {
        let v = format!("boltffi_{stem}_{}", self.next);
        self.next += 1;
        v
    }
}
impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

pub fn free_owned(
    ty: &TypeRef,
    expression: &str,
    context: &RenderContext<Native>,
) -> Result<String> {
    let mut code = Code::new();
    free_value(ty, expression, &mut code, context)?;
    Ok(code.text)
}

fn free_value(
    ty: &TypeRef,
    expr: &str,
    code: &mut Code,
    context: &RenderContext<Native>,
) -> Result<()> {
    match ty {
        TypeRef::String | TypeRef::Bytes => {
            code.line(format!("free((void *){expr}.ptr);"));
            code.line(format!("{expr}.ptr=NULL; {expr}.len=0;"));
        }
        TypeRef::Record(id) => {
            if matches!(context.record(*id), Some(RecordDecl::Encoded(_))) {
                code.line(format!(
                    "{}(&{expr});",
                    record_helper_name(*id, context, "free")?
                ));
            }
        }
        TypeRef::Optional(inner) => {
            code.open(format!("if ({expr}.has_value) {{"));
            free_value(inner, &format!("{expr}.value"), code, context)?;
            code.close("}");
            code.line(format!("{expr}.has_value=false;"));
        }
        TypeRef::Sequence(element) => {
            if type_owns(element, context) {
                let index = code.var("index");
                code.open(format!(
                    "for (uintptr_t {index}=0; {index} < {expr}.len; ++{index}) {{"
                ));
                free_value(element, &format!("{expr}.ptr[{index}]"), code, context)?;
                code.close("}");
            }
            code.line(format!(
                "free((void *){expr}.ptr); {expr}.ptr=NULL; {expr}.len=0;"
            ));
        }
        TypeRef::Enum(id) if matches!(context.enumeration(*id), Some(EnumDecl::Data(_))) => {
            code.line(format!(
                "{}(&{expr});",
                super::enumeration::helper_name(*id, context, "free")?
            ));
        }
        TypeRef::Primitive(_) | TypeRef::Enum(_) => {}
        TypeRef::Tuple(elements) => {
            elements
                .iter()
                .enumerate()
                .try_for_each(|(index, element)| {
                    free_value(element, &format!("{expr}.field_{index}"), code, context)
                })?
        }
        TypeRef::Result { ok, err } => {
            code.open(format!("if ({expr}.ok) {{"));
            free_value(ok, &format!("{expr}.data.value"), code, context)?;
            code.close("} else {");
            free_value(err, &format!("{expr}.data.error"), code, context)?;
            code.line("}");
        }
        TypeRef::Map { key, value } => {
            if type_owns(key, context) || type_owns(value, context) {
                let index = code.var("index");
                code.open(format!(
                    "for (uintptr_t {index} = 0; {index} < {expr}.len; ++{index}) {{"
                ));
                free_value(key, &format!("{expr}.ptr[{index}].key"), code, context)?;
                free_value(value, &format!("{expr}.ptr[{index}].value"), code, context)?;
                code.close("}");
            }
            code.line(format!(
                "free({expr}.ptr); {expr}.ptr = NULL; {expr}.len = 0;"
            ));
        }
        TypeRef::Builtin(BuiltinType::Url) => free_value(&TypeRef::String, expr, code, context)?,
        TypeRef::Builtin(_) => {}
        TypeRef::Custom(id) => free_value(
            context
                .custom_type(*id)
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "missing custom C value destructor",
                })?
                .representation(),
            expr,
            code,
            context,
        )?,
        _ => return unsupported("wire free type"),
    }
    Ok(())
}

/// Emits one fixed-width little-endian wire write statement for a primitive.
pub fn wire_write_stmt(primitive: Primitive, expr: &str, package: &str) -> String {
    let call = |name: &str, cast: &str| {
        format!("boltffi_c_{package}_{name}(boltffi_writer, ({cast}){expr});")
    };
    match primitive {
        Primitive::Bool | Primitive::I8 | Primitive::U8 => call("write_u8", "uint8_t"),
        Primitive::I16 | Primitive::U16 => call("write_le16", "uint16_t"),
        Primitive::I32 | Primitive::U32 => call("write_u32", "uint32_t"),
        Primitive::I64 | Primitive::U64 => call("write_le64", "uint64_t"),
        Primitive::F32 => format!("boltffi_c_{package}_write_f32(boltffi_writer, {expr});"),
        Primitive::F64 => format!("boltffi_c_{package}_write_f64(boltffi_writer, {expr});"),
        Primitive::ISize => format!("boltffi_c_{package}_write_isize(boltffi_writer, {expr});"),
        Primitive::USize => format!("boltffi_c_{package}_write_usize(boltffi_writer, {expr});"),
        _ => format!("(void){expr};"),
    }
}

/// Emits the read expression for a primitive's fixed-width little-endian wire form.
pub fn wire_read_expr(primitive: Primitive, package: &str) -> String {
    match primitive {
        Primitive::Bool | Primitive::U8 => format!("boltffi_c_{package}_read_u8(boltffi_reader)"),
        Primitive::I8 => format!("boltffi_c_{package}_read_i8(boltffi_reader)"),
        Primitive::I16 => format!("boltffi_c_{package}_read_i16(boltffi_reader)"),
        Primitive::U16 => format!("boltffi_c_{package}_read_le16(boltffi_reader)"),
        Primitive::I32 => format!("boltffi_c_{package}_read_i32(boltffi_reader)"),
        Primitive::U32 => format!("boltffi_c_{package}_read_u32(boltffi_reader)"),
        Primitive::I64 => format!("boltffi_c_{package}_read_i64(boltffi_reader)"),
        Primitive::U64 => format!("boltffi_c_{package}_read_le64(boltffi_reader)"),
        Primitive::F32 => format!("boltffi_c_{package}_read_f32(boltffi_reader)"),
        Primitive::F64 => format!("boltffi_c_{package}_read_f64(boltffi_reader)"),
        Primitive::ISize => format!("boltffi_c_{package}_read_isize(boltffi_reader)"),
        Primitive::USize => format!("boltffi_c_{package}_read_usize(boltffi_reader)"),
        _ => "0".to_owned(),
    }
}

fn free_helpers(
    bindings: &Bindings<Native>,
    context: &RenderContext<Native>,
    record_ids: &std::collections::HashSet<boltffi_binding::RecordId>,
) -> Result<String> {
    let mut out = String::new();
    let p = package_member(context);
    let package_type_prefix = package_pascal(context);
    out.push_str(&format!("static inline {package_type_prefix}StringView {p}_string_view(const char *ptr, uintptr_t len) {{ {package_type_prefix}StringView v={{ptr,len}}; return v; }}\nstatic inline {package_type_prefix}BytesView {p}_bytes_view(const uint8_t *ptr, uintptr_t len) {{ {package_type_prefix}BytesView v={{ptr,len}}; return v; }}\n"));
    out.push_str(&format!("static inline void {p}_string_free({package_type_prefix}String *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\nstatic inline void {p}_string_sequence_free({package_type_prefix}StringSequence *v) {{ if (v == NULL) return; for (uintptr_t i=0;i<v->len;++i) {p}_string_free(&v->ptr[i]); free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\nstatic inline void {p}_bytes_free({package_type_prefix}Bytes *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\n"));
    for primitive in all_primitives() {
        let stem = primitive_stem(*primitive);
        let member = stem.to_ascii_lowercase();
        out.push_str(&format!("static inline void {p}_{member}_sequence_free({package_type_prefix}{stem}Sequence *v) {{ if (v == NULL) return; free((void *)v->ptr); v->ptr=NULL; v->len=0; }}\n"));
    }
    let prefix = PackagePrefix::from_context(context);
    for decl in bindings.decls() {
        if let boltffi_binding::DeclarationRef::Record(record) =
            boltffi_binding::DeclarationRef::from(decl)
        {
            if !record_ids.contains(&record.id()) {
                continue;
            }
            let ty = prefix.type_name(record.name());
            let m = Name::new(record.name()).member();
            if matches!(record, RecordDecl::Encoded(encoded) if encoded.fields().iter().any(|field| type_owns(field.ty(), context)))
            {
                out.push_str(&format!(
                    "static inline void {p}_{m}_free({ty} *v) {{ {}(v); }}\n",
                    record_helper_name(record.id(), context, "free")?
                ));
            }
            out.push_str(&format!("static inline void {p}_{m}_sequence_free({ty}Sequence *v) {{ if (v == NULL) return;"));
            if type_owns(&TypeRef::Record(record.id()), context) {
                out.push_str(&format!(
                    " for (uintptr_t i=0;i<v->len;++i) {}(&v->ptr[i]);",
                    record_helper_name(record.id(), context, "free")?
                ));
            }
            out.push_str(" free((void *)v->ptr); v->ptr=NULL; v->len=0; }\n");
        }
    }
    Ok(out)
}

pub fn type_owns(ty: &TypeRef, context: &RenderContext<Native>) -> bool {
    match ty {
        TypeRef::String | TypeRef::Bytes | TypeRef::Sequence(_) | TypeRef::Map { .. } => true,
        TypeRef::Tuple(elements) => elements.iter().any(|element| type_owns(element, context)),
        TypeRef::Result { ok, err } => type_owns(ok, context) || type_owns(err, context),
        TypeRef::Optional(inner) => type_owns(inner, context),
        TypeRef::Enum(id) => match context.enumeration(*id) {
            Some(EnumDecl::Data(enumeration)) => enumeration.variants().iter().any(|variant| {
                variant
                    .payload()
                    .fields()
                    .iter()
                    .any(|field| type_owns(field.ty(), context))
            }),
            _ => false,
        },
        TypeRef::Builtin(BuiltinType::Url) => true,
        TypeRef::Custom(id) => context
            .custom_type(*id)
            .is_some_and(|custom| type_owns(custom.representation(), context)),
        TypeRef::Record(id) => match context.record(*id) {
            Some(RecordDecl::Encoded(r)) => r.fields().iter().any(|f| type_owns(f.ty(), context)),
            _ => false,
        },
        _ => false,
    }
}

fn primitive_stem(p: Primitive) -> &'static str {
    match p {
        Primitive::Bool => "Bool",
        Primitive::I8 => "I8",
        Primitive::U8 => "U8",
        Primitive::I16 => "I16",
        Primitive::U16 => "U16",
        Primitive::I32 => "I32",
        Primitive::U32 => "U32",
        Primitive::I64 => "I64",
        Primitive::U64 => "U64",
        Primitive::ISize => "ISize",
        Primitive::USize => "USize",
        Primitive::F32 => "F32",
        Primitive::F64 => "F64",
        _ => "Unsupported",
    }
}
fn all_primitives() -> &'static [Primitive] {
    &[
        Primitive::Bool,
        Primitive::I8,
        Primitive::U8,
        Primitive::I16,
        Primitive::U16,
        Primitive::I32,
        Primitive::U32,
        Primitive::I64,
        Primitive::U64,
        Primitive::ISize,
        Primitive::USize,
        Primitive::F32,
        Primitive::F64,
    ]
}
fn package_pascal(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).r#type()
}
fn package_member(context: &RenderContext<Native>) -> String {
    Name::new(context.bindings().package().name()).member()
}
fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget { target: "c", shape })
}
