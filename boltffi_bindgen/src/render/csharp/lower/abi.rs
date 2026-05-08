use crate::ir::abi::{AbiCall, AbiRecord, AbiStream, CallId};
use crate::ir::definitions::{FunctionDef, StreamDef};
use crate::ir::ids::{ClassId, RecordId};

use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Linear lookup of an ABI call by its `Function` ID.
    pub(super) fn abi_call_for_function(&self, function: &FunctionDef) -> Option<&AbiCall> {
        self.abi.calls.iter().find(|call| match &call.id {
            CallId::Function(id) => id == &function.id,
            _ => false,
        })
    }

    /// Linear lookup of an ABI record by ID.
    pub(super) fn abi_record_for(&self, record_id: &RecordId) -> Option<&AbiRecord> {
        self.abi
            .records
            .iter()
            .find(|record| record.id == *record_id)
    }

    /// Linear lookup of an ABI stream by owning class and stream ID.
    pub(super) fn abi_stream_for(
        &self,
        class_id: &ClassId,
        stream: &StreamDef,
    ) -> Option<&AbiStream> {
        self.abi.streams.iter().find(|abi_stream| {
            abi_stream.class_id == *class_id && abi_stream.stream_id == stream.id
        })
    }
}
