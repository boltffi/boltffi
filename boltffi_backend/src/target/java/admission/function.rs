use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, Direction, ExecutionDecl,
    ExportedCallable, FunctionDecl, HandlePresence, HandleTarget, IntoRust, Native, OutOfRust,
    ParamPlanRender, Primitive as BindingPrimitive, Receive, ReturnPlanRender, ReturnValueSlot,
    TypeRef, native,
};

use crate::{
    core::{Error, Result},
    target::{
        java::{JavaHost, primitive::Primitive},
        jvm::method::{Parameter as JvmParameter, Parameters as JvmParameters, SlotWidth},
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FunctionShape {
    Supported,
    Receiver,
    AsyncProtocol,
    ParameterSlots,
    PrimitiveParameter,
    UnknownDirectParameter,
    EncodedParameter,
    MutableEncodedParameter,
    HandleParameter,
    PrimitiveReturn,
    OutPointerPrimitiveReturn,
    DirectEnumReturn,
    UnknownDirectReturn,
    HandleReturn,
    ScalarOptionReturn,
    ClosureReturn,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReceiverSupport {
    Forbidden,
    Direct,
    Encoded,
    Handle(native::HandleCarrier),
}

struct ParameterShape;
struct ReturnShape;

#[derive(Clone, Copy)]
struct CarrierWidth(SlotWidth);

impl JvmParameter for CarrierWidth {
    fn slot_width(&self) -> SlotWidth {
        self.0
    }
}

impl FunctionShape {
    pub fn classify(declaration: &FunctionDecl<Native>) -> Self {
        let callable = declaration.callable();
        Self::classify_callable(callable, ReceiverSupport::Forbidden)
    }

    pub fn classify_callable(
        callable: &ExportedCallable<Native>,
        receiver_support: ReceiverSupport,
    ) -> Self {
        let receiver = callable.receiver();
        let parameter =
            receiver
                .filter(|_| !matches!(receiver_support, ReceiverSupport::Forbidden))
                .map(|_| {
                    Ok(match receiver_support {
                        ReceiverSupport::Direct => vec![CarrierWidth(SlotWidth::Single)],
                        ReceiverSupport::Encoded => vec![
                            CarrierWidth(SlotWidth::Single),
                            CarrierWidth(SlotWidth::Single),
                        ],
                        ReceiverSupport::Handle(carrier) => {
                            return Primitive::from_handle_carrier(carrier)
                                .map(|primitive| vec![CarrierWidth(primitive.slot_width())])
                                .map_err(|_| Self::HandleParameter);
                        }
                        ReceiverSupport::Forbidden => Vec::new(),
                    })
                })
                .into_iter()
                .chain(callable.params().iter().map(
                    |parameter| match parameter.payload().as_value() {
                        Some(plan) => plan.render_with(&mut ParameterShape),
                        None => Ok(vec![CarrierWidth(SlotWidth::Double)]),
                    },
                ))
                .collect::<std::result::Result<Vec<_>, _>>()
                .and_then(|parameters| {
                    JvmParameters::for_static(parameters.into_iter().flatten().collect())
                        .map_err(|_| Self::ParameterSlots)
                })
                .err();
        let returns = callable.returns().plan().render_with(&mut ReturnShape);
        [
            (receiver.is_some() && matches!(receiver_support, ReceiverSupport::Forbidden))
                .then_some(Self::Receiver),
            matches!(
                callable.execution(),
                ExecutionDecl::Asynchronous(protocol)
                    if !matches!(protocol, native::AsyncProtocol::PollHandle { .. })
            )
            .then_some(Self::AsyncProtocol),
            parameter,
            (!returns.is_supported()).then_some(returns),
        ]
        .into_iter()
        .flatten()
        .next()
        .unwrap_or(Self::Supported)
    }

    pub fn require_supported(self) -> Result<()> {
        self.unsupported_reason().map_or(Ok(()), |shape| {
            Err(Error::UnsupportedTarget {
                target: match self {
                    Self::ParameterSlots => "jvm",
                    _ => "java",
                },
                shape,
            })
        })
    }

    pub fn unexpected_shape() -> Error {
        JavaHost::broken_bridge_contract(
            "Java function shape is admitted before signature rendering",
        )
    }

    pub const fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            Self::Supported => None,
            Self::Receiver => Some("free function receiver"),
            Self::AsyncProtocol => Some("asynchronous function protocol"),
            Self::ParameterSlots => Some("method parameter slots exceed 255 units"),
            Self::PrimitiveParameter => Some("primitive Java representation"),
            Self::UnknownDirectParameter => Some("unknown direct function parameter"),
            Self::EncodedParameter => Some("encoded function parameter"),
            Self::MutableEncodedParameter => Some("mutable encoded function parameter"),
            Self::HandleParameter => Some("handle function parameter"),
            Self::PrimitiveReturn => Some("primitive Java representation"),
            Self::OutPointerPrimitiveReturn => Some("out-pointer primitive function return"),
            Self::DirectEnumReturn => Some("direct enum function return"),
            Self::UnknownDirectReturn => Some("unknown direct function return"),
            Self::HandleReturn => Some("handle function return"),
            Self::ScalarOptionReturn => Some("scalar option function return"),
            Self::ClosureReturn => Some("closure function return"),
        }
    }

    const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterShape {
    type Output = std::result::Result<Vec<CarrierWidth>, FunctionShape>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: Receive) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => Primitive::try_from(*primitive)
                .map(|primitive| vec![CarrierWidth(primitive.slot_width())])
                .map_err(|_| FunctionShape::PrimitiveParameter),
            DirectValueType::Record(_) => Ok(vec![CarrierWidth(SlotWidth::Single)]),
            DirectValueType::Enum(_) => Ok(vec![CarrierWidth(SlotWidth::Single)]),
            _ => Err(FunctionShape::UnknownDirectParameter),
        }
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan <IntoRust as Direction>::Codec,
        shape: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        if receive == Receive::ByMutRef {
            return Err(FunctionShape::MutableEncodedParameter);
        }
        match shape {
            native::BufferShape::Slice => Ok(vec![
                CarrierWidth(SlotWidth::Single),
                CarrierWidth(SlotWidth::Single),
            ]),
            _ => Err(FunctionShape::EncodedParameter),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match (target, presence) {
            (
                HandleTarget::Class(_) | HandleTarget::Callback(_),
                HandlePresence::Required | HandlePresence::Nullable,
            ) => Primitive::from_handle_carrier(carrier)
                .map(|primitive| vec![CarrierWidth(primitive.slot_width())])
                .map_err(|_| FunctionShape::HandleParameter),
            _ => Err(FunctionShape::HandleParameter),
        }
    }

    fn scalar_option(&mut self, _: BindingPrimitive) -> Self::Output {
        Ok(vec![
            CarrierWidth(SlotWidth::Single),
            CarrierWidth(SlotWidth::Single),
        ])
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType, _: Receive) -> Self::Output {
        Ok(vec![CarrierWidth(SlotWidth::Single)])
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnShape {
    type Output = FunctionShape;

    fn void(&mut self) -> Self::Output {
        FunctionShape::Supported
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match (slot, ty) {
            (ReturnValueSlot::ReturnSlot, DirectValueType::Primitive(primitive))
                if Primitive::try_from(*primitive).is_ok() =>
            {
                FunctionShape::Supported
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Primitive(_)) => {
                FunctionShape::PrimitiveReturn
            }
            (ReturnValueSlot::OutPointer, DirectValueType::Primitive(primitive))
                if Primitive::try_from(*primitive).is_ok() =>
            {
                FunctionShape::Supported
            }
            (ReturnValueSlot::OutPointer, DirectValueType::Primitive(_)) => {
                FunctionShape::OutPointerPrimitiveReturn
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Record(_)) => FunctionShape::Supported,
            (ReturnValueSlot::OutPointer, DirectValueType::Record(_)) => FunctionShape::Supported,
            (ReturnValueSlot::ReturnSlot, DirectValueType::Enum(_)) => FunctionShape::Supported,
            (ReturnValueSlot::OutPointer, DirectValueType::Enum(_)) => {
                FunctionShape::DirectEnumReturn
            }
            _ => FunctionShape::UnknownDirectReturn,
        }
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan <OutOfRust as Direction>::Codec,
        _: native::BufferShape,
    ) -> Self::Output {
        FunctionShape::Supported
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match (slot, target, presence) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                HandleTarget::Class(_) | HandleTarget::Callback(_),
                HandlePresence::Required | HandlePresence::Nullable,
            ) if Primitive::from_handle_carrier(carrier).is_ok() => FunctionShape::Supported,
            _ => FunctionShape::HandleReturn,
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        match Primitive::try_from(primitive).is_ok() {
            true => FunctionShape::Supported,
            false => FunctionShape::ScalarOptionReturn,
        }
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        FunctionShape::Supported
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
        FunctionShape::ClosureReturn
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{DeclarationRef, Native, lower};

    use super::FunctionShape;

    #[test]
    fn classifies_the_complete_primitive_function_boundary() {
        let file = syn::parse_str(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub value: i32,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            pub struct Engine {
                value: i32,
            }

            #[export]
            impl Engine {
                pub fn new(value: i32) -> Self {
                    Self { value }
                }
            }

            #[export]
            pub fn primitive(value: i32) -> i32 { value }

            #[export]
            pub fn encoded_parameter(value: String) {}

            #[export]
            pub fn record_parameter(value: Point) {}

            #[export]
            pub fn enum_parameter(value: Mode) {}

            #[export]
            pub fn handle_parameter(value: Engine) {}

            #[export]
            pub fn option_parameter(value: Option<i32>) {}

            #[export]
            pub fn vector_parameter(value: Vec<i32>) {}

            #[export]
            pub fn encoded_return() -> String { unimplemented!() }

            #[export]
            pub fn record_return() -> Point { unimplemented!() }

            #[export]
            pub fn enum_return() -> Mode { unimplemented!() }

            #[export]
            pub fn handle_return() -> Engine { unimplemented!() }

            #[export]
            pub fn option_return() -> Option<i32> { None }

            #[export]
            pub fn vector_return() -> Vec<i32> { Vec::new() }
            "#,
        )
        .expect("valid Java function boundary fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("Java function boundary fixture scans");
        let bindings = lower::<Native>(&source).expect("Java function boundary fixture lowers");
        let shapes = bindings
            .decls()
            .iter()
            .filter_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function) => Some((
                    function
                        .name()
                        .source_spelling()
                        .expect("function source spelling"),
                    FunctionShape::classify(function).unsupported_reason(),
                )),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            shapes,
            BTreeMap::from([
                ("encoded_parameter", None),
                ("encoded_return", None),
                ("enum_parameter", None),
                ("enum_return", None),
                ("handle_parameter", None),
                ("handle_return", None),
                ("option_parameter", None),
                ("option_return", None),
                ("primitive", None),
                ("record_parameter", None),
                ("record_return", None),
                ("vector_parameter", None),
                ("vector_return", None),
            ])
        );
    }
}
