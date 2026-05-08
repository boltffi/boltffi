use crate::ir::definitions::StreamMode;

use super::super::ast::{CSharpComment, CSharpMethodName, CSharpType};
use super::CFunctionName;

#[derive(Debug, Clone)]
pub struct CSharpStreamPlan {
    pub summary_doc: Option<CSharpComment>,
    pub name: CSharpMethodName,
    pub item_type: CSharpType,
    pub mode: StreamMode,
    pub subscribe_method_name: CSharpMethodName,
    pub subscribe_ffi_name: CFunctionName,
    pub pop_batch_method_name: CSharpMethodName,
    pub pop_batch_ffi_name: CFunctionName,
    pub wait_method_name: CSharpMethodName,
    pub wait_ffi_name: CFunctionName,
    pub unsubscribe_method_name: CSharpMethodName,
    pub unsubscribe_ffi_name: CFunctionName,
    pub free_method_name: CSharpMethodName,
    pub free_ffi_name: CFunctionName,
}
