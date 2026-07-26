//! Final source view for one registered closure signature.
//!
//! A closure registration becomes a small generated bridge class in C: load and
//! unload hooks, a call trampoline, a release trampoline, argument conversion,
//! return conversion, and optional helper methods for closure handles passed
//! through callbacks.
//!
//! This module flattens the closure contract into the fields that source
//! template needs. The same view is used for every declaration that mentions the
//! signature, which keeps closure rendering shared across functions, callbacks,
//! nested closures, and returned closures.

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, Literal, TypeFragment},
        jni::{ClosureArgument, ClosureRegistration, name::LookupText},
    },
    core::Result,
};

use super::{
    ClosureBytesArgumentView, ClosureCParameterView, ClosureDirectVectorArgumentView,
    ClosureHandleArgumentView, ClosureRecordArgumentView,
};

pub struct ClosureRegistrationView {
    pub class: LookupText,
    pub global_class: Identifier,
    pub call_method: Identifier,
    pub free_method: Identifier,
    pub load: Identifier,
    pub unload: Identifier,
    pub call: Identifier,
    pub release: Identifier,
    pub c_return_type: TypeFragment,
    pub returns_void: bool,
    pub returns_byte_array: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback_handle: bool,
    pub callback_handle_constructor: Option<Identifier>,
    pub method_signature: LookupText,
    pub call_method_suffix: String,
    pub failure_value: Expression,
    pub c_parameters: Vec<ClosureCParameterView>,
    pub byte_arrays: Vec<ClosureBytesArgumentView>,
    pub direct_vectors: Vec<ClosureDirectVectorArgumentView>,
    pub records: Vec<ClosureRecordArgumentView>,
    pub closure_handles: Vec<ClosureHandleArgumentView>,
    pub jni_arguments: ArgumentList,
    pub has_jni_arguments: bool,
    pub handle_parameters: Vec<ClosureCParameterView>,
    pub handle_byte_arrays: Vec<ClosureBytesArgumentView>,
    pub handle_direct_vectors: Vec<ClosureDirectVectorArgumentView>,
    pub handle_records: Vec<ClosureRecordArgumentView>,
    pub rust_arguments: ArgumentList,
    pub has_rust_arguments: bool,
}

impl ClosureRegistrationView {
    pub fn from_registration(registration: &ClosureRegistration) -> Result<Self> {
        let arguments = registration.arguments();
        Ok(Self {
            class: LookupText::new(&registration.class().as_jni_class_name()),
            global_class: registration.global_class().clone(),
            call_method: registration.call_method().clone(),
            free_method: registration.free_method().clone(),
            load: registration.load().clone(),
            unload: registration.unload().clone(),
            call: registration.call().clone(),
            release: registration.release().clone(),
            c_return_type: registration.c_return_type().clone(),
            returns_void: registration.returns_void(),
            returns_byte_array: registration.returns_byte_array(),
            returns_bytes: registration.returns_bytes(),
            returns_record: registration.returns_record(),
            returns_callback_handle: registration.returns_callback_handle(),
            callback_handle_constructor: registration.callback_handle_constructor().cloned(),
            method_signature: LookupText::new(&registration.method_signature()),
            call_method_suffix: registration
                .call_method_suffix()
                .unwrap_or_default()
                .to_owned(),
            failure_value: registration
                .failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            c_parameters: arguments
                .iter()
                .flat_map(ClosureArgument::c_parameters)
                .map(ClosureCParameterView::from_parameter)
                .collect(),
            byte_arrays: arguments
                .iter()
                .filter_map(ClosureArgument::call_bytes)
                .map(ClosureBytesArgumentView::from_argument)
                .collect(),
            direct_vectors: arguments
                .iter()
                .filter_map(ClosureArgument::call_direct_vector)
                .map(ClosureDirectVectorArgumentView::from_argument)
                .collect::<Result<Vec<_>>>()?,
            records: arguments
                .iter()
                .filter_map(ClosureArgument::call_record)
                .map(ClosureRecordArgumentView::from_argument)
                .collect(),
            closure_handles: arguments
                .iter()
                .filter_map(ClosureArgument::call_closure)
                .map(ClosureHandleArgumentView::from_argument)
                .collect(),
            jni_arguments: ClosureArgument::jvm_argument_list(arguments),
            has_jni_arguments: !arguments.is_empty(),
            handle_parameters: arguments
                .iter()
                .flat_map(ClosureArgument::handle_parameters)
                .map(ClosureCParameterView::from_parameter)
                .collect(),
            handle_byte_arrays: arguments
                .iter()
                .filter_map(ClosureArgument::handle_bytes)
                .map(ClosureBytesArgumentView::from_argument)
                .collect(),
            handle_direct_vectors: arguments
                .iter()
                .filter_map(ClosureArgument::handle_direct_vector)
                .map(ClosureDirectVectorArgumentView::from_argument)
                .collect::<Result<Vec<_>>>()?,
            handle_records: arguments
                .iter()
                .filter_map(ClosureArgument::handle_record)
                .map(ClosureRecordArgumentView::from_argument)
                .collect(),
            rust_arguments: ClosureArgument::rust_argument_list(arguments),
            has_rust_arguments: !arguments.is_empty(),
        })
    }
}
