use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumDecl,
    EnumId, MapKind, Native, Op, Primitive, RecordId, TypeRef, ValueRef, WritePlan,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::c::render::surface,
};

use super::value;

pub struct Sizer<'context, 'bindings> {
    context: &'context RenderContext<'bindings, Native>,
    root: String,
    usage: surface::ValueUse,
}

pub enum EncodedSize {
    Fixed(u64),
    Variable(String),
}

impl EncodedSize {
    fn add_to(self, destination: &str) -> String {
        match self {
            Self::Fixed(bytes) => format!("{destination} += {bytes};"),
            Self::Variable(statements) => {
                statements.replace("(*boltffi_encoded_size)", destination)
            }
        }
    }
}

impl<'context, 'bindings> Sizer<'context, 'bindings> {
    pub fn new(
        context: &'context RenderContext<'bindings, Native>,
        root: impl Into<String>,
        usage: surface::ValueUse,
    ) -> Self {
        Self {
            context,
            root: root.into(),
            usage,
        }
    }

    pub fn measure(&mut self, plan: &WritePlan, destination: &str) -> Result<String> {
        plan.size_with(self).map(|size| size.add_to(destination))
    }

    fn bytes_size(&self, value: &ValueRef, prefix_size: u32) -> Result<EncodedSize> {
        let value = value::reference(value, &self.root)?;
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += {prefix_size} + {value}.len;"
        )))
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget { target: "c", shape })
    }
}

impl CodecSize for Sizer<'_, '_> {
    type Expr = Result<EncodedSize>;

    fn primitive(&mut self, primitive: Primitive, _: &ValueRef) -> Self::Expr {
        Ok(EncodedSize::Fixed(primitive.wire_size().get()))
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value, 4)
    }

    fn utf8_string(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value, 0)
    }

    fn raw_bytes(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value, 0)
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value, 4)
    }

    fn interned_string(&mut self, _: &[String], _: &ValueRef) -> Self::Expr {
        self.unsupported("interned string size")
    }

    fn direct_record(&mut self, id: RecordId, _: &ValueRef) -> Self::Expr {
        let ty = surface::value_type(&TypeRef::Record(id), self.context, surface::ValueUse::Param)?;
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += sizeof({ty});"
        )))
    }

    fn encoded_record(&mut self, id: RecordId, reference: &ValueRef) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let measure =
            surface::record_helper_name(id, self.context, &self.usage.codec_operation("size"))?;
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += {measure}(&{value});"
        )))
    }

    fn c_style_enum(&mut self, id: EnumId, _: &ValueRef) -> Self::Expr {
        match self.context.enumeration(id) {
            Some(EnumDecl::CStyle(enumeration)) => Ok(EncodedSize::Fixed(
                enumeration.repr().primitive().wire_size().get(),
            )),
            _ => Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "C-style enum codec references a missing or data enum declaration",
            }),
        }
    }

    fn data_enum(&mut self, id: EnumId, reference: &ValueRef) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let measure = crate::target::c::render::enumeration::helper_name(
            id,
            self.context,
            &self.usage.codec_operation("size"),
        )?;
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += {measure}(&{value});"
        )))
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Self::Expr {
        Ok(EncodedSize::Fixed(8))
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Self::Expr {
        Ok(EncodedSize::Fixed(16))
    }

    fn custom<F>(&mut self, _: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
    {
        representation(self, value)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind.fixed_wire_size() {
            Some(bytes) => Ok(EncodedSize::Fixed(bytes)),
            None => self.bytes_size(value, 4),
        }
    }

    fn optional(
        &mut self,
        reference: &ValueRef,
        binder: BinderId,
        inner: Self::Expr,
    ) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let inner = value::bind(
            &inner?.add_to("(*boltffi_encoded_size)"),
            binder,
            &format!("({value}.value)"),
        );
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += 1;\nif ({value}.has_value) {{\n{inner}\n}}"
        )))
    }

    fn sequence(
        &mut self,
        reference: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let statements = match element? {
            EncodedSize::Fixed(bytes) => {
                format!("(*boltffi_encoded_size) += 4 + {value}.len * {bytes};")
            }
            EncodedSize::Variable(element) => {
                let index = format!("boltffi_index_{}", binder.raw());
                let element = value::bind(&element, binder, &format!("({value}.ptr[{index}])"));
                format!(
                    "(*boltffi_encoded_size) += 4;\nfor (uintptr_t {index} = 0; {index} < {value}.len; ++{index}) {{\n{element}\n}}"
                )
            }
        };
        Ok(EncodedSize::Variable(statements))
    }

    fn tuple(&mut self, _: &ValueRef, elements: Vec<Self::Expr>) -> Self::Expr {
        elements
            .into_iter()
            .try_fold(EncodedSize::Fixed(0), |total, element| {
                Ok(match (total, element?) {
                    (EncodedSize::Fixed(total), EncodedSize::Fixed(bytes)) => {
                        EncodedSize::Fixed(total + bytes)
                    }
                    (total, element) => EncodedSize::Variable(format!(
                        "{}\n{}",
                        total.add_to("(*boltffi_encoded_size)"),
                        element.add_to("(*boltffi_encoded_size)")
                    )),
                })
            })
    }

    fn result(
        &mut self,
        reference: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let ok = value::bind(
            &ok?.add_to("(*boltffi_encoded_size)"),
            binder,
            &format!("({value}.data.value)"),
        );
        let error = value::bind(
            &err?.add_to("(*boltffi_encoded_size)"),
            binder,
            &format!("({value}.data.error)"),
        );
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += 1;\nif ({value}.ok) {{\n{ok}\n}} else {{\n{error}\n}}"
        )))
    }

    fn map(
        &mut self,
        _: MapKind,
        reference: &ValueRef,
        key_binder: BinderId,
        key: Self::Expr,
        value_binder: BinderId,
        map_value: Self::Expr,
    ) -> Self::Expr {
        let value = value::reference(reference, &self.root)?;
        let index = format!("boltffi_index_{}", key_binder.raw());
        let key = value::bind(
            &key?.add_to("(*boltffi_encoded_size)"),
            key_binder,
            &format!("({value}.ptr[{index}].key)"),
        );
        let map_value = value::bind(
            &map_value?.add_to("(*boltffi_encoded_size)"),
            value_binder,
            &format!("({value}.ptr[{index}].value)"),
        );
        Ok(EncodedSize::Variable(format!(
            "(*boltffi_encoded_size) += 4;\nfor (uintptr_t {index} = 0; {index} < {value}.len; ++{index}) {{\n{key}\n{map_value}\n}}"
        )))
    }
}
