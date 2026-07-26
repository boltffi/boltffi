//! Support-fragment selection driven by closure registrations.
//!
//! Registered closures can need byte-array helpers, direct-vector helpers,
//! return-buffer helpers, direct-record helpers, and callback-handle wrappers.
//! Those fragments are shared across every closure signature in the generated
//! source file.
//!
//! This module scans the rendered closure views and reports which shared
//! fragments are needed. It does not rebuild closure signatures or inspect the
//! original declaration that mentioned the closure.

use crate::bridge::jni::template::closure::ClosureRegistrationView;

pub struct ClosureFeatures {
    pub has_registrations: bool,
    pub uses_byte_arrays: bool,
    pub uses_direct_vectors: bool,
    pub uses_records: bool,
    pub returns_byte_arrays: bool,
    pub returns_records: bool,
    pub returns_callback_handles: bool,
}

impl ClosureFeatures {
    pub fn from_registrations(closures: &[ClosureRegistrationView]) -> Self {
        Self {
            has_registrations: !closures.is_empty(),
            uses_byte_arrays: closures.iter().any(|closure| {
                !closure.byte_arrays.is_empty() || !closure.handle_byte_arrays.is_empty()
            }),
            uses_direct_vectors: closures.iter().any(|closure| {
                !closure.direct_vectors.is_empty() || !closure.handle_direct_vectors.is_empty()
            }),
            uses_records: closures
                .iter()
                .any(|closure| !closure.records.is_empty() || !closure.handle_records.is_empty()),
            returns_byte_arrays: closures
                .iter()
                .any(|closure| closure.returns_bytes || closure.returns_record),
            returns_records: closures.iter().any(|closure| closure.returns_record),
            returns_callback_handles: closures
                .iter()
                .any(|closure| closure.returns_callback_handle),
        }
    }
}
