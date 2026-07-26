//! Encoded direct-buffer parameters passed from the JVM into Rust.
//!
//! Encoded values enter JNI as a direct buffer plus an exact used length, while
//! the lower C bridge accepts a borrowed pointer plus length.
//!
//! This module records the stable names for that borrow. It does not decide what
//! the bytes mean and it does not inspect codec plans. By this point lowering
//! has already selected an encoded parameter; the JNI contract only describes
//! how the direct buffer is held long enough to reach the C ABI.

use crate::{
    bridge::c::{self, Identifier},
    core::Result,
};

/// A JVM direct-buffer parameter borrowed as C pointer and length arguments.
///
/// The parameter owns the JVM-facing buffer and length names and the generated
/// pointer local used by the native method body.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BytesParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
    writeback: Option<BytesWriteback>,
}

impl BytesParameter {
    /// Returns the generated JNI direct-buffer parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the generated JNI payload-length parameter.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    /// Returns the encoded mutation output written by the C bridge.
    pub fn writeback(&self) -> Option<&BytesWriteback> {
        self.writeback.as_ref()
    }

    /// Creates a byte-array parameter from a C byte-slice parameter group.
    pub fn from_c_group(bytes: &c::ByteSliceParameter) -> Result<Self> {
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", bytes.name()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", bytes.name()))?,
            name: Identifier::escape(bytes.name())?,
            writeback: None,
        })
    }

    /// Creates a byte-array parameter from an encoded writeback C group.
    pub fn from_c_writeback(writeback: &c::EncodedWritebackParameter) -> Result<Self> {
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", writeback.name()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", writeback.name()))?,
            name: Identifier::escape(writeback.name())?,
            writeback: Some(BytesWriteback::from_c_writeback(writeback)?),
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
/// Native storage for an encoded mutation result returned to the JVM.
pub struct BytesWriteback {
    local: Identifier,
}

impl BytesWriteback {
    /// Returns the local `FfiBuf_u8` that receives the mutation bytes.
    pub fn local(&self) -> &Identifier {
        &self.local
    }

    /// Creates mutation output storage from a C encoded writeback group.
    pub fn from_c_writeback(writeback: &c::EncodedWritebackParameter) -> Result<Self> {
        Ok(Self {
            local: Identifier::parse(format!("__boltffi_{}_out", writeback.name()))?,
        })
    }
}
