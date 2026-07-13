//! Template view for a JNI native opaque borrow wrapper.

use crate::{
    bridge::{c::Identifier, jni::NativeOpaqueBorrowWrapper},
    core::Result,
};

/// Askama template view for one JNI borrow wrapper.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[allow(dead_code)]
pub struct BorrowWrapperView {
    /// The `Java_*` symbol as a C identifier.
    pub(crate) symbol: Identifier,
    /// The raw C borrow function name.
    pub(crate) c_function: Identifier,
}

impl BorrowWrapperView {
    /// Builds a view from one borrow wrapper.
    pub fn from_wrapper(wrapper: &NativeOpaqueBorrowWrapper) -> Result<Self> {
        Ok(Self {
            symbol: Identifier::parse(wrapper.symbol().as_identifier().as_str())?,
            c_function: wrapper.c_function().clone(),
        })
    }

    #[allow(dead_code)]
    pub fn symbol(&self) -> &Identifier {
        &self.symbol
    }

    #[allow(dead_code)]
    pub fn c_function(&self) -> &Identifier {
        &self.c_function
    }
}
