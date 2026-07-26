use crate::{
    bridge::c::{ArgumentList, Expression, Identifier, TypeFragment},
    core::{Error, Result},
    target::python::cpython::{
        codec,
        render::{direct_vector, primitive, result},
    },
};

#[derive(Clone)]
pub enum BufferedArgument {
    OptionalPrimitive(primitive::Runtime),
    RegisteredObject(RegisteredObject),
    RawWire,
    Utf8Text,
    DirectVector(direct_vector::Element),
    Native(codec::NativeCodec),
}

impl BufferedArgument {
    pub fn parser(&self) -> Result<Identifier> {
        match self {
            Self::OptionalPrimitive(primitive) => primitive.optional_wire_encoder(),
            Self::RegisteredObject(registered) => Ok(registered.parser.clone()),
            Self::RawWire => Identifier::parse("boltffi_python_wire_raw"),
            Self::Utf8Text => Identifier::parse("boltffi_python_wire_string"),
            Self::DirectVector(element) => Ok(element.argument_parser().clone()),
            Self::Native(codec) => Ok(codec.encoder().clone()),
        }
    }

    pub fn call_args(
        &self,
        pointer: &Identifier,
        length: &Identifier,
        mutation: Option<&MutationOutput>,
    ) -> Result<Vec<Expression>> {
        match self {
            Self::DirectVector(element) if mutation.is_some() => {
                debug_assert!(
                    element.runtime_primitive().is_some(),
                    "mutable packed-record vectors are rejected during argument construction"
                );
                Ok(vec![
                    Expression::cast(
                        TypeFragment::new(format!("{} *", element.c_type())),
                        Expression::identifier(pointer.clone()),
                    ),
                    Expression::identifier(length.clone()),
                ])
            }
            Self::DirectVector(element) => {
                Ok(element.argument_expressions(pointer.clone(), length.clone()))
            }
            Self::OptionalPrimitive(_)
            | Self::RegisteredObject(_)
            | Self::RawWire
            | Self::Utf8Text
            | Self::Native(_) => Ok([pointer, length]
                .into_iter()
                .cloned()
                .map(Expression::identifier)
                .chain(
                    mutation
                        .and_then(MutationOutput::buffer)
                        .cloned()
                        .map(Expression::identifier)
                        .map(Expression::address_of),
                )
                .collect()),
        }
    }

    pub fn mutation_output(
        &self,
        name: &Identifier,
        pointer: &Identifier,
        length: &Identifier,
    ) -> Result<Option<MutationOutput>> {
        match self {
            Self::RegisteredObject(registered) => Ok(Some(MutationOutput::new(
                Identifier::parse(format!("{name}_out"))?,
                registered.owned_decoder.clone(),
                None,
            ))),
            Self::RawWire => Ok(Some(MutationOutput::new(
                Identifier::parse(format!("{name}_out"))?,
                result::OwnedBuffer::RawWire.converter()?,
                Some(result::OwnedBuffer::RawWire),
            ))),
            Self::DirectVector(element) if element.runtime_primitive().is_some() => Ok(Some(
                MutationOutput::direct_vector(pointer.clone(), length.clone(), element),
            )),
            Self::OptionalPrimitive(_)
            | Self::Utf8Text
            | Self::DirectVector(_)
            | Self::Native(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter",
            }),
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        match self {
            Self::OptionalPrimitive(primitive) => Some(*primitive),
            Self::RegisteredObject(_)
            | Self::RawWire
            | Self::Utf8Text
            | Self::DirectVector(_)
            | Self::Native(_) => None,
        }
    }

    pub fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        match self {
            Self::DirectVector(element) => Some(element.clone()),
            Self::OptionalPrimitive(_)
            | Self::RegisteredObject(_)
            | Self::RawWire
            | Self::Utf8Text
            | Self::Native(_) => None,
        }
    }

    pub fn native_sequence(&self) -> Option<codec::NativeSequence> {
        match self {
            Self::Native(codec) => codec.sequence().cloned(),
            Self::OptionalPrimitive(_)
            | Self::RegisteredObject(_)
            | Self::RawWire
            | Self::Utf8Text
            | Self::DirectVector(_) => None,
        }
    }

    pub fn is_raw_wire(&self) -> bool {
        matches!(self, Self::RawWire)
    }

    pub fn is_utf8_text(&self) -> bool {
        matches!(self, Self::Utf8Text)
    }
}

#[derive(Clone)]
pub struct RegisteredObject {
    parser: Identifier,
    owned_decoder: Identifier,
}

impl RegisteredObject {
    pub fn new(parser: Identifier, owned_decoder: Identifier) -> Self {
        Self {
            parser,
            owned_decoder,
        }
    }
}

#[derive(Clone)]
pub struct MutationOutput {
    buffer: Option<Identifier>,
    conversion: Expression,
    owned_buffer: Option<result::OwnedBuffer>,
}

impl MutationOutput {
    fn new(
        buffer: Identifier,
        decoder: Identifier,
        owned_buffer: Option<result::OwnedBuffer>,
    ) -> Self {
        Self {
            conversion: Expression::call(
                decoder,
                ArgumentList::from_iter([Expression::identifier(buffer.clone())]),
            ),
            buffer: Some(buffer),
            owned_buffer,
        }
    }

    pub(super) fn from_boxer(buffer: Identifier, boxer: Identifier) -> Self {
        Self::new(buffer, boxer, None)
    }

    fn direct_vector(
        pointer: Identifier,
        length: Identifier,
        element: &direct_vector::Element,
    ) -> Self {
        Self {
            buffer: None,
            conversion: Expression::call(
                element.vector_boxer().clone(),
                ArgumentList::from_iter([
                    Expression::cast(
                        TypeFragment::new(format!("const {} *", element.c_type())),
                        Expression::identifier(pointer),
                    ),
                    Expression::identifier(length),
                ]),
            ),
            owned_buffer: None,
        }
    }

    pub fn buffer(&self) -> Option<&Identifier> {
        self.buffer.as_ref()
    }

    pub fn conversion(&self) -> &Expression {
        &self.conversion
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.owned_buffer.clone()
    }
}
