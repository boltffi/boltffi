//! Direct-record value layout for JNI.
//!
//! Direct records cross the JVM boundary as raw bytes, but the bridge must still
//! know which C ABI type those bytes represent and how many bytes are expected.
//! That information comes from the lower C bridge, where record layout was
//! already selected.
//!
//! This module stores the fixed layout facts needed by parameters, returns, and
//! callback arguments. It never looks into record fields; it only carries the ABI
//! value shape already chosen by the C bridge.

use crate::bridge::c::{self, Expression, Identifier, TypeFragment};

/// Direct record value carried through JNI as a fixed-size byte array.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RecordValue {
    c_type: Identifier,
    passing: RecordPassing,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RecordPassing {
    Value,
    Pointer,
}

impl RecordValue {
    /// Returns the C typedef used by the C bridge.
    pub fn c_type(&self) -> &Identifier {
        &self.c_type
    }

    /// Returns the C typedef as a type fragment.
    pub fn c_type_fragment(&self) -> TypeFragment {
        TypeFragment::new(self.c_type.to_string())
    }

    /// Returns the C bridge call expression for this record value.
    pub fn c_argument(&self, local: Identifier) -> Expression {
        let local = Expression::identifier(local);
        match self.passing {
            RecordPassing::Value => local,
            RecordPassing::Pointer => Expression::address_of(local),
        }
    }

    /// Creates a direct-record JNI value from a C ABI type.
    pub fn from_c_type(ty: &c::Type) -> Option<Self> {
        match ty {
            c::Type::DirectRecord(c_type) => Some(Self {
                c_type: c_type.clone(),
                passing: RecordPassing::Value,
            }),
            c::Type::ConstPointer(inner) | c::Type::MutPointer(inner) => match inner.as_ref() {
                c::Type::DirectRecord(c_type) => Some(Self {
                    c_type: c_type.clone(),
                    passing: RecordPassing::Pointer,
                }),
                _ => None,
            },
            _ => None,
        }
    }
}
