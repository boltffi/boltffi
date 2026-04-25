use crate::ir::definitions::{EnumRepr, ParamDef, ParamPassing};
use crate::ir::ids::{EnumId, RecordId};
use crate::ir::types::TypeExpr;

use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Whether the enum has data-carrying variants. Data enums travel as
    /// wire-encoded `byte[]` payloads; C-style enums marshal as their
    /// integral backing type.
    pub(super) fn is_data_enum(&self, id: &EnumId) -> bool {
        self.ffi
            .catalog
            .resolve_enum(id)
            .is_some_and(|e| matches!(e.repr, EnumRepr::Data { .. }))
    }

    /// Whether the record passes directly across P/Invoke by value with
    /// `[StructLayout(Sequential)]` and no wire encoding. Defers to the
    /// ABI's precomputed `is_blittable` flag (set by the Rust `#[export]`
    /// macro). Widening this without teaching the macro would mismatch
    /// C#'s call site against the symbol's ABI and segfault at runtime.
    pub(super) fn is_blittable_record(&self, id: &RecordId) -> bool {
        self.abi_record_for(id).is_some_and(|r| r.is_blittable)
    }

    /// Whether the param can be handled by the C# backend. Today only
    /// by-value passing is supported (no `&` / `&mut`).
    pub(super) fn is_supported_param(&self, param: &ParamDef) -> bool {
        param.passing == ParamPassing::Value && self.is_supported_type(&param.type_expr)
    }

    /// Whether the type can appear as a function param or return today.
    /// Records and enums must be admitted via the supported-set fixed
    /// point; nested options are rejected because C# can't express `T??`.
    pub(super) fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => self.supported_records.contains(id),
            TypeExpr::Enum(id) => self.supported_enums.contains(id),
            TypeExpr::Vec(inner) => self.is_supported_vec_element(inner),
            TypeExpr::Option(inner) => {
                !matches!(inner.as_ref(), TypeExpr::Option(_)) && self.is_supported_type(inner)
            }
            _ => false,
        }
    }

    /// Which element types the C# backend currently admits inside a
    /// top-level `Vec<_>` param or return. This is only the admission
    /// gate: primitives and blittable records can use the blittable
    /// path; strings, enums, non-blittable records, and nested vecs
    /// travel through the encoded wire form.
    pub(super) fn is_supported_vec_element(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String => true,
            TypeExpr::Record(id) => self.supported_records.contains(id),
            TypeExpr::Enum(id) => self.supported_enums.contains(id),
            TypeExpr::Vec(inner) => self.is_supported_vec_element(inner),
            TypeExpr::Option(inner) => {
                !matches!(inner.as_ref(), TypeExpr::Option(_))
                    && self.is_supported_vec_element(inner)
            }
            _ => false,
        }
    }

    /// Vec element types that pass directly as a pinned `T[]` across
    /// P/Invoke. Primitives qualify (blittable C# value types). Blittable
    /// records qualify (`[StructLayout(Sequential)]` matches Rust
    /// `#[repr(C)]`). C-style enums do NOT qualify: the Rust `#[export]`
    /// macro classifies them as `DataTypeCategory::Scalar` and routes
    /// `Vec<CStyleEnum>` through the wire-encoded path. Admitting them
    /// here would mismatch the ABI. Tracked in issue #196.
    pub(super) fn is_blittable_vec_element(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) => true,
            TypeExpr::Record(id) => self.is_blittable_record(id),
            _ => false,
        }
    }
}
