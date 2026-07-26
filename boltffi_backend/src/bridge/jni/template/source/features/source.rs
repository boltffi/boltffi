//! Source-wide support-fragment selection.
//!
//! The root JNI template contains optional C support blocks for arrays, records,
//! callbacks, closures, continuations, streams, status checks, lifecycle hooks,
//! and async completion. Printing every block for every crate makes the
//! generated source larger and hides which protocols are actually used.
//!
//! This module combines the feature scans from rendered method, callback,
//! closure, completion, and stream views. It selects source fragments from the
//! contract that will be printed, not from raw binding IR.

use crate::bridge::jni::template::{
    callback::{CallbackCompletionInvokerView, CallbackRegistrationView},
    closure::{CallbackClosureHandleView, ClosureRegistrationView},
    method::NativeMethodView,
    source::SuccessOutWriterView,
    stream::DirectStreamBatchView,
};

use super::{
    callback::CallbackFeatures, closure::ClosureFeatures, completion::CompletionFeatures,
    method::MethodFeatures, stream::StreamFeatures,
};

pub struct SourceFeatures {
    pub uses_limits: bool,
    pub checks_status: bool,
    pub checks_error_buffer: bool,
    pub uses_byte_arrays: bool,
    pub uses_record_arrays: bool,
    pub uses_direct_buffers: bool,
    pub uses_exceptions: bool,
    pub uses_lifecycle: bool,
    pub uses_continuations: bool,
    pub uses_callback_handles: bool,
    pub uses_closure_handles: bool,
}

impl SourceFeatures {
    pub fn from_views(
        methods: &[NativeMethodView],
        direct_stream_batches: &[DirectStreamBatchView],
        callbacks: &[CallbackRegistrationView],
        callback_completions: &[CallbackCompletionInvokerView],
        success_out_writers: &[SuccessOutWriterView],
        closures: &[ClosureRegistrationView],
        closure_handles: &[CallbackClosureHandleView],
    ) -> Self {
        let methods = MethodFeatures::from_methods(methods);
        let callbacks = CallbackFeatures::from_registrations(callbacks);
        let completions = CompletionFeatures::from_invokers(callback_completions);
        let success_writers_use_byte_arrays = success_out_writers
            .iter()
            .any(|writer| writer.writes_bytes || writer.writes_record);
        let success_writers_use_record_arrays = success_out_writers
            .iter()
            .any(|writer| writer.writes_record);
        let closures = ClosureFeatures::from_registrations(closures);
        let streams = StreamFeatures::from_direct_batches(direct_stream_batches);
        let uses_closure_handles = !closure_handles.is_empty();
        let uses_byte_arrays = callbacks.uses_byte_arrays
            || closures.uses_byte_arrays
            || callbacks.returns_byte_arrays
            || closures.returns_byte_arrays
            || methods.returns_byte_arrays
            || completions.uses_byte_arrays
            || success_writers_use_byte_arrays
            || streams.returns_direct_batches;
        let uses_record_arrays = methods.uses_record_arrays
            || callbacks.uses_record_arrays
            || closures.uses_records
            || closures.returns_records
            || callbacks.returns_records
            || completions.uses_record_arrays
            || success_writers_use_record_arrays;
        let uses_direct_buffers = methods.uses_direct_buffers || callbacks.uses_direct_buffers;

        Self {
            uses_limits: uses_byte_arrays
                || uses_record_arrays
                || callbacks.uses_direct_vectors
                || closures.uses_direct_vectors
                || closures.uses_records
                || methods.checks_error_buffer
                || callbacks.checks_error_buffer,
            checks_status: methods.checks_status || callbacks.checks_status,
            checks_error_buffer: methods.checks_error_buffer || callbacks.checks_error_buffer,
            uses_byte_arrays,
            uses_record_arrays,
            uses_direct_buffers,
            uses_exceptions: callbacks.uses_byte_arrays
                || callbacks.uses_direct_vectors
                || callbacks.uses_record_arrays
                || callbacks.uses_handles
                || callbacks.has_handle_methods
                || callbacks.checks_error_buffer
                || uses_closure_handles
                || closures.uses_byte_arrays
                || closures.uses_direct_vectors
                || callbacks.returns_byte_arrays
                || closures.returns_byte_arrays
                || callbacks.returns_callback_handles
                || closures.returns_callback_handles
                || completions.uses_byte_arrays
                || !success_out_writers.is_empty()
                || streams.returns_direct_batches
                || methods.uses_exceptions
                || methods.uses_callback_parameters
                || uses_direct_buffers,
            uses_continuations: methods.uses_continuations,
            uses_lifecycle: methods.uses_continuations
                || callbacks.has_registrations
                || closures.has_registrations,
            uses_callback_handles: callbacks.uses_handles
                || callbacks.has_handle_methods
                || callbacks.returns_callback_handles
                || closures.returns_callback_handles
                || methods.returns_callback_handles
                || methods.uses_callback_parameters,
            uses_closure_handles,
        }
    }
}
