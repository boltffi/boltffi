use crate::ir::abi::{AbiCall, AbiRecord, CallId};
use crate::ir::definitions::FunctionDef;
use crate::ir::ids::RecordId;

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
}
