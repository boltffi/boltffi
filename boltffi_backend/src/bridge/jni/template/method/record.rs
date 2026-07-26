//! Source fields for direct-record native method parameters.
//!
//! Java passes direct records as byte arrays. Before the C bridge call, the
//! generated method checks the byte length and copies the bytes into local C
//! record storage. Mutable records also need a writeback copy after Rust
//! returns.
//!
//! This module prepares those local names from the record parameter contract.
//! The template prints the copy and writeback steps; it does not decide whether
//! the record is direct or mutable.

use crate::bridge::{c::Identifier, jni::RecordParameter};

#[derive(Clone)]
pub struct RecordParameterView {
    pub name: Identifier,
    pub c_type: Identifier,
    pub pointer: Identifier,
    pub local: Identifier,
    pub writeback: Option<RecordWritebackView>,
}

impl RecordParameterView {
    pub fn from_record(parameter: &RecordParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            c_type: parameter.c_type().clone(),
            pointer: parameter.pointer().clone(),
            local: parameter.local().clone(),
            writeback: parameter.writeback().map(|writeback| RecordWritebackView {
                c_type: parameter.c_type().clone(),
                local: writeback.local().clone(),
            }),
        }
    }
}

#[derive(Clone)]
pub struct RecordWritebackView {
    pub c_type: Identifier,
    pub local: Identifier,
}
