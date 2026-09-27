use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumDecl, EnumId,
    MapKind, Native, Op, Primitive, ReadPlan, RecordId, TypeRef,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::c::{name_style::Name, render::surface},
};

pub struct Reader<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
    source: String,
    package: String,
    next_local: usize,
}

pub struct DecodedValue {
    ty: TypeRef,
    statements: String,
}

impl DecodedValue {
    fn assign_to(self, destination: &str) -> String {
        self.statements
            .replace("(*boltffi_decoded_value)", &format!("({destination})"))
    }
}

impl<'context, 'bindings> Reader<'context, 'bindings> {
    pub fn new(
        context: &'context RenderContext<'bindings, Native>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            context,
            source: source.into(),
            package: Name::new(context.bindings().package().name()).member(),
            next_local: 0,
        }
    }

    pub fn decode(&mut self, plan: &ReadPlan, destination: &str) -> Result<String> {
        plan.render_with(self)
            .map(|value| value.assign_to(destination))
    }

    fn local(&mut self, stem: &str) -> String {
        let name = format!("boltffi_decode_{stem}_{}", self.next_local);
        self.next_local += 1;
        name
    }

    fn read_bytes(&self, ty: TypeRef, length_prefixed: bool) -> Result<DecodedValue> {
        let source = &self.source;
        let package = &self.package;
        let is_string = ty == TypeRef::String;
        let statements = if length_prefixed {
            let copy = if is_string {
                "copy_string"
            } else {
                "copy_bytes"
            };
            format!(
                "if (!boltffi_c_{package}_{copy}({source}, &(*boltffi_decoded_value))) {{ ({source})->ok = false; }}"
            )
        } else {
            let element_type = if is_string { "char" } else { "uint8_t" };
            let terminator = if is_string {
                "(*boltffi_decoded_value).ptr[(*boltffi_decoded_value).len] = '\\0';"
            } else {
                ""
            };
            let extra_byte = usize::from(is_string);
            format!(
                "if (({source})->ok) {{\n\
                 uintptr_t boltffi_remaining = ({source})->len - ({source})->offset;\n\
                 if (boltffi_remaining > UINTPTR_MAX - {extra_byte}) {{ ({source})->ok = false; }}\n\
                 else {{\n\
                 (*boltffi_decoded_value).ptr = ({element_type} *)boltffi_c_{package}_alloc(boltffi_remaining + {extra_byte}, 1);\n\
                 if (boltffi_remaining + {extra_byte} != 0 && (*boltffi_decoded_value).ptr == NULL) {{ ({source})->ok = false; }}\n\
                 else {{\n\
                 (*boltffi_decoded_value).len = boltffi_remaining;\n\
                 boltffi_c_{package}_read({source}, (*boltffi_decoded_value).ptr, boltffi_remaining);\n\
                 {terminator}\n\
                 }}\n}}\n}}"
            )
        };
        Ok(DecodedValue { ty, statements })
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget { target: "c", shape })
    }
}

impl CodecRead for Reader<'_, '_> {
    type Expr = Result<DecodedValue>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        let value = surface::wire_read_expr(primitive, &self.package)
            .replace("boltffi_reader", &self.source);
        Ok(DecodedValue {
            ty: TypeRef::Primitive(primitive),
            statements: format!("(*boltffi_decoded_value) = {value};"),
        })
    }

    fn string(&mut self) -> Self::Expr {
        self.read_bytes(TypeRef::String, true)
    }

    fn utf8_string(&mut self) -> Self::Expr {
        self.read_bytes(TypeRef::String, false)
    }

    fn raw_bytes(&mut self) -> Self::Expr {
        self.read_bytes(TypeRef::Bytes, false)
    }

    fn bytes(&mut self) -> Self::Expr {
        self.read_bytes(TypeRef::Bytes, true)
    }

    fn interned_string(&mut self, _: &[String]) -> Self::Expr {
        self.unsupported("interned string decoding")
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        Ok(DecodedValue {
            ty: TypeRef::Record(id),
            statements: format!(
                "boltffi_c_{}_read({}, &(*boltffi_decoded_value), sizeof(*boltffi_decoded_value));",
                self.package, self.source
            ),
        })
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        let decode = surface::record_helper_name(id, self.context, "decode")?;
        Ok(DecodedValue {
            ty: TypeRef::Record(id),
            statements: format!(
                "if (!{decode}({}, &(*boltffi_decoded_value))) {{ ({})->ok = false; }}",
                self.source, self.source
            ),
        })
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        let Some(EnumDecl::CStyle(enumeration)) = self.context.enumeration(id) else {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "C-style enum codec references a missing or data enum declaration",
            });
        };
        let ty = TypeRef::Enum(id);
        let c_type = surface::value_type(&ty, self.context, surface::ValueUse::Return)?;
        let value = surface::wire_read_expr(enumeration.repr().primitive(), &self.package)
            .replace("boltffi_reader", &self.source);
        Ok(DecodedValue {
            ty,
            statements: format!("(*boltffi_decoded_value) = ({c_type}){value};"),
        })
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        let decode =
            crate::target::c::render::enumeration::helper_name(id, self.context, "decode")?;
        Ok(DecodedValue {
            ty: TypeRef::Enum(id),
            statements: format!(
                "if (!{decode}({}, &(*boltffi_decoded_value))) {{ ({})->ok = false; }}",
                self.source, self.source
            ),
        })
    }

    fn class_handle(&mut self, id: ClassId) -> Self::Expr {
        Ok(DecodedValue {
            ty: TypeRef::Class(id),
            statements: format!(
                "(*boltffi_decoded_value)._boltffi_handle = boltffi_c_{}_read_le64({});",
                self.package, self.source
            ),
        })
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        self.unsupported("callback handle decoding")
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        representation.map(|representation| DecodedValue {
            ty: TypeRef::Custom(id),
            statements: representation.statements,
        })
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        let source = &self.source;
        let package = &self.package;
        let statements = match kind {
            BuiltinType::Url => self.string()?.statements,
            BuiltinType::Duration | BuiltinType::SystemTime => {
                let seconds = if kind == BuiltinType::Duration {
                    "read_le64"
                } else {
                    "read_i64"
                };
                format!(
                    "(*boltffi_decoded_value).seconds = boltffi_c_{package}_{seconds}({source});\n(*boltffi_decoded_value).nanoseconds = boltffi_c_{package}_read_u32({source});\nif ((*boltffi_decoded_value).nanoseconds >= 1000000000) {{ ({source})->ok = false; }}"
                )
            }
            BuiltinType::Uuid => format!(
                "for (uintptr_t boltffi_uuid_half = 0; boltffi_uuid_half < 2; ++boltffi_uuid_half) {{\nuint64_t boltffi_uuid_word = boltffi_c_{package}_read_le64({source});\nfor (uintptr_t boltffi_uuid_byte = 0; boltffi_uuid_byte < 8; ++boltffi_uuid_byte) {{\n(*boltffi_decoded_value).bytes[boltffi_uuid_half * 8 + boltffi_uuid_byte] = (uint8_t)(boltffi_uuid_word >> (56 - boltffi_uuid_byte * 8));\n}}\n}}"
            ),
        };
        Ok(DecodedValue {
            ty: TypeRef::Builtin(kind),
            statements,
        })
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        let inner = inner?;
        let ty = TypeRef::Optional(Box::new(inner.ty.clone()));
        let tag = self.local("tag");
        let statements = inner.assign_to("(*boltffi_decoded_value).value");
        Ok(DecodedValue {
            ty,
            statements: format!(
                "uint8_t {tag} = boltffi_c_{}_read_u8({});\n\
                 (*boltffi_decoded_value).has_value = {tag} == 1;\n\
                 if ({tag} == 1) {{\n{statements}\n}} else if ({tag} != 0) {{ ({})->ok = false; }}",
                self.package, self.source, self.source
            ),
        })
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        let c_type = surface::value_type(&element.ty, self.context, surface::ValueUse::Field)?;
        let ty = TypeRef::Sequence(Box::new(element.ty.clone()));
        let count = self.local("count");
        let index = self.local("index");
        let statements = element.assign_to(&format!("(*boltffi_decoded_value).ptr[{index}]"));
        Ok(DecodedValue {
            ty,
            statements: format!(
                "uint32_t {count} = boltffi_c_{}_read_u32({});\n\
                 if (({})->ok && {count} != 0) {{\n\
                 (*boltffi_decoded_value).ptr = ({c_type} *)boltffi_c_{}_alloc({count}, sizeof({c_type}));\n\
                 if ((*boltffi_decoded_value).ptr == NULL) {{ ({})->ok = false; }}\n\
                 else {{ (*boltffi_decoded_value).len = {count}; }}\n}}\n\
                 for (uintptr_t {index} = 0; ({})->ok && {index} < (*boltffi_decoded_value).len; ++{index}) {{\n{statements}\n}}",
                self.package, self.source, self.source, self.package, self.source, self.source
            ),
        })
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        let elements = elements.into_iter().collect::<Result<Vec<_>>>()?;
        let ty = TypeRef::Tuple(elements.iter().map(|element| element.ty.clone()).collect());
        let statements = elements
            .into_iter()
            .enumerate()
            .map(|(index, element)| {
                element.assign_to(&format!("(*boltffi_decoded_value).field_{index}"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(DecodedValue { ty, statements })
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        let ok = ok?;
        let error = err?;
        let ty = TypeRef::Result {
            ok: Box::new(ok.ty.clone()),
            err: Box::new(error.ty.clone()),
        };
        let tag = self.local("tag");
        let success = ok.assign_to("(*boltffi_decoded_value).data.value");
        let failure = error.assign_to("(*boltffi_decoded_value).data.error");
        Ok(DecodedValue {
            ty,
            statements: format!(
                "uint8_t {tag} = boltffi_c_{}_read_u8({});\n\
                 (*boltffi_decoded_value).ok = {tag} == 0;\n\
                 if ({tag} == 0) {{\n{success}\n}} else if ({tag} == 1) {{\n{failure}\n}} else {{ ({})->ok = false; }}",
                self.package, self.source, self.source
            ),
        })
    }

    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        let key = key?;
        let value = value?;
        let ty = TypeRef::Map {
            key: Box::new(key.ty.clone()),
            value: Box::new(value.ty.clone()),
        };
        let c_type = surface::value_type(&ty, self.context, surface::ValueUse::Return)?;
        let count = self.local("count");
        let index = self.local("index");
        let key = key.assign_to(&format!("(*boltffi_decoded_value).ptr[{index}].key"));
        let value = value.assign_to(&format!("(*boltffi_decoded_value).ptr[{index}].value"));
        let source = &self.source;
        let package = &self.package;
        Ok(DecodedValue {
            ty,
            statements: format!(
                "uint32_t {count} = boltffi_c_{package}_read_u32({source});\n\
                 if (({source})->ok && {count} != 0) {{\n\
                 (*boltffi_decoded_value).ptr = ({c_type}Entry *)boltffi_c_{package}_alloc({count}, sizeof({c_type}Entry));\n\
                 if ((*boltffi_decoded_value).ptr == NULL) {{ ({source})->ok = false; }}\n\
                 else {{ (*boltffi_decoded_value).len = {count}; }}\n}}\n\
                 for (uintptr_t {index} = 0; ({source})->ok && {index} < (*boltffi_decoded_value).len; ++{index}) {{\n{key}\n{value}\n}}"
            ),
        })
    }
}
