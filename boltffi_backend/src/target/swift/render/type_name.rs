use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, EnumId, Native, Primitive, RecordId, TypeRef,
    TypeRefRender, native,
};

use crate::{
    bridge::c::{CBridgeContract, Type as CType},
    core::{Error, RenderContext, Result},
    target::swift::{SwiftHost, name_style::Name, primitive::SwiftPrimitive, syntax::TypeName},
};

pub struct SwiftType;

impl SwiftType {
    pub fn primitive(primitive: Primitive) -> Result<TypeName> {
        SwiftPrimitive::new(primitive).api_type()
    }

    pub fn record(id: RecordId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .record(id)
            .map(|record| Name::new(record.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing record type in Swift render context",
            })
    }

    pub fn enumeration(id: EnumId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .enumeration(id)
            .map(|enumeration| Name::new(enumeration.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing enum type in Swift render context",
            })
    }

    pub fn class(id: ClassId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .class(id)
            .map(|class| Name::new(class.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing class type in Swift render context",
            })
    }

    pub fn callback(id: CallbackId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .callback(id)
            .map(|callback| Name::new(callback.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing callback type in Swift render context",
            })
    }

    pub fn custom(id: CustomTypeId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .custom_type(id)
            .map(|custom_type| Name::new(custom_type.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing custom type in Swift render context",
            })
    }

    pub fn handle_carrier(carrier: native::HandleCarrier) -> Result<TypeName> {
        match carrier {
            native::HandleCarrier::U64 => Ok(TypeName::uint64()),
            native::HandleCarrier::USize => Ok(TypeName::uint()),
            native::HandleCarrier::CallbackHandle => Ok(TypeName::new("BoltFFICallbackHandle")),
            _ => Err(SwiftHost::unsupported("unknown handle carrier")),
        }
    }

    pub fn direct_record_storage(id: RecordId, bridge: &CBridgeContract) -> Result<TypeName> {
        bridge
            .source_direct_record(id)
            .map(|record| TypeName::new(record.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing C direct record for Swift storage type",
            })
    }

    pub fn c_style_enum_storage(id: EnumId, bridge: &CBridgeContract) -> Result<TypeName> {
        bridge
            .source_c_style_enum(id)
            .map(|enumeration| TypeName::new(enumeration.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing C enum for Swift storage type",
            })
    }

    pub fn c_type(ty: &CType) -> Result<TypeName> {
        match ty {
            CType::Void => Ok(TypeName::void()),
            CType::Bool => Ok(TypeName::bool()),
            CType::Int8 => Ok(TypeName::int8()),
            CType::Uint8 => Ok(TypeName::uint8()),
            CType::Int16 => Ok(TypeName::int16()),
            CType::Uint16 => Ok(TypeName::uint16()),
            CType::Int32 => Ok(TypeName::int32()),
            CType::Uint32 => Ok(TypeName::uint32()),
            CType::Int64 => Ok(TypeName::int64()),
            CType::Uint64 => Ok(TypeName::uint64()),
            CType::Float32 => Ok(TypeName::float()),
            CType::Float64 => Ok(TypeName::double()),
            CType::SignedPointerWidth => Ok(TypeName::int()),
            CType::PointerWidth => Ok(TypeName::uint()),
            CType::Status => Ok(TypeName::new("FfiStatus")),
            CType::Buffer => Ok(TypeName::new("FfiBuf_u8")),
            CType::String => Ok(TypeName::new("FfiString")),
            CType::Span => Ok(TypeName::new("FfiSpan")),
            CType::FutureHandle => Ok(TypeName::new("RustFutureHandle")),
            CType::StreamPollResult => Ok(TypeName::new("StreamPollResult")),
            CType::WaitResult => Ok(TypeName::new("WaitResult")),
            CType::CallbackHandle(_) => Ok(TypeName::new("BoltFFICallbackHandle")),
            CType::Named(name) | CType::DirectRecord(name) => Ok(TypeName::new(name.as_str())),
            CType::CStyleEnum { name, .. } => Ok(TypeName::new(name.as_str())),
            CType::ConstPointer(inner) => Self::c_pointer(inner, false),
            CType::MutPointer(inner) => Self::c_pointer(inner, true),
            CType::FunctionPointer { returns, params } => {
                let params = params
                    .iter()
                    .map(Self::c_type)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(|param| param.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(TypeName::new(format!(
                    "@convention(c) ({params}) -> {}",
                    Self::c_type(returns)?
                )))
            }
        }
    }

    pub fn type_ref(ty: &TypeRef, context: &RenderContext<Native>) -> Result<TypeName> {
        ty.render_with(&mut SwiftTypeRef { context })
            .map(TypeSpelling::into_value)
    }

    fn builtin(kind: BuiltinType) -> TypeName {
        match kind {
            BuiltinType::Duration => TypeName::double(),
            BuiltinType::SystemTime => TypeName::new("Date"),
            BuiltinType::Uuid => TypeName::new("UUID"),
            BuiltinType::Url => TypeName::new("URL"),
        }
    }

    fn c_pointer(inner: &CType, mutable: bool) -> Result<TypeName> {
        match (mutable, inner) {
            (true, CType::Void) => Ok(TypeName::new("UnsafeMutableRawPointer?")),
            (false, CType::Void) => Ok(TypeName::new("UnsafeRawPointer?")),
            (true, inner) => Ok(Self::c_type(inner)?.mutable_pointer()),
            (false, inner) => Ok(Self::c_type(inner)?.pointer()),
        }
    }
}

struct SwiftTypeRef<'context> {
    context: &'context RenderContext<'context, Native>,
}

struct TypeSpelling {
    value: TypeName,
    result_failure: Option<TypeName>,
}

impl TypeSpelling {
    fn value(value: TypeName) -> Self {
        Self {
            value,
            result_failure: None,
        }
    }

    fn error(value: TypeName) -> Self {
        Self {
            value: value.clone(),
            result_failure: Some(value),
        }
    }

    fn string() -> Self {
        Self {
            value: TypeName::string(),
            result_failure: Some(TypeName::new("FfiError")),
        }
    }

    fn into_value(self) -> TypeName {
        self.value
    }

    fn into_result_failure(self) -> Result<TypeName> {
        self.result_failure
            .ok_or(SwiftHost::unsupported("Swift Result failure type"))
    }
}

impl TypeRefRender for SwiftTypeRef<'_> {
    type Output = Result<TypeSpelling>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        SwiftType::primitive(primitive).map(TypeSpelling::value)
    }

    fn string(&mut self) -> Self::Output {
        Ok(TypeSpelling::string())
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        // Swift does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString type ref reached Swift renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(TypeSpelling::value(TypeName::data()))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        let ty = SwiftType::record(id, self.context)?;
        match self
            .context
            .record(id)
            .is_some_and(|record| record.is_error_payload())
        {
            true => Ok(TypeSpelling::error(ty)),
            false => Ok(TypeSpelling::value(ty)),
        }
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        let ty = SwiftType::enumeration(id, self.context)?;
        match self
            .context
            .enumeration(id)
            .is_some_and(|enumeration| enumeration.is_error_payload())
        {
            true => Ok(TypeSpelling::error(ty)),
            false => Ok(TypeSpelling::value(ty)),
        }
    }

    fn class(&mut self, id: ClassId) -> Self::Output {
        SwiftType::class(id, self.context).map(TypeSpelling::value)
    }

    fn callback(&mut self, id: CallbackId) -> Self::Output {
        SwiftType::callback(id, self.context).map(TypeSpelling::value)
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        if let Some(mapping) = self.context.custom_type_mapping(id) {
            return Ok(TypeSpelling::value(SwiftHost::custom_type_name(mapping)));
        }

        SwiftType::custom(id, self.context).map(TypeSpelling::value)
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(TypeSpelling::value(SwiftType::builtin(kind)))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        inner.map(|inner| TypeSpelling::value(inner.into_value().optional()))
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        element.map(|element| TypeSpelling::value(TypeName::array(element.into_value())))
    }

    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output {
        elements
            .into_iter()
            .collect::<Result<Vec<_>>>()
            .map(|elements| {
                TypeSpelling::value(TypeName::tuple(
                    elements.into_iter().map(TypeSpelling::into_value),
                ))
            })
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        Ok(TypeSpelling::value(TypeName::result(
            ok?.into_value(),
            err?.into_result_failure()?,
        )))
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        Ok(TypeSpelling::value(TypeName::dictionary(
            key?.into_value(),
            value?.into_value(),
        )))
    }
}
