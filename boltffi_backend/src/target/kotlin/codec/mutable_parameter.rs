use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind, Op,
    Primitive, ReadPlan, RecordId,
};

use crate::{core::Result, target::kotlin::KotlinHost};

pub struct MutableParameter;

pub enum MutableParameterValue {
    Copyable,
    PrimitiveElement,
}

impl MutableParameter {
    pub fn validate(plan: &ReadPlan) -> Result<()> {
        let mut renderer = Self;
        match plan.render_with(&mut renderer)? {
            MutableParameterValue::Copyable => Ok(()),
            MutableParameterValue::PrimitiveElement => {
                Err(KotlinHost::unsupported("mutable encoded parameter"))
            }
        }
    }

    fn unsupported() -> Result<MutableParameterValue> {
        Err(KotlinHost::unsupported("mutable encoded parameter"))
    }
}

impl CodecRead for MutableParameter {
    type Expr = Result<MutableParameterValue>;

    fn primitive(&mut self, _primitive: Primitive) -> Self::Expr {
        Ok(MutableParameterValue::PrimitiveElement)
    }

    fn string(&mut self) -> Self::Expr {
        Self::unsupported()
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        // Kotlin does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec read reached Kotlin mutable-parameter renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(MutableParameterValue::Copyable)
    }

    fn direct_record(&mut self, _id: RecordId) -> Self::Expr {
        Self::unsupported()
    }

    fn encoded_record(&mut self, _id: RecordId) -> Self::Expr {
        Self::unsupported()
    }

    fn c_style_enum(&mut self, _id: EnumId) -> Self::Expr {
        Self::unsupported()
    }

    fn data_enum(&mut self, _id: EnumId) -> Self::Expr {
        Self::unsupported()
    }

    fn class_handle(&mut self, _id: ClassId) -> Self::Expr {
        Self::unsupported()
    }

    fn callback_handle(&mut self, _id: CallbackId) -> Self::Expr {
        Self::unsupported()
    }

    fn custom(&mut self, _id: CustomTypeId, _representation: Self::Expr) -> Self::Expr {
        Self::unsupported()
    }

    fn builtin(&mut self, _kind: BuiltinType) -> Self::Expr {
        Self::unsupported()
    }

    fn optional(&mut self, _inner: Self::Expr) -> Self::Expr {
        Self::unsupported()
    }

    fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        match element? {
            MutableParameterValue::PrimitiveElement => Ok(MutableParameterValue::Copyable),
            MutableParameterValue::Copyable => Self::unsupported(),
        }
    }

    fn tuple(&mut self, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported()
    }

    fn result(&mut self, _ok: Self::Expr, _err: Self::Expr) -> Self::Expr {
        Self::unsupported()
    }

    fn map(&mut self, _kind: MapKind, _key: Self::Expr, _value: Self::Expr) -> Self::Expr {
        Self::unsupported()
    }
}
