use boltffi_binding::{DirectVectorElementType, Native};

use crate::{
    bridge::{
        c::{Expression, Identifier, TypeFragment},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::cpython::render::{direct, primitive},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Element {
    slot: direct::NativeSlot,
    vector_boxer: Identifier,
    vector_encoder: Identifier,
    vector_parser: Identifier,
    vector_decoder: Identifier,
}

impl Element {
    pub fn from_element(
        element: &DirectVectorElementType,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let slot = match element {
            DirectVectorElementType::Primitive(primitive) => {
                direct::NativeSlot::from_primitive(primitive.primitive())?
            }
            DirectVectorElementType::Record(record) => {
                direct::NativeSlot::from_record_id(*record, bridge, context)?
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct vector element",
                });
            }
        };
        Self::from_native_slot(slot)
    }

    pub fn c_type(&self) -> &TypeFragment {
        self.slot.c_type()
    }

    pub fn parser(&self) -> &Identifier {
        self.slot.parser()
    }

    pub fn boxer(&self) -> &Identifier {
        self.slot.boxer()
    }

    pub fn vector_boxer(&self) -> &Identifier {
        &self.vector_boxer
    }

    pub fn vector_encoder(&self) -> &Identifier {
        &self.vector_encoder
    }

    pub fn vector_parser(&self) -> &Identifier {
        &self.vector_parser
    }

    pub fn vector_decoder(&self) -> &Identifier {
        &self.vector_decoder
    }

    /// Returns the conversion that readies a Python sequence for the C call.
    ///
    /// A primitive element crosses as a typed pointer with an element count,
    /// so the parser leaves the count untouched. A record element crosses as
    /// packed bytes with a byte length, so the encoder scales the count by
    /// the element size. The split mirrors the C bridge parameter contract.
    pub fn argument_parser(&self) -> &Identifier {
        match self.slot.primitive() {
            Some(_) => &self.vector_parser,
            None => &self.vector_encoder,
        }
    }

    /// Returns the pointer and length expressions the C call receives.
    pub fn argument_expressions(&self, pointer: Identifier, length: Identifier) -> Vec<Expression> {
        let length = Expression::identifier(length);
        match self.slot.primitive() {
            Some(_) => vec![
                Expression::cast(
                    TypeFragment::new(format!("const {} *", self.c_type())),
                    Expression::identifier(pointer),
                ),
                length,
            ],
            None => vec![Expression::identifier(pointer), length],
        }
    }

    pub fn runtime_primitive(&self) -> Option<primitive::Runtime> {
        self.slot.primitive()
    }

    fn from_native_slot(slot: direct::NativeSlot) -> Result<Self> {
        let stem = slot.stem();
        let vector_parser = match slot.primitive() {
            Some(primitive) => primitive.direct_vec_parser()?,
            None => Identifier::parse(format!("boltffi_python_parse_vec_{stem}"))?,
        };
        let vector_decoder = match slot.primitive() {
            Some(primitive) => primitive.direct_vec_decoder()?,
            None => Identifier::parse(format!("boltffi_python_decode_owned_vec_{stem}"))?,
        };
        Ok(Self {
            vector_boxer: Identifier::parse(format!("boltffi_python_box_vec_{stem}"))?,
            vector_encoder: Identifier::parse(format!("boltffi_python_wire_vec_{stem}"))?,
            vector_parser,
            vector_decoder,
            slot,
        })
    }
}
