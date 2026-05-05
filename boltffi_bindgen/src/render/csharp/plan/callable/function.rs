//! [`CSharpFunctionPlan`] (a top-level primitive function binding).
//! The binding serves the public wrapper method and the `[DllImport]`
//! native declaration at once.

use super::super::super::ast::{
    CSharpArgumentList, CSharpComment, CSharpMethodName, CSharpParameterList, CSharpType,
};
use super::super::CFunctionName;
use super::async_call::CSharpAsyncCallPlan;
use super::callable_plan::CSharpCallablePlan;
use super::param::{native_call_arg_list, native_param_list};
use super::return_kind::CSharpReturnKind;
use super::{CSharpParamPlan, CSharpWireWriterPlan};

/// A top-level function binding. Serves double duty: drives both the public
/// static wrapper method and the matching `[DllImport]` declaration.
///
/// Examples:
/// ```csharp
/// // Public wrapper (functions.txt)
/// public static int Echo(int value)
/// {
///     return NativeMethods.Echo(value);
/// }
///
/// // DllImport declaration (native.txt)
/// [DllImport(LibName, EntryPoint = "boltffi_echo")]
/// internal static extern int Echo(int value);
/// ```
#[derive(Debug, Clone)]
pub struct CSharpFunctionPlan {
    /// Renders a `<summary>` block comment, when `Some`.
    pub summary_doc: Option<CSharpComment>,
    /// Public wrapper method name.
    pub name: CSharpMethodName,
    /// Parameters with C# types.
    pub params: Vec<CSharpParamPlan>,
    /// C# return type as it appears in the public wrapper signature.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI. Drives how the wrapper body
    /// decodes the native return and what the `[DllImport]` signature looks
    /// like.
    pub return_kind: CSharpReturnKind,
    /// The C function this wrapper calls across the ABI boundary.
    pub ffi_name: CFunctionName,
    /// Async poll / complete / cancel / free entry points when this
    /// wrapper starts a Rust future instead of completing synchronously.
    pub async_call: Option<CSharpAsyncCallPlan>,
    /// For each non-blittable record param, the setup code that wire-encodes
    /// it into a `byte[]` before the native call.
    pub wire_writers: Vec<CSharpWireWriterPlan>,
}

impl CSharpFunctionPlan {
    /// Typed param list for the `[DllImport]` native signature.
    pub fn native_param_list(&self) -> CSharpParameterList {
        native_param_list(&self.params)
    }

    /// Typed argument list for the native invocation.
    pub fn native_call_args(&self) -> CSharpArgumentList {
        native_call_arg_list(&self.params)
    }

    /// Whether the function has any
    /// [`CSharpParamKind::PinnedArray`](super::CSharpParamKind::PinnedArray)
    /// param. Gates the `unsafe { fixed (...) { ... } }` scaffolding in
    /// the wrapper template.
    pub fn has_pinned_params(&self) -> bool {
        self.params.iter().any(CSharpParamPlan::is_pinned)
    }
}

impl CSharpCallablePlan for CSharpFunctionPlan {
    fn async_call(&self) -> Option<&CSharpAsyncCallPlan> {
        self.async_call.as_ref()
    }

    fn return_type(&self) -> &CSharpType {
        &self.return_type
    }

    fn return_kind(&self) -> &CSharpReturnKind {
        &self.return_kind
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::ast::{CSharpClassName, CSharpLocalName, CSharpParamName};
    use super::super::CSharpParamKind;
    use super::*;

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParamPlan {
        CSharpParamPlan {
            name: super::super::CSharpParamName::from_source(name),
            csharp_type,
            kind,
        }
    }

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(CSharpClassName::from_source(name).into())
    }

    fn function_with_params(
        params: Vec<CSharpParamPlan>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunctionPlan {
        CSharpFunctionPlan {
            summary_doc: None,
            name: CSharpMethodName::from_source("test"),
            params,
            return_type,
            return_kind,
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            async_call: None,
            wire_writers: vec![],
        }
    }

    /// The native param list exposes each slot's marshalling shape: a
    /// string expands to a pair, bool gets a MarshalAs, and primitives
    /// stay bare. Mixed-shape case pins the spacing.
    #[test]
    fn native_param_list_expands_each_slot_by_kind() {
        let f = function_with_params(
            vec![
                param("flag", CSharpType::Bool, CSharpParamKind::Direct),
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
                param(
                    "person",
                    record_type("person"),
                    CSharpParamKind::WireEncoded {
                        binding_name: CSharpLocalName::for_bytes(&CSharpParamName::from_source(
                            "person",
                        )),
                    },
                ),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_param_list().to_string(),
            "[MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, UIntPtr vLen, uint count, byte[] person, UIntPtr personLen",
        );
    }

    #[test]
    fn native_call_args_mirror_param_shapes() {
        let f = function_with_params(
            vec![
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_call_args().to_string(),
            "_vBytes, (UIntPtr)_vBytes.Length, count",
        );
    }
}
