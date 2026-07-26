//! Support-fragment selection driven by callback registrations.
//!
//! Callback vtable dispatch can need byte arrays, direct primitive vectors,
//! direct-record arrays, callback handle storage, closure handles, async
//! completion helpers, and callback-return helpers. The generated source should
//! include each support block only when at least one rendered callback uses it.
//!
//! This module derives those booleans from callback template views. The callback
//! contract has already selected the ABI behavior; this pass only controls which
//! C fragments are printed.

use crate::bridge::jni::template::callback::CallbackRegistrationView;

pub struct CallbackFeatures {
    pub has_registrations: bool,
    pub has_handle_methods: bool,
    pub uses_byte_arrays: bool,
    pub uses_direct_vectors: bool,
    pub uses_record_arrays: bool,
    pub uses_direct_buffers: bool,
    pub uses_handles: bool,
    pub checks_status: bool,
    pub checks_error_buffer: bool,
    pub returns_byte_arrays: bool,
    pub returns_records: bool,
    pub returns_callback_handles: bool,
}

impl CallbackFeatures {
    pub fn from_registrations(callbacks: &[CallbackRegistrationView]) -> Self {
        Self {
            has_registrations: !callbacks.is_empty(),
            has_handle_methods: callbacks
                .iter()
                .any(|callback| !callback.handle_methods.is_empty()),
            uses_byte_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.byte_arrays.is_empty())
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_bytes
                            || method.returns_record
                            || method.completion.as_ref().is_some_and(|completion| {
                                completion.payload_bytes || completion.payload_record
                            })
                    })
            }),
            uses_direct_vectors: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.direct_vectors.is_empty())
                    || callback.handle_methods.iter().any(|method| {
                        method
                            .borrowed_arrays
                            .iter()
                            .any(|array| array.stack_copy.is_some())
                    })
            }),
            uses_record_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.record_arrays.is_empty())
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_record
                            || method
                                .completion
                                .as_ref()
                                .is_some_and(|completion| completion.payload_record)
                    })
            }),
            uses_direct_buffers: callbacks.iter().any(|callback| {
                callback.handle_methods.iter().any(|method| {
                    !method.direct_buffers.is_empty() || !method.record_buffers.is_empty()
                })
            }),
            uses_handles: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.callback_handles.is_empty())
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_callback
                            || method
                                .completion
                                .as_ref()
                                .is_some_and(|completion| completion.payload_callback_handle)
                    })
            }),
            checks_status: callbacks.iter().any(|callback| {
                callback
                    .handle_methods
                    .iter()
                    .any(|method| method.returns_closure || method.checks_status)
            }),
            checks_error_buffer: callbacks.iter().any(|callback| {
                callback
                    .handle_methods
                    .iter()
                    .any(|method| method.checks_error_buffer)
            }),
            returns_byte_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| method.returns_bytes || method.returns_record)
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_bytes
                            || method.returns_record
                            || method.completion.as_ref().is_some_and(|completion| {
                                completion.payload_bytes || completion.payload_record
                            })
                    })
            }),
            returns_records: callbacks.iter().any(|callback| {
                callback.methods.iter().any(|method| method.returns_record)
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_record
                            || method
                                .completion
                                .as_ref()
                                .is_some_and(|completion| completion.payload_record)
                    })
            }),
            returns_callback_handles: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| method.returns_callback_handle)
                    || callback.handle_methods.iter().any(|method| {
                        method.returns_callback
                            || method
                                .completion
                                .as_ref()
                                .is_some_and(|completion| completion.payload_callback_handle)
                    })
            }),
        }
    }
}
