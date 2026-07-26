use boltffi_binding::{
    CodecNode, EncodedFieldDecl, EncodedRecordDecl, FieldKey, Native, Primitive, RecordDecl,
    RecordId,
};

use crate::{
    bridge::c::{Identifier, TypeFragment},
    core::{Error, RenderContext, Result},
    target::python::{
        cpython::render::primitive, name_style::Name, syntax::Identifier as PythonIdentifier,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncodedCodec {
    Native { fields: Vec<EncodedCodecField> },
    Registered,
}

impl EncodedCodec {
    pub fn from_record(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        record
            .fields()
            .iter()
            .map(EncodedCodecField::from_field)
            .collect::<Result<Vec<_>>>()
            .map(|fields| Self::Native { fields })
            .or_else(|error| match error {
                Error::UnsupportedTarget {
                    target: "python",
                    shape: "encoded record C codec",
                } => Ok(Self::Registered),
                error => Err(error),
            })
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        match self {
            Self::Native { fields } => fields
                .iter()
                .flat_map(EncodedCodecField::primitives)
                .collect(),
            Self::Registered => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedCodecField {
    python_name: PythonIdentifier,
    value_name: Identifier,
    codec: EncodedCodecNode,
}

impl EncodedCodecField {
    fn from_field(field: &EncodedFieldDecl) -> Result<Self> {
        let python_name = Self::field_name(field.key())?;
        Ok(Self {
            value_name: Identifier::escape(format!("{python_name}_value"))?,
            python_name,
            codec: EncodedCodecNode::from_node(field.read().root())?,
        })
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.codec.primitives()
    }

    pub fn python_name(&self) -> &PythonIdentifier {
        &self.python_name
    }

    pub fn value_name(&self) -> &Identifier {
        &self.value_name
    }

    pub fn codec(&self) -> &EncodedCodecNode {
        &self.codec
    }

    fn field_name(key: &FieldKey) -> Result<PythonIdentifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => Name::position_field(*position),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record field key",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncodedCodecNode {
    Primitive(EncodedPrimitive),
    String,
    Sequence(EncodedSequence),
}

impl EncodedCodecNode {
    fn from_node(node: &CodecNode) -> Result<Self> {
        match node {
            CodecNode::Primitive(primitive) => {
                EncodedPrimitive::new(*primitive).map(Self::Primitive)
            }
            CodecNode::String => Ok(Self::String),
            CodecNode::Sequence { element, .. } => {
                EncodedSequence::new(element.as_ref()).map(Self::Sequence)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded record C codec",
            }),
        }
    }

    fn from_sequence_item(node: &CodecNode) -> Result<Self> {
        match node {
            CodecNode::Primitive(_) | CodecNode::String => Self::from_node(node),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded record C codec",
            }),
        }
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        match self {
            Self::Primitive(primitive) => vec![primitive.runtime()],
            Self::String => Vec::new(),
            Self::Sequence(sequence) => sequence.primitives(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedSequence {
    item_name: Identifier,
    item: Box<EncodedCodecNode>,
}

impl EncodedSequence {
    fn new(node: &CodecNode) -> Result<Self> {
        Ok(Self {
            item_name: Identifier::parse("item")?,
            item: Box::new(EncodedCodecNode::from_sequence_item(node)?),
        })
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.item.primitives()
    }

    pub fn item_name(&self) -> &Identifier {
        &self.item_name
    }

    pub fn item(&self) -> &EncodedCodecNode {
        &self.item
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPrimitive {
    runtime: primitive::Runtime,
    c_type: TypeFragment,
    parser: Identifier,
    boxer: Identifier,
    wire_size: usize,
}

impl EncodedPrimitive {
    fn new(primitive: Primitive) -> Result<Self> {
        let runtime = primitive::Runtime::new(primitive);
        Ok(Self {
            runtime,
            c_type: runtime.c_type()?,
            parser: runtime.parser()?,
            boxer: runtime.boxer()?,
            wire_size: runtime.wire_size()?,
        })
    }

    fn runtime(&self) -> primitive::Runtime {
        self.runtime
    }

    pub fn c_type(&self) -> &TypeFragment {
        &self.c_type
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn boxer(&self) -> &Identifier {
        &self.boxer
    }

    pub fn wire_size(&self) -> usize {
        self.wire_size
    }

    pub fn is_bool(&self) -> bool {
        self.runtime.is_bool()
    }

    pub fn is_i8(&self) -> bool {
        self.runtime.is_i8()
    }

    pub fn is_u8(&self) -> bool {
        self.runtime.is_u8()
    }

    pub fn is_i16(&self) -> bool {
        self.runtime.is_i16()
    }

    pub fn is_u16(&self) -> bool {
        self.runtime.is_u16()
    }

    pub fn is_i32(&self) -> bool {
        self.runtime.is_i32()
    }

    pub fn is_u32(&self) -> bool {
        self.runtime.is_u32()
    }

    pub fn is_i64(&self) -> bool {
        self.runtime.is_i64()
    }

    pub fn is_u64(&self) -> bool {
        self.runtime.is_u64()
    }

    pub fn is_isize(&self) -> bool {
        self.runtime.is_isize()
    }

    pub fn is_usize(&self) -> bool {
        self.runtime.is_usize()
    }

    pub fn is_f32(&self) -> bool {
        self.runtime.is_f32()
    }

    pub fn is_f64(&self) -> bool {
        self.runtime.is_f64()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeCodec {
    Record(NativeRecord),
    Sequence(NativeSequence),
}

impl NativeCodec {
    pub fn from_node(node: &CodecNode, context: &RenderContext<Native>) -> Result<Option<Self>> {
        match node {
            CodecNode::EncodedRecord(record_id) => {
                Self::record(*record_id, context).map(|record| record.map(Self::Record))
            }
            CodecNode::Sequence { element, .. } => match element.as_ref() {
                CodecNode::EncodedRecord(record_id) => Self::record(*record_id, context)
                    .and_then(|record| record.map(NativeSequence::new).transpose())
                    .map(|sequence| sequence.map(Self::Sequence)),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    pub fn supports_node(
        node: &CodecNode,
        mut supports_record: impl FnMut(RecordId) -> Result<bool>,
    ) -> Result<bool> {
        match node {
            CodecNode::EncodedRecord(record_id) => supports_record(*record_id),
            CodecNode::Sequence { element, .. } => match element.as_ref() {
                CodecNode::EncodedRecord(record_id) => supports_record(*record_id),
                _ => Ok(false),
            },
            _ => Ok(false),
        }
    }

    pub fn encoder(&self) -> &Identifier {
        match self {
            Self::Record(record) => record.encoder(),
            Self::Sequence(sequence) => sequence.encoder(),
        }
    }

    pub fn decoder(&self) -> &Identifier {
        match self {
            Self::Record(record) => record.decoder(),
            Self::Sequence(sequence) => sequence.decoder(),
        }
    }

    pub fn sequence(&self) -> Option<&NativeSequence> {
        match self {
            Self::Sequence(sequence) => Some(sequence),
            Self::Record(_) => None,
        }
    }

    fn record(
        record_id: RecordId,
        context: &RenderContext<Native>,
    ) -> Result<Option<NativeRecord>> {
        let Some(RecordDecl::Encoded(record)) = context.record(record_id) else {
            return Ok(None);
        };
        if !EncodedCodec::from_record(record)?.is_native() {
            return Ok(None);
        }
        NativeRecord::new(record).map(Some)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeRecord {
    stem: Identifier,
    encoder: Identifier,
    decoder: Identifier,
    reader: Identifier,
}

impl NativeRecord {
    fn new(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function_text()?)?;
        Ok(Self {
            stem: stem.clone(),
            encoder: Identifier::parse(format!("boltffi_python_wire_{stem}"))?,
            decoder: Identifier::parse(format!("boltffi_python_decode_owned_{stem}"))?,
            reader: Identifier::parse(format!("boltffi_python_decode_owned_{stem}_read"))?,
        })
    }

    pub fn encoder(&self) -> &Identifier {
        &self.encoder
    }

    pub fn decoder(&self) -> &Identifier {
        &self.decoder
    }

    pub fn reader(&self) -> &Identifier {
        &self.reader
    }

    fn stem(&self) -> &Identifier {
        &self.stem
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeSequence {
    encoder: Identifier,
    decoder: Identifier,
    item: NativeRecord,
}

impl NativeSequence {
    fn new(item: NativeRecord) -> Result<Self> {
        let stem = item.stem();
        Ok(Self {
            encoder: Identifier::parse(format!("boltffi_python_wire_vec_{stem}"))?,
            decoder: Identifier::parse(format!("boltffi_python_decode_owned_vec_{stem}"))?,
            item,
        })
    }

    pub fn encoder(&self) -> &Identifier {
        &self.encoder
    }

    pub fn decoder(&self) -> &Identifier {
        &self.decoder
    }

    pub fn item_encoder(&self) -> &Identifier {
        self.item.encoder()
    }

    pub fn item_reader(&self) -> &Identifier {
        self.item.reader()
    }
}
