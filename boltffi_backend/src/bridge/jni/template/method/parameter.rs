//! Source fields for JNI native method declarations.
//!
//! The contract layer keeps each native parameter tied to its C bridge
//! arguments. The C source declaration only needs the Java-facing name and JNI
//! type. This module projects the contract down to that declaration view.
//!
//! Keeping this tiny view separate matters because method signatures are printed
//! before the body borrows arrays, writes records, checks status, or calls the C
//! bridge. The template receives only the fields that belong in the signature.

use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::{NativeParameter, NativeParameterKind},
};

/// One parameter in the generated `Java_*` function signature.
pub struct NativeParameterView {
    pub name: Identifier,
    pub ty: TypeFragment,
}

impl NativeParameterView {
    pub fn from_parameter(parameter: &NativeParameter) -> Vec<Self> {
        match parameter.kind() {
            NativeParameterKind::Scalar(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: parameter.ty().as_type_fragment(),
            }],
            NativeParameterKind::Bytes(parameter) => vec![
                Self {
                    name: parameter.name().clone(),
                    ty: TypeFragment::new("jobject"),
                },
                Self {
                    name: parameter.length().clone(),
                    ty: TypeFragment::new("jint"),
                },
            ],
            NativeParameterKind::DirectVector(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: parameter.array_type(),
            }],
            NativeParameterKind::Record(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: TypeFragment::new("jobject"),
            }],
            NativeParameterKind::Callback(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: parameter.ty(),
            }],
            NativeParameterKind::Closure(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: parameter.ty(),
            }],
            NativeParameterKind::Continuation(parameter) => vec![Self {
                name: parameter.name().clone(),
                ty: parameter.ty(),
            }],
        }
    }
}
