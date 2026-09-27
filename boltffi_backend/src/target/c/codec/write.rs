use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumDecl,
    EnumId, MapKind, Native, Op, Primitive, RecordId, ValueRef, WritePlan,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::c::{name_style::Name, render::surface},
};

use super::value;

pub struct Writer<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
    destination: String,
    root: String,
    usage: surface::ValueUse,
    package: String,
}

impl<'context, 'bindings> Writer<'context, 'bindings> {
    pub fn new(
        context: &'context RenderContext<'bindings, Native>,
        destination: impl Into<String>,
        root: impl Into<String>,
        usage: surface::ValueUse,
    ) -> Self {
        Self {
            context,
            destination: destination.into(),
            root: root.into(),
            usage,
            package: Name::new(context.bindings().package().name()).member(),
        }
    }

    pub fn encode(&mut self, plan: &WritePlan) -> Result<String> {
        plan.render_with(self)
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .map(|statements| statements.join("\n"))
    }

    fn bind(
        &self,
        statements: Vec<Result<String>>,
        binder: BinderId,
        expression: &str,
    ) -> Result<String> {
        statements
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .map(|statements| value::bind(&statements.join("\n"), binder, expression))
    }

    fn write_bytes(&self, value: &ValueRef, length_prefixed: bool) -> Result<String> {
        let value = value::reference(value, &self.root)?;
        let destination = &self.destination;
        let package = &self.package;
        let length = if length_prefixed {
            format!(
                "if ({value}.len > UINT32_MAX) {{ ({destination})->ok = false; }}\n\
                 boltffi_c_{package}_write_u32({destination}, (uint32_t){value}.len);\n"
            )
        } else {
            String::new()
        };
        Ok(format!(
            "{length}boltffi_c_{package}_write({destination}, {value}.ptr, {value}.len);"
        ))
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget { target: "c", shape })
    }
}

impl CodecWrite for Writer<'_, '_> {
    type Stmt = Result<String>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).map(|value| {
            surface::wire_write_stmt(primitive, &value, &self.package)
                .replace("boltffi_writer", &self.destination)
        })]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_bytes(value, true)]
    }

    fn utf8_string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_bytes(value, false)]
    }

    fn raw_bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_bytes(value, false)]
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_bytes(value, true)]
    }

    fn interned_string(&mut self, _: &[String], _: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.unsupported("interned string encoding")]
    }

    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).map(|value| {
            format!(
                "boltffi_c_{}_write({}, &{value}, sizeof({value}));",
                self.package, self.destination
            )
        })]
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).and_then(|value| {
            let encode = surface::record_helper_name(
                id,
                self.context,
                &self.usage.codec_operation("encode"),
            )?;
            Ok(format!("{encode}({}, &{value});", self.destination))
        })]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        match self.context.enumeration(id) {
            Some(EnumDecl::CStyle(enumeration)) => {
                self.primitive(enumeration.repr().primitive(), value)
            }
            _ => vec![Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "C-style enum codec references a missing or data enum declaration",
            })],
        }
    }

    fn data_enum(&mut self, id: EnumId, reference: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(reference, &self.root).and_then(|value| {
            let encode = crate::target::c::render::enumeration::helper_name(
                id,
                self.context,
                &self.usage.codec_operation("encode"),
            )?;
            Ok(format!("{encode}({}, &{value});", self.destination))
        })]
    }

    fn class_handle(&mut self, _: ClassId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).map(|value| {
            format!(
                "boltffi_c_{}_write_le64({}, {value}._boltffi_handle);",
                self.package, self.destination
            )
        })]
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.unsupported("callback handle encoding")]
    }

    fn custom<F>(&mut self, _: CustomTypeId, value: &ValueRef, representation: F) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        representation(self, value)
    }

    fn builtin(&mut self, kind: BuiltinType, reference: &ValueRef) -> Vec<Self::Stmt> {
        vec![value::reference(reference, &self.root).and_then(|value| {
            let destination = &self.destination;
            let package = &self.package;
            Ok(match kind {
                BuiltinType::Url => self.write_bytes(reference, true)?,
                BuiltinType::Duration | BuiltinType::SystemTime => format!("if ({value}.nanoseconds >= 1000000000) {{ ({destination})->ok = false; }}\nboltffi_c_{package}_write_le64({destination}, (uint64_t){value}.seconds);\nboltffi_c_{package}_write_u32({destination}, {value}.nanoseconds);"),
                BuiltinType::Uuid => format!("for (uintptr_t boltffi_uuid_half = 0; boltffi_uuid_half < 2; ++boltffi_uuid_half) {{\nuint64_t boltffi_uuid_word = 0;\nfor (uintptr_t boltffi_uuid_byte = 0; boltffi_uuid_byte < 8; ++boltffi_uuid_byte) {{\nboltffi_uuid_word = (boltffi_uuid_word << 8) | {value}.bytes[boltffi_uuid_half * 8 + boltffi_uuid_byte];\n}}\nboltffi_c_{package}_write_le64({destination}, boltffi_uuid_word);\n}}"),
            })
        })]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).and_then(|value| {
            let inner = self.bind(inner, binder, &format!("({value}.value)"))?;
            Ok(format!(
                "boltffi_c_{}_write_u8({}, {value}.has_value ? 1 : 0);\n\
                 if ({value}.has_value) {{\n{inner}\n}}",
                self.package, self.destination
            ))
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).and_then(|value| {
            let index = format!("boltffi_index_{}", binder.raw());
            let element = self.bind(element, binder, &format!("({value}.ptr[{index}])"))?;
            Ok(format!(
                "if ({value}.len > UINT32_MAX) {{ ({})->ok = false; }}\n\
                 boltffi_c_{}_write_u32({}, (uint32_t){value}.len);\n\
                 for (uintptr_t {index} = 0; ({})->ok && {index} < {value}.len; ++{index}) {{\n{element}\n}}",
                self.destination, self.package, self.destination, self.destination
            ))
        })]
    }

    fn tuple(&mut self, _: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        elements.into_iter().flatten().collect()
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).and_then(|value| {
            let ok = self.bind(ok, binder, &format!("({value}.data.value)"))?;
            let error = self.bind(err, binder, &format!("({value}.data.error)"))?;
            Ok(format!(
                "boltffi_c_{}_write_u8({}, {value}.ok ? 0 : 1);\n\
                 if ({value}.ok) {{\n{ok}\n}} else {{\n{error}\n}}",
                self.package, self.destination
            ))
        })]
    }

    fn map(
        &mut self,
        _: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![value::reference(value, &self.root).and_then(|value| {
            let index = format!("boltffi_index_{}", key_binder.raw());
            let key = self.bind(key, key_binder, &format!("({value}.ptr[{index}].key)"))?;
            let map_value = self.bind(
                map_value,
                value_binder,
                &format!("({value}.ptr[{index}].value)"),
            )?;
            Ok(format!(
                "if ({value}.len > UINT32_MAX) {{ ({})->ok = false; }}\n\
                 boltffi_c_{}_write_u32({}, (uint32_t){value}.len);\n\
                 for (uintptr_t {index} = 0; ({})->ok && {index} < {value}.len; ++{index}) {{\n{key}\n{map_value}\n}}",
                self.destination, self.package, self.destination, self.destination
            ))
        })]
    }
}
