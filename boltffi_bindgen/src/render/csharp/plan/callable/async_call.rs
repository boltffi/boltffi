//! Shared C# async runtime entry-point vocabulary.

use super::super::super::ast::CSharpMethodName;
use super::super::CFunctionName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSharpAsyncCallPlan {
    /// C-side async poll symbol.
    pub poll_ffi_name: CFunctionName,
    /// C-side async complete symbol.
    pub complete_ffi_name: CFunctionName,
    /// C-side async cancel symbol.
    pub cancel_ffi_name: CFunctionName,
    /// C-side async free symbol.
    pub free_ffi_name: CFunctionName,
    /// DllImport method name for poll.
    pub poll_method_name: CSharpMethodName,
    /// DllImport method name for complete.
    pub complete_method_name: CSharpMethodName,
    /// DllImport method name for cancel.
    pub cancel_method_name: CSharpMethodName,
    /// DllImport method name for free.
    pub free_method_name: CSharpMethodName,
}
