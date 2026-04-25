use crate::ir::definitions::{ParamDef, ParamPassing, ReturnDef};
use crate::ir::ids::ParamName;
use crate::ir::types::TypeExpr;

use super::super::ast::{
    CSharpClassName, CSharpLocalName, CSharpParamName, CSharpType, CSharpTypeReference,
};
use super::super::plan::{CSharpParamKind, CSharpParamPlan, CSharpWireWriterPlan};
use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Lowers a Rust param to a [`CSharpParamPlan`]. Returns `None` for
    /// non-by-value passing, unsupported types, or wire-encoded params
    /// whose `wire_writer` isn't pre-registered.
    pub(super) fn lower_param(
        &self,
        param: &ParamDef,
        wire_writers: &[CSharpWireWriterPlan],
    ) -> Option<CSharpParamPlan> {
        if param.passing != ParamPassing::Value {
            return None;
        }

        let csharp_type = self.lower_type(&param.type_expr)?;
        let csharp_param_name: CSharpParamName = (&param.name).into();
        let kind = match &param.type_expr {
            TypeExpr::String => CSharpParamKind::Utf8Bytes,
            TypeExpr::Record(id) if !self.is_blittable_record(id) => {
                wire_encoded_kind(wire_writers, &param.name)?
            }
            TypeExpr::Enum(id) if self.is_data_enum(id) => {
                wire_encoded_kind(wire_writers, &param.name)?
            }
            TypeExpr::Vec(inner) if matches!(inner.as_ref(), TypeExpr::Primitive(_)) => {
                CSharpParamKind::DirectArray
            }
            TypeExpr::Vec(inner) if self.is_blittable_vec_element(inner) => {
                // Primitive arrays can use the CLR's built-in direct-array
                // path. Record arrays are less predictable once the element
                // type stops being blittable to the marshaller, e.g. because
                // it contains `bool` or `char`: P/Invoke may marshal through
                // a temporary native buffer rather than exposing the managed
                // array in place. `fixed` keeps this path zero-copy and
                // makes the ABI contract explicit: Rust reads the actual
                // managed element buffer, not a marshaled surrogate.
                let element_type = match inner.as_ref() {
                    TypeExpr::Record(id) => {
                        CSharpType::Record(CSharpTypeReference::Plain(id.into()))
                    }
                    other => todo!(
                        "C# backend pinned-array param support not yet implemented for {other:?}"
                    ),
                };
                CSharpParamKind::PinnedArray {
                    element_type,
                    ptr_local: CSharpLocalName::for_pinned_ptr(&csharp_param_name),
                }
            }
            TypeExpr::Vec(inner) if self.is_supported_vec_element(inner) => {
                // Vec<String> and Vec<Vec<_>> carry variable-width elements, so
                // the param travels wire-encoded rather than as a pinned T[].
                wire_encoded_kind(wire_writers, &param.name)?
            }
            TypeExpr::Option(_) => {
                // Options are always wire-encoded: the 1-byte tag plus an
                // optional payload does not line up with any CLR
                // primitive layout.
                wire_encoded_kind(wire_writers, &param.name)?
            }
            // Primitives, bools, blittable records, and C-style enums
            // pass directly. The CLR marshals them across P/Invoke with
            // no extra setup.
            _ => CSharpParamKind::Direct,
        };

        Some(CSharpParamPlan {
            name: csharp_param_name,
            csharp_type,
            kind,
        })
    }

    /// Lowers a return signature to its C# type. `Result<_, _>` returns
    /// aren't supported yet.
    pub(super) fn lower_return(&self, return_def: &ReturnDef) -> Option<CSharpType> {
        match return_def {
            ReturnDef::Void => Some(CSharpType::Void),
            ReturnDef::Value(type_expr) => self.lower_type(type_expr),
            ReturnDef::Result { .. } => None,
        }
    }

    /// Maps a Rust [`TypeExpr`] to its C# equivalent. Returns `None` if
    /// the type isn't admitted by the backend (callbacks, streams,
    /// records/enums outside the supported sets).
    pub(super) fn lower_type(&self, type_expr: &TypeExpr) -> Option<CSharpType> {
        match type_expr {
            TypeExpr::Void => Some(CSharpType::Void),
            TypeExpr::Primitive(primitive) => Some(CSharpType::from(*primitive)),
            TypeExpr::String => Some(CSharpType::String),
            TypeExpr::Record(id) if self.supported_records.contains(id) => {
                let class_name: CSharpClassName = id.into();
                Some(CSharpType::Record(class_name.into()))
            }
            TypeExpr::Enum(id) if self.supported_enums.contains(id) => {
                let enum_def = self.ffi.catalog.resolve_enum(id)?;
                Some(CSharpType::for_enum(enum_def))
            }
            TypeExpr::Vec(inner) if self.is_supported_vec_element(inner) => {
                let inner_type = self.lower_type(inner)?;
                Some(CSharpType::Array(Box::new(inner_type)))
            }
            TypeExpr::Option(inner) => {
                let inner_type = self.lower_type(inner)?;
                Some(CSharpType::Nullable(Box::new(inner_type)))
            }
            _ => None,
        }
    }
}

/// Builds the [`CSharpParamKind::WireEncoded`] variant by finding the
/// matching pre-registered wire writer. Returns `None` if no writer with
/// the given param name exists (caller propagates).
fn wire_encoded_kind(
    wire_writers: &[CSharpWireWriterPlan],
    param_name: &ParamName,
) -> Option<CSharpParamKind> {
    let writer = wire_writers
        .iter()
        .find(|w| w.param_name.as_str() == param_name.as_str())?;
    Some(CSharpParamKind::WireEncoded {
        binding_name: writer.bytes_binding_name.clone(),
    })
}
