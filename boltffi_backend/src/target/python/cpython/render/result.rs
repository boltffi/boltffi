use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, HandlePresence, HandleTarget, Native,
    OutOfRust, Primitive, ReadPlan, RecordId, ReturnPlan, ReturnPlanRender, ReturnValueSlot,
    TypeRef, native,
};

use crate::{
    bridge::{c::Identifier, python_cext::PythonCExtBridgeContract},
    core::{Error, RenderContext, Result},
    target::python::cpython::render::{direct, direct_vector, primitive},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conversion {
    pub void: bool,
    converter: Option<Identifier>,
    primitive: Option<primitive::Runtime>,
    owned_buffer: Option<OwnedBuffer>,
}

impl Conversion {
    pub fn from_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut Renderer { bridge, context })
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn owned_buffer(&self) -> Option<OwnedBuffer> {
        self.owned_buffer.clone()
    }

    pub fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        match &self.owned_buffer {
            Some(OwnedBuffer::DirectVector(element)) => Some((**element).clone()),
            Some(OwnedBuffer::RawWire | OwnedBuffer::OptionalPrimitive(_)) | None => None,
        }
    }

    pub fn is_void(&self) -> bool {
        self.void
    }

    pub fn converter(&self) -> &Identifier {
        self.converter
            .as_ref()
            .expect("non-void return conversion has a converter")
    }

    pub fn from_owned_buffer(buffer: OwnedBuffer) -> Result<Self> {
        Ok(Self {
            void: false,
            converter: Some(buffer.converter()?),
            primitive: buffer.primitive(),
            owned_buffer: Some(buffer),
        })
    }
}

struct Renderer<'render> {
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'render> Renderer<'render> {
    fn direct_type(&self, ty: &DirectValueType) -> Result<Conversion> {
        let direct = direct::NativeSlot::from_direct_value(ty, self.bridge, self.context)?;
        Ok(Conversion {
            void: false,
            converter: Some(direct.boxer().clone()),
            primitive: direct.primitive(),
            owned_buffer: None,
        })
    }

    fn handle_type(
        &self,
        slot: ReturnValueSlot,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Result<Conversion> {
        match (target, presence) {
            (HandleTarget::Class(_), HandlePresence::Required) => {
                let carrier = primitive::Runtime::native_handle(carrier)?;
                Ok(Conversion {
                    void: false,
                    converter: Some(carrier.boxer()?),
                    primitive: Some(carrier),
                    owned_buffer: None,
                })
            }
            (HandleTarget::Callback(_), _) => Ok(Conversion {
                void: false,
                converter: Some(Identifier::parse("boltffi_python_box_callback_handle")?),
                primitive: None,
                owned_buffer: None,
            }),
            (HandleTarget::Class(_), HandlePresence::Nullable) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: slot.unsupported_nullable_class_shape(),
            }),
            (HandleTarget::Stream(_), _) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "stream handle return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown handle return",
            }),
        }
    }
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, OutOfRust> for Renderer<'render> {
    type Output = Result<Conversion>;

    fn void(&mut self) -> Self::Output {
        Ok(Conversion {
            void: true,
            converter: None,
            primitive: None,
            owned_buffer: None,
        })
    }

    fn direct(&mut self, _: ReturnValueSlot, ty: &DirectValueType) -> Self::Output {
        self.direct_type(ty)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &TypeRef,
        _: &ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => Conversion::from_owned_buffer(OwnedBuffer::RawWire),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: slot.unsupported_encoded_shape(),
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        self.handle_type(slot, target, carrier, presence)
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Conversion::from_owned_buffer(OwnedBuffer::OptionalPrimitive(primitive::Runtime::new(
            primitive,
        )))
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType) -> Self::Output {
        Conversion::from_owned_buffer(OwnedBuffer::DirectVector(Box::new(
            direct_vector::Element::from_element(element, self.bridge, self.context)?,
        )))
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "closure return",
        })
    }

    fn native_opaque_record(&mut self, record: RecordId) -> Self::Output {
        use crate::bridge::c::Identifier;
        use crate::target::python::name_style::Name;
        // Look up the record name to construct the boxer symbol
        let record_decl = self
            .context
            .record(record)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "native opaque record without declaration",
            })?;
        let stem = Identifier::escape(Name::new(record_decl.name()).function_text()?)?.to_string();
        let boxer = Identifier::parse(format!("boltffi_python_box_opaque_{stem}"))?;
        // The boxer is a static C function generated by the native_opaque_record.c template.
        // No primitive runtime helper is needed.
        Ok(Conversion {
            void: false,
            converter: Some(boxer),
            primitive: None,
            owned_buffer: None,
        })
    }
}

trait ReturnValueSlotMessage {
    fn unsupported_encoded_shape(self) -> &'static str
    where
        Self: Sized,
    {
        self.unsupported_return_shape()
    }

    fn unsupported_nullable_class_shape(self) -> &'static str;

    fn unsupported_return_shape(self) -> &'static str;
}

impl ReturnValueSlotMessage for ReturnValueSlot {
    fn unsupported_nullable_class_shape(self) -> &'static str {
        match self {
            Self::ReturnSlot => "nullable class handle return",
            Self::OutPointer => "fallible nullable class handle success",
            _ => "nullable class handle return",
        }
    }

    fn unsupported_return_shape(self) -> &'static str {
        match self {
            Self::ReturnSlot => "non-primitive return",
            Self::OutPointer => "fallible success return",
            _ => "unknown return plan",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OwnedBuffer {
    RawWire,
    DirectVector(Box<direct_vector::Element>),
    OptionalPrimitive(primitive::Runtime),
}

impl OwnedBuffer {
    pub fn converter(&self) -> Result<Identifier> {
        match self {
            Self::RawWire => Identifier::parse("boltffi_python_decode_owned_raw_wire"),
            Self::DirectVector(element) => Ok(element.vector_decoder().clone()),
            Self::OptionalPrimitive(primitive) => primitive.optional_owned_wire_decoder(),
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        match self {
            Self::OptionalPrimitive(primitive) => Some(*primitive),
            Self::RawWire | Self::DirectVector(_) => None,
        }
    }
}
