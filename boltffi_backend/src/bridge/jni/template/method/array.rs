//! Source fields for Java arrays borrowed by native methods.
//!
//! The method contract knows which parameters arrive as Java arrays. The C
//! source template needs a flatter view: local pointer names, length variables,
//! JNI function-table members, cleanup rules, and optional stack-copy storage.
//!
//! This module is that projection layer. It prepares source fields from the
//! already typed contract and keeps array lifetime rules out of the Askama
//! fragments. Encoded byte arrays always use the borrowed-elements path. Direct
//! primitive vectors may use the small stack-copy path recorded by the contract.

use crate::{
    bridge::{
        c::{Identifier, TypeFragment},
        jni::{DirectVectorParameter, NativeParameter},
    },
    core::Result,
};

/// Template input for one Java array borrowed before a C bridge call.
///
/// The view contains only source-generation facts: the Java array name, local
/// pointer and length storage, JNI borrow/release functions, and optional
/// stack-copy storage. The decision that a parameter is an encoded byte array or
/// a direct vector has already happened in the contract layer.
#[derive(Clone)]
pub struct BorrowedArrayParameterView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
    pub element_type: TypeFragment,
    pub getter: &'static str,
    pub releaser: &'static str,
    pub release_mode: &'static str,
    pub stack_copy: Option<BorrowedArrayStackCopyView>,
}

/// Stack-copy template fields for one borrowed primitive array.
///
/// The generated method copies small Java primitive arrays into this local stack
/// storage with `Get*ArrayRegion`. Larger arrays still use the normal borrowed
/// elements path and are released during cleanup.
#[derive(Clone)]
pub struct BorrowedArrayStackCopyView {
    pub storage: Identifier,
    pub needs_release: Identifier,
    pub max_len: usize,
    pub region_getter: &'static str,
}

impl BorrowedArrayParameterView {
    pub fn from_direct_vector(parameter: &DirectVectorParameter) -> Result<Self> {
        Ok(Self {
            name: parameter.name().clone(),
            pointer: parameter.pointer().clone(),
            length: parameter.length().clone(),
            element_type: parameter.element_type(),
            getter: parameter.getter(),
            releaser: parameter.releaser(),
            release_mode: parameter.release_mode(),
            stack_copy: parameter
                .stack_copy()
                .map(|stack_copy| -> Result<BorrowedArrayStackCopyView> {
                    Ok(BorrowedArrayStackCopyView {
                        storage: Identifier::parse(format!(
                            "__boltffi_{}_stack",
                            parameter.name()
                        ))?,
                        needs_release: Identifier::parse(format!(
                            "__boltffi_{}_needs_release",
                            parameter.name()
                        ))?,
                        max_len: stack_copy.max_len(),
                        region_getter: stack_copy.region_getter(),
                    })
                })
                .transpose()?,
        })
    }

    pub fn from_parameter(parameter: &NativeParameter) -> Option<Result<Self>> {
        parameter.direct_vector().map(Self::from_direct_vector)
    }
}
