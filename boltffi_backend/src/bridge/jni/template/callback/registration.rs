//! Source fields for one JVM callback registration.
//!
//! A callback registration connects a Rust callback trait to a generated JVM
//! class. The generated C file needs load and unload hooks for method ids, a C
//! vtable with one slot per trait method, and local helper symbols for clone and
//! free operations.
//!
//! This module gathers those source-facing names from the callback contract. It
//! keeps the registration shape together so the root source template can render
//! every callback class through the same path.

use crate::bridge::{
    c::Identifier,
    jni::{CallbackRegistration, name::LookupText},
};

use super::{CallbackHandleMethodView, CallbackMethodView};

pub struct CallbackRegistrationView {
    pub class: LookupText,
    pub global_class: Identifier,
    pub free_method: Identifier,
    pub clone_method: Identifier,
    pub load: Identifier,
    pub unload: Identifier,
    pub vtable_type: Identifier,
    pub vtable: Identifier,
    pub register: Identifier,
    pub free: Identifier,
    pub clone: Identifier,
    pub methods: Vec<CallbackMethodView>,
    pub handle_methods: Vec<CallbackHandleMethodView>,
}

impl CallbackRegistrationView {
    pub fn from_registration(registration: &CallbackRegistration) -> crate::core::Result<Self> {
        Ok(Self {
            class: LookupText::new(&registration.class().as_jni_class_name()),
            global_class: registration.global_class().clone(),
            free_method: registration.free_method().clone(),
            clone_method: registration.clone_method().clone(),
            load: registration.load().clone(),
            unload: registration.unload().clone(),
            vtable_type: registration.vtable_type().clone(),
            vtable: registration.vtable().clone(),
            register: registration.register().clone(),
            free: registration.free().clone(),
            clone: registration.clone_callback().clone(),
            methods: registration
                .methods()
                .iter()
                .map(CallbackMethodView::from_method)
                .collect(),
            handle_methods: registration
                .handle_methods()
                .iter()
                .map(CallbackHandleMethodView::from_method)
                .collect::<crate::core::Result<Vec<_>>>()?,
        })
    }
}
