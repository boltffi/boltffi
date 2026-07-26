//! Source fields for one generated `Java_*` native method.
//!
//! The native method contract is shaped for correctness: parameters know their
//! kind, returns know their ABI behavior, and records know their writeback
//! rules. The final C template needs one ordered method body with declarations,
//! borrowed-array setup, direct-record locals, C bridge arguments, status checks,
//! cleanup, and the return expression.
//!
//! This module performs that final projection from contract to source fields.
//! It does not reinterpret the C bridge contract or inspect binding IR. The
//! method template receives a prepared body shape, not raw declarations that it
//! has to understand.

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, TypeFragment},
        jni::{
            NativeMethod, SuccessOutReturn,
            template::method::{
                BorrowedArrayParameterView, DirectBufferParameterView, NativeParameterView,
                RecordParameterView,
            },
        },
    },
    core::Result,
};

/// Template input for a generated JNI native method body.
pub struct NativeMethodView {
    pub symbol: Identifier,
    pub c_function: Identifier,
    pub return_type: TypeFragment,
    pub c_result_type: TypeFragment,
    pub parameters: Vec<NativeParameterView>,
    pub borrowed_arrays: Vec<BorrowedArrayParameterView>,
    pub direct_buffers: Vec<DirectBufferParameterView>,
    pub record_buffers: Vec<RecordParameterView>,
    pub arguments: ArgumentList,
    pub returns_void: bool,
    pub returns_boolean: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback: bool,
    pub return_value: Expression,
    pub checks_status: bool,
    pub checks_completion_status: bool,
    pub checks_error_buffer: bool,
    pub success_out: Option<SuccessOutReturn>,
    pub uses_callback_parameters: bool,
    pub uses_continuations: bool,
    pub has_error_label: bool,
}

impl NativeMethodView {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
        let borrowed_arrays = method
            .parameters()
            .iter()
            .flat_map(BorrowedArrayParameterView::from_parameter)
            .collect::<Result<Vec<_>>>()?;
        let direct_buffers = method
            .parameters()
            .iter()
            .filter_map(DirectBufferParameterView::from_parameter)
            .collect::<Vec<_>>();
        let record_buffers = method
            .parameters()
            .iter()
            .filter_map(|parameter| parameter.record().map(RecordParameterView::from_record))
            .collect::<Vec<_>>();
        Ok(Self {
            symbol: method.symbol().as_identifier().clone(),
            c_function: Identifier::parse(method.c_function().name())?,
            return_type: method.returns().jni_type(),
            c_result_type: method.returns().c_result_type()?,
            parameters: method
                .parameters()
                .iter()
                .flat_map(NativeParameterView::from_parameter)
                .collect(),
            has_error_label: !borrowed_arrays.is_empty()
                || !direct_buffers.is_empty()
                || !record_buffers.is_empty(),
            borrowed_arrays,
            direct_buffers,
            record_buffers,
            arguments: method.arguments()?,
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback: method.returns_callback(),
            return_value: method.returns().return_expression(Expression::identifier(
                Identifier::parse("__boltffi_result")?,
            ))?,
            checks_status: method.checks_status(),
            checks_completion_status: method.checks_completion_status(),
            checks_error_buffer: method.checks_error_buffer(),
            success_out: method.success_out().cloned(),
            uses_callback_parameters: method
                .parameters()
                .iter()
                .any(|parameter| parameter.is_callback()),
            uses_continuations: method
                .parameters()
                .iter()
                .any(|parameter| parameter.is_continuation()),
        })
    }
}
