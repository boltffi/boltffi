use crate::ir::types::PrimitiveType;
use crate::render::python::primitives::PythonScalarTypeExt as _;

use super::{PythonEnumType, PythonRecordType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonSequenceType {
    PrimitiveVec(PrimitiveType),
    CStyleEnumVec(PythonEnumType),
}

impl PythonSequenceType {
    pub fn parameter_annotation(&self) -> String {
        match self {
            Self::PrimitiveVec(PrimitiveType::U8) => "bytes | Sequence[int]".to_string(),
            Self::PrimitiveVec(primitive) => {
                format!("Sequence[{}]", primitive.python_annotation())
            }
            Self::CStyleEnumVec(enum_type) => format!("Sequence[{}]", enum_type.type_literal()),
        }
    }

    pub fn return_annotation(&self) -> String {
        match self {
            Self::PrimitiveVec(PrimitiveType::U8) => "bytes".to_string(),
            Self::PrimitiveVec(primitive) => {
                format!("list[{}]", primitive.python_annotation())
            }
            Self::CStyleEnumVec(enum_type) => format!("list[{}]", enum_type.type_literal()),
        }
    }

    pub fn primitive_element(&self) -> Option<PrimitiveType> {
        match self {
            Self::PrimitiveVec(primitive) => Some(*primitive),
            Self::CStyleEnumVec(_) => None,
        }
    }

    pub fn enum_element(&self) -> Option<&PythonEnumType> {
        match self {
            Self::CStyleEnumVec(enum_type) => Some(enum_type),
            _ => None,
        }
    }

    pub fn is_byte_like(&self) -> bool {
        matches!(self, Self::PrimitiveVec(PrimitiveType::U8))
    }

    pub fn is_primitive_vector(&self) -> bool {
        matches!(self, Self::PrimitiveVec(_))
    }

    pub fn is_c_style_enum_vector(&self) -> bool {
        matches!(self, Self::CStyleEnumVec(_))
    }

    pub fn uses_buffer_input(&self) -> bool {
        matches!(self, Self::PrimitiveVec(_) | Self::CStyleEnumVec(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonType {
    Void,
    Primitive(PrimitiveType),
    Record(PythonRecordType),
    CStyleEnum(PythonEnumType),
    String,
    Sequence(PythonSequenceType),
}

impl PythonType {
    pub fn parameter_annotation(&self) -> String {
        match self {
            Self::Void => "None".to_string(),
            Self::Primitive(primitive) => primitive.python_annotation().to_string(),
            Self::Record(record_type) => record_type.type_literal(),
            Self::CStyleEnum(enum_type) => enum_type.type_literal(),
            Self::String => "str".to_string(),
            Self::Sequence(sequence) => sequence.parameter_annotation(),
        }
    }

    pub fn return_annotation(&self) -> String {
        match self {
            Self::Void => "None".to_string(),
            Self::Primitive(primitive) => primitive.python_annotation().to_string(),
            Self::Record(record_type) => record_type.type_literal(),
            Self::CStyleEnum(enum_type) => enum_type.type_literal(),
            Self::String => "str".to_string(),
            Self::Sequence(sequence) => sequence.return_annotation(),
        }
    }

    pub fn native_primitive(&self) -> Option<PrimitiveType> {
        match self {
            Self::Void => None,
            Self::Primitive(primitive) => Some(*primitive),
            Self::Record(_) => None,
            Self::CStyleEnum(enum_type) => Some(enum_type.tag_type),
            Self::String => None,
            Self::Sequence(sequence) => sequence.primitive_element(),
        }
    }

    pub fn record(&self) -> Option<&PythonRecordType> {
        match self {
            Self::Record(record_type) => Some(record_type),
            _ => None,
        }
    }

    pub fn c_style_enum(&self) -> Option<&PythonEnumType> {
        match self {
            Self::CStyleEnum(enum_type) => Some(enum_type),
            _ => None,
        }
    }

    pub fn sequence_c_style_enum(&self) -> Option<&PythonEnumType> {
        match self {
            Self::Sequence(sequence) => sequence.enum_element(),
            _ => None,
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn is_record(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    pub fn is_c_style_enum(&self) -> bool {
        matches!(self, Self::CStyleEnum(_))
    }

    pub fn is_byte_like(&self) -> bool {
        matches!(self, Self::Sequence(sequence) if sequence.is_byte_like())
    }

    pub fn is_primitive_vector(&self) -> bool {
        matches!(self, Self::Sequence(PythonSequenceType::PrimitiveVec(_)))
    }

    pub fn is_c_style_enum_vector(&self) -> bool {
        matches!(self, Self::Sequence(PythonSequenceType::CStyleEnumVec(_)))
    }

    pub fn uses_buffer_input(&self) -> bool {
        matches!(self, Self::Sequence(sequence) if sequence.uses_buffer_input())
    }

    pub fn is_owned_buffer(&self) -> bool {
        matches!(self, Self::String | Self::Sequence(_))
    }
}
