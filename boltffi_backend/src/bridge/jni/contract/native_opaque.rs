//! JNI native methods for native opaque record borrow helpers.
//!
//! The C bridge borrow helpers expose `(const void*, const uint8_t**, uintptr_t*) -> i32`
//! which cannot be called directly from Kotlin/JNI. Instead, we generate a JNI
//! C wrapper that accepts a `jlong` handle, calls the borrow function with
//! stack-allocated pointer/length out-parameters, copies the bytes into a
//! `jbyteArray`, and returns it. Kotlin sees a single-parameter method returning
//! `ByteArray?`.

use crate::{
    bridge::{
        c::{self, Identifier, Type},
        jni::{JniSymbolName, JvmClassPath},
    },
    core::Result,
};

/// JNI borrow wrapper for one native opaque String or Bytes field.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct NativeOpaqueBorrowWrapper {
    /// The `Java_*` symbol for this wrapper method.
    symbol: JniSymbolName,
    /// The raw C borrow function being wrapped (e.g. `record_borrow_title`).
    c_function: Identifier,
}

impl NativeOpaqueBorrowWrapper {
    /// Creates a borrow wrapper for one C borrow function.
    pub fn new(class: &JvmClassPath, c_function: &c::Function) -> Result<Self> {
        Ok(Self {
            symbol: JniSymbolName::native_method(class, c_function.name())?,
            c_function: Identifier::parse(c_function.name())?,
        })
    }

    /// Returns the `Java_*` C symbol for this wrapper.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the raw C borrow function name.
    pub fn c_function(&self) -> &Identifier {
        &self.c_function
    }
}

/// Returns whether a C function is a native opaque borrow helper.
///
/// Borrow helpers have exactly 3 value-group parameters:
/// - `const void *handle`
/// - `const uint8_t **ptr_out` (`MutPointer(ConstPointer(Uint8))`)
/// - `uintptr_t *len_out` (`MutPointer(PointerWidth)`)
///
/// and return `int32_t`.
pub fn is_borrow_helper(function: &c::Function) -> bool {
    let groups = function.parameter_groups();
    if groups.len() != 3 {
        return false;
    }
    if !matches!(function.returns(), Type::Int32) {
        return false;
    }
    // All 3 must be plain value parameters
    let all_values = groups
        .iter()
        .all(|g| matches!(g, c::ParameterGroup::Value(_)));
    if !all_values {
        return false;
    }
    let params = function.params();
    if params.len() != 3 {
        return false;
    }
    // param[0]: const void* (handle)
    let p0_ok =
        matches!(params[0].ty(), Type::ConstPointer(inner) if matches!(inner.as_ref(), Type::Void));
    // param[1]: mut pointer to const uint8_t pointer
    let p1_ok = matches!(params[1].ty(), Type::MutPointer(inner)
        if matches!(inner.as_ref(), Type::ConstPointer(inner2) if matches!(inner2.as_ref(), Type::Uint8)));
    // param[2]: mut pointer to uintptr_t
    let p2_ok = matches!(params[2].ty(), Type::MutPointer(inner) if matches!(inner.as_ref(), Type::PointerWidth));
    p0_ok && p1_ok && p2_ok
}
