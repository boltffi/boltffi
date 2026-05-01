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
    /// `Custom` resolves through to its `repr` since the lowerer erases
    /// it before emit.
    pub(super) fn is_supported_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Void => true,
            TypeExpr::Record(id) => self.supported_records.contains(id),
            TypeExpr::Enum(id) => self.supported_enums.contains(id),
            TypeExpr::Custom(id) => self.is_supported_type(self.custom_repr_type(id)),
            TypeExpr::Vec(inner) => self.is_supported_vec_element(inner),
            TypeExpr::Option(inner) => {
                !matches!(inner.as_ref(), TypeExpr::Option(_)) && self.is_supported_type(inner)
            }
            _ => false,
        }
    }

    /// Whether `ty` is admissible as the Ok or Err side of a
    /// `Result<Ok, Err>` return. Same gate as [`is_supported_type`]
    /// plus an explicit allow for `Void` (a `Result<(), E>` Ok is
    /// legal even though void isn't a normal return).
    pub(super) fn is_supported_result_type(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Void => true,
            other => self.is_supported_type(other),
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
            TypeExpr::Custom(id) => self.is_supported_vec_element(self.custom_repr_type(id)),
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
    /// `#[repr(C)]`). `Custom` resolves through to its `repr` so a
    /// `Vec<UtcDateTime>` (i64 underneath) rides the pinned-array path
    /// the macro already produced ABI-side. C-style enums do NOT qualify:
    /// the Rust `#[export]` macro classifies them as
    /// `DataTypeCategory::Scalar` and routes `Vec<CStyleEnum>` through
    /// the wire-encoded path. Admitting them here would mismatch the
    /// ABI. Tracked in issue #196.
    pub(super) fn is_blittable_vec_element(&self, ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Primitive(_) => true,
            TypeExpr::Record(id) => self.is_blittable_record(id),
            TypeExpr::Custom(id) => self.is_blittable_vec_element(self.custom_repr_type(id)),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lowerer::CSharpLowerer;
    use super::*;
    use crate::ir::FfiContract;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::PackageInfo;
    use crate::ir::types::PrimitiveType;

    use super::super::super::CSharpOptions;

    fn empty_lowerer_inputs() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    /// `Result<(), E>` is legal — Java exposes it as a `void` returning
    /// throwing method, and C# does the same. The Ok-side admission gate
    /// has to allow `Void` even though plain `Void` returns can't carry
    /// a `Result` payload.
    #[test]
    fn is_supported_result_type_admits_void() {
        let contract = empty_lowerer_inputs();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        assert!(
            lowerer.is_supported_result_type(&TypeExpr::Void),
            "expecting Result<(), E> Ok side to admit Void",
        );
    }

    /// Anything `is_supported_type` allows on a normal return is also
    /// admissible inside a `Result<_, _>`, so the wrapper can wire-decode
    /// the same shapes the rest of the backend already handles.
    #[test]
    fn is_supported_result_type_accepts_supported_types() {
        let contract = empty_lowerer_inputs();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        for ty in [
            TypeExpr::Primitive(PrimitiveType::I32),
            TypeExpr::String,
            TypeExpr::Vec(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
        ] {
            assert!(
                lowerer.is_supported_result_type(&ty),
                "expecting {ty:?} to admit as a Result Ok/Err type",
            );
        }
    }

    /// The shapes `is_supported_type` rejects also fail the Result gate
    /// — the Result branch is a thin Void-tolerant wrapper around the
    /// plain support gate, not an escape hatch for unsupported types.
    #[test]
    fn is_supported_result_type_rejects_nested_options_and_results() {
        let contract = empty_lowerer_inputs();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let nested_option = TypeExpr::Option(Box::new(TypeExpr::Option(Box::new(
            TypeExpr::Primitive(PrimitiveType::I32),
        ))));
        assert!(
            !lowerer.is_supported_result_type(&nested_option),
            "expecting Option<Option<i32>> to fail the Result admission gate",
        );
    }
}
