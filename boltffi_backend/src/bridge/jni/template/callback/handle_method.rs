//! Source fields for calling Rust-owned callback handles from the JVM.
//!
//! Returned callback handles live in native storage and expose their methods
//! through a C vtable. The generated JNI method takes the handle token from
//! Java, validates the stored vtable, prepares the Java-provided arguments, and
//! invokes the matching vtable slot.
//!
//! This module prepares the source view for that method body. It borrows the
//! same parameter, array, record, and return views used by normal native methods,
//! so a returned callback handle does not get a second conversion model just
//! because the call starts from a `jlong` token.

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, Statement, TypeFragment},
        jni::{
            CallbackCompletionPayload, CallbackHandleClosureReturn, CallbackHandleCompletion,
            CallbackHandleMethod, SuccessOutReturn,
            name::LookupText,
            template::method::{
                BorrowedArrayParameterView, DirectBufferParameterView, NativeParameterView,
                RecordParameterView,
            },
        },
    },
    core::Result,
};

pub struct CallbackHandleMethodView {
    pub symbol: Identifier,
    pub return_type: TypeFragment,
    pub c_result_type: TypeFragment,
    pub vtable_type: Identifier,
    pub slot: Identifier,
    pub parameters: Vec<NativeParameterView>,
    pub borrowed_arrays: Vec<BorrowedArrayParameterView>,
    pub direct_buffers: Vec<DirectBufferParameterView>,
    pub record_buffers: Vec<RecordParameterView>,
    pub arguments: ArgumentList,
    pub completion: Option<CallbackHandleCompletionView>,
    pub returns_void: bool,
    pub returns_boolean: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback: bool,
    pub returns_closure: bool,
    pub return_value: Expression,
    pub checks_status: bool,
    pub checks_completion_status: bool,
    pub checks_error_buffer: bool,
    pub success_out: Option<SuccessOutReturn>,
    pub closure_return: Option<CallbackHandleClosureReturnView>,
}

impl CallbackHandleMethodView {
    pub fn from_method(method: &CallbackHandleMethod) -> Result<Self> {
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
            return_type: method.jni_type(),
            c_result_type: method.c_result_type()?,
            vtable_type: method.vtable_type().clone(),
            slot: method.slot().clone(),
            parameters: method
                .parameters()
                .iter()
                .flat_map(NativeParameterView::from_parameter)
                .collect(),
            borrowed_arrays,
            direct_buffers,
            record_buffers,
            arguments: method.arguments()?,
            completion: method
                .completion()
                .map(CallbackHandleCompletionView::from_completion),
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback: method.returns_callback(),
            returns_closure: method.returns_closure(),
            return_value: method
                .return_value(Expression::identifier(Identifier::parse("result")?))?,
            checks_status: method.checks_status(),
            checks_completion_status: false,
            checks_error_buffer: method.checks_error_buffer(),
            success_out: method.success_out().cloned(),
            closure_return: method
                .closure_return()
                .map(CallbackHandleClosureReturnView::from_return),
        })
    }
}

pub struct CallbackHandleCompletionView {
    pub function: Identifier,
    pub context: Identifier,
    pub success_method: LookupText,
    pub success_method_id: Identifier,
    pub success_signature: LookupText,
    pub failure_method: LookupText,
    pub failure_method_id: Identifier,
    pub failure_signature: LookupText,
    pub has_payload: bool,
    pub payload_c_type: TypeFragment,
    pub payload_jni_type: TypeFragment,
    pub payload_bytes: bool,
    pub payload_record: bool,
    pub payload_callback_handle: bool,
}

pub struct CallbackHandleClosureReturnView {
    pub storage: Identifier,
    pub invoke_field: Statement,
    pub local: Identifier,
}

impl CallbackHandleClosureReturnView {
    pub fn from_return(returned: &CallbackHandleClosureReturn) -> Self {
        Self {
            storage: returned.storage().clone(),
            invoke_field: returned.invoke_field().clone(),
            local: returned.local().clone(),
        }
    }
}

impl CallbackHandleCompletionView {
    pub fn from_completion(completion: &CallbackHandleCompletion) -> Self {
        let payload = completion.payload();
        Self {
            function: completion.function().clone(),
            context: completion.context().clone(),
            success_method: LookupText::new(completion.success_method().as_str()),
            success_method_id: completion.success_method_id().clone(),
            success_signature: LookupText::new(completion.success_signature()),
            failure_method: LookupText::new(completion.failure_method().as_str()),
            failure_method_id: completion.failure_method_id().clone(),
            failure_signature: LookupText::new("(J)V"),
            has_payload: payload.is_some(),
            payload_c_type: payload
                .map(CallbackCompletionPayload::c_type)
                .cloned()
                .unwrap_or_else(|| TypeFragment::new("void")),
            payload_jni_type: payload
                .map(CallbackCompletionPayload::jni_type)
                .cloned()
                .unwrap_or_else(|| TypeFragment::new("void")),
            payload_bytes: payload.is_some_and(CallbackCompletionPayload::is_bytes),
            payload_record: payload.is_some_and(CallbackCompletionPayload::is_record),
            payload_callback_handle: payload
                .and_then(CallbackCompletionPayload::callback_handle_constructor)
                .is_some(),
        }
    }
}
