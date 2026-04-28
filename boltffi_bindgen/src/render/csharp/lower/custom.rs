//! Custom-type erasure for the C# backend.
//!
//! `#[custom_ffi]` and `custom_type!` introduce Rust-side wrapper types
//! (`Email`, `UtcDateTime`) whose wire form is a different `TypeExpr`
//! (`String`, `i64`). The macro already substitutes `Custom` for `repr`
//! when emitting the ABI, so by the time codegen runs `Vec<UtcDateTime>`
//! is classified the same as `Vec<i64>`. The C# backend mirrors that:
//! every entry point that pattern-matches on `TypeExpr` / `ReadOp` /
//! `WriteOp` / `SizeExpr` first runs the value through the helpers
//! below, replacing each `Custom { underlying }` with what's inside.
//! Downstream lowering then never sees a `Custom` variant.

use crate::ir::definitions::CustomTypeDef;
use crate::ir::ids::CustomTypeId;
use crate::ir::ops::{FieldReadOp, FieldWriteOp, ReadOp, ReadSeq, SizeExpr, WriteOp, WriteSeq};
use crate::ir::types::TypeExpr;

use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    pub(super) fn resolve_custom_type(&self, id: &CustomTypeId) -> &CustomTypeDef {
        self.ffi
            .catalog
            .resolve_custom(id)
            .unwrap_or_else(|| panic!("custom type should be in the catalog: {id:?}"))
    }

    pub(super) fn custom_repr_type(&self, id: &CustomTypeId) -> &TypeExpr {
        &self.resolve_custom_type(id).repr
    }

    /// Resolves `Custom` recursively through compound types so a
    /// `Vec<Custom<UtcDateTime>>` flattens to `Vec<i64>` and a chained
    /// `Custom<X>` whose repr is itself `Custom<Y>` lands on the leaf
    /// repr in one call.
    pub(super) fn normalize_custom_type_expr(&self, ty: &TypeExpr) -> TypeExpr {
        match ty {
            TypeExpr::Custom(id) => self.normalize_custom_type_expr(self.custom_repr_type(id)),
            TypeExpr::Option(inner) => {
                TypeExpr::Option(Box::new(self.normalize_custom_type_expr(inner)))
            }
            TypeExpr::Vec(inner) => TypeExpr::Vec(Box::new(self.normalize_custom_type_expr(inner))),
            TypeExpr::Result { ok, err } => TypeExpr::Result {
                ok: Box::new(self.normalize_custom_type_expr(ok)),
                err: Box::new(self.normalize_custom_type_expr(err)),
            },
            _ => ty.clone(),
        }
    }

    /// `SizeExpr` carries no `Custom` variant directly; it just needs to
    /// recurse through the compound shapes so a normalized `WriteSeq`
    /// stays internally consistent. Static because it never inspects the
    /// catalog.
    pub(super) fn normalize_custom_size_expr(size: &SizeExpr) -> SizeExpr {
        match size {
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: value.clone(),
                inner: Box::new(Self::normalize_custom_size_expr(inner)),
            },
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: value.clone(),
                inner: Box::new(Self::normalize_custom_size_expr(inner)),
                layout: layout.clone(),
            },
            SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
                value: value.clone(),
                ok: Box::new(Self::normalize_custom_size_expr(ok)),
                err: Box::new(Self::normalize_custom_size_expr(err)),
            },
            SizeExpr::Sum(parts) => {
                SizeExpr::Sum(parts.iter().map(Self::normalize_custom_size_expr).collect())
            }
            _ => size.clone(),
        }
    }

    pub(super) fn normalize_custom_read_seq(&self, seq: &ReadSeq) -> ReadSeq {
        if let Some(ReadOp::Custom { underlying, .. }) = seq.ops.first() {
            return self.normalize_custom_read_seq(underlying);
        }
        ReadSeq {
            size: Self::normalize_custom_size_expr(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.normalize_custom_read_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn normalize_custom_read_op(&self, op: &ReadOp) -> ReadOp {
        match op {
            ReadOp::Option { tag_offset, some } => ReadOp::Option {
                tag_offset: tag_offset.clone(),
                some: Box::new(self.normalize_custom_read_seq(some)),
            },
            ReadOp::Vec {
                len_offset,
                element_type,
                element,
                layout,
            } => ReadOp::Vec {
                len_offset: len_offset.clone(),
                element_type: self.normalize_custom_type_expr(element_type),
                element: Box::new(self.normalize_custom_read_seq(element)),
                layout: layout.clone(),
            },
            ReadOp::Record { id, offset, fields } => ReadOp::Record {
                id: id.clone(),
                offset: offset.clone(),
                fields: fields
                    .iter()
                    .map(|field| FieldReadOp {
                        name: field.name.clone(),
                        seq: self.normalize_custom_read_seq(&field.seq),
                    })
                    .collect(),
            },
            ReadOp::Result {
                tag_offset,
                ok,
                err,
            } => ReadOp::Result {
                tag_offset: tag_offset.clone(),
                ok: Box::new(self.normalize_custom_read_seq(ok)),
                err: Box::new(self.normalize_custom_read_seq(err)),
            },
            ReadOp::Custom { underlying, .. } => self
                .normalize_custom_read_seq(underlying)
                .ops
                .into_iter()
                .next()
                .expect("normalized custom read seq cannot be empty"),
            _ => op.clone(),
        }
    }

    pub(super) fn normalize_custom_write_seq(&self, seq: &WriteSeq) -> WriteSeq {
        if let Some(WriteOp::Custom { underlying, .. }) = seq.ops.first() {
            return self.normalize_custom_write_seq(underlying);
        }
        WriteSeq {
            size: Self::normalize_custom_size_expr(&seq.size),
            ops: seq
                .ops
                .iter()
                .map(|op| self.normalize_custom_write_op(op))
                .collect(),
            shape: seq.shape,
        }
    }

    fn normalize_custom_write_op(&self, op: &WriteOp) -> WriteOp {
        match op {
            WriteOp::Option { value, some } => WriteOp::Option {
                value: value.clone(),
                some: Box::new(self.normalize_custom_write_seq(some)),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: value.clone(),
                element_type: self.normalize_custom_type_expr(element_type),
                element: Box::new(self.normalize_custom_write_seq(element)),
                layout: layout.clone(),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: value.clone(),
                fields: fields
                    .iter()
                    .map(|field| FieldWriteOp {
                        name: field.name.clone(),
                        accessor: field.accessor.clone(),
                        seq: self.normalize_custom_write_seq(&field.seq),
                    })
                    .collect(),
            },
            WriteOp::Result { value, ok, err } => WriteOp::Result {
                value: value.clone(),
                ok: Box::new(self.normalize_custom_write_seq(ok)),
                err: Box::new(self.normalize_custom_write_seq(err)),
            },
            WriteOp::Custom { underlying, .. } => self
                .normalize_custom_write_seq(underlying)
                .ops
                .into_iter()
                .next()
                .expect("normalized custom write seq cannot be empty"),
            _ => op.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::ids::{ConverterPath, CustomTypeId, QualifiedName};
    use crate::ir::types::{PrimitiveType, TypeExpr};

    use super::super::super::CSharpOptions;

    fn stub_converters() -> ConverterPath {
        ConverterPath {
            into_ffi: QualifiedName::new("test_into_ffi"),
            try_from_ffi: QualifiedName::new("test_try_from_ffi"),
        }
    }

    fn datetime_custom() -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new("UtcDateTime"),
            rust_type: QualifiedName::new("chrono::DateTime<Utc>"),
            repr: TypeExpr::Primitive(PrimitiveType::I64),
            converters: stub_converters(),
            doc: None,
        }
    }

    fn email_custom() -> CustomTypeDef {
        CustomTypeDef {
            id: CustomTypeId::new("Email"),
            rust_type: QualifiedName::new("demo::Email"),
            repr: TypeExpr::String,
            converters: stub_converters(),
            doc: None,
        }
    }

    fn empty_contract_with(customs: Vec<CustomTypeDef>) -> FfiContract {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo".into(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        for c in customs {
            contract.catalog.insert_custom(c);
        }
        contract
    }

    #[test]
    fn custom_resolves_to_primitive_repr() {
        let contract = empty_contract_with(vec![datetime_custom()]);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let normalized =
            lowerer.normalize_custom_type_expr(&TypeExpr::Custom(CustomTypeId::new("UtcDateTime")));

        assert!(matches!(
            normalized,
            TypeExpr::Primitive(PrimitiveType::I64)
        ));
    }

    #[test]
    fn vec_of_custom_resolves_to_vec_of_repr() {
        let contract = empty_contract_with(vec![datetime_custom()]);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let normalized = lowerer.normalize_custom_type_expr(&TypeExpr::Vec(Box::new(
            TypeExpr::Custom(CustomTypeId::new("UtcDateTime")),
        )));

        match normalized {
            TypeExpr::Vec(inner) => {
                assert!(matches!(*inner, TypeExpr::Primitive(PrimitiveType::I64)))
            }
            other => panic!("expected Vec<i64>, got {other:?}"),
        }
    }

    #[test]
    fn option_of_custom_string_resolves_to_option_string() {
        let contract = empty_contract_with(vec![email_custom()]);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let normalized = lowerer.normalize_custom_type_expr(&TypeExpr::Option(Box::new(
            TypeExpr::Custom(CustomTypeId::new("Email")),
        )));

        match normalized {
            TypeExpr::Option(inner) => assert!(matches!(*inner, TypeExpr::String)),
            other => panic!("expected Option<String>, got {other:?}"),
        }
    }
}
