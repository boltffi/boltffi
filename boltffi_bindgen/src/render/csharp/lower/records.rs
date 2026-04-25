use crate::ir::definitions::{FieldDef, RecordDef};
use crate::ir::ids::{FieldName, RecordId};
use crate::ir::ops::{ReadOp, ReadSeq, WriteOp, WriteSeq};

use super::super::ast::{CSharpClassName, CSharpExpression, CSharpIdentity, CSharpLocalName};
use super::super::plan::{CSharpFieldPlan, CSharpRecordPlan};
use super::lowerer::CSharpLowerer;
use super::{decode, encode, size, value};

impl<'a> CSharpLowerer<'a> {
    /// Lowers a record's fields and computes its blittability into a plan.
    pub(super) fn lower_record(&self, record: &RecordDef) -> CSharpRecordPlan {
        let class_name: CSharpClassName = (&record.id).into();
        // Share one emit context across all fields so pattern-binding
        // names (e.g. `sizeOpt0`, `opt0`) stay unique within the same
        // `WireEncodedSize`/`WireEncodeTo` method body.
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let mut decode_locals = decode::DecodeLocalCounters::default();
        let fields = record
            .fields
            .iter()
            .map(|field| {
                self.lower_record_field(
                    &record.id,
                    field,
                    &mut size_locals,
                    &mut encode_locals,
                    &mut decode_locals,
                )
            })
            .collect();
        let is_blittable = self.is_blittable_record(&record.id);
        CSharpRecordPlan {
            class_name,
            fields,
            is_blittable,
        }
    }

    /// Lowers one record field, materializing the decode/size/encode trees
    /// from the ABI's per-field op sequences. Panics if the field is
    /// missing from the ABI record (an invariant of validated contracts).
    fn lower_record_field(
        &self,
        record_id: &RecordId,
        field: &FieldDef,
        size_locals: &mut size::SizeLocalCounters,
        encode_locals: &mut encode::EncodeLocalCounters,
        decode_locals: &mut decode::DecodeLocalCounters,
    ) -> CSharpFieldPlan {
        let decode_seq = self
            .record_field_read_seq(record_id, &field.name)
            .expect("record field decode ops");
        let encode_seq = self
            .record_field_write_seq(record_id, &field.name)
            .expect("record field encode ops");
        let csharp_type = self
            .lower_type(&field.type_expr)
            .expect("record field type must be supported");
        CSharpFieldPlan {
            name: (&field.name).into(),
            csharp_type,
            wire_decode_expr: decode::lower_decode_expr(
                &decode_seq,
                &CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("reader"))),
                None,
                &self.namespace,
                decode_locals,
            ),
            wire_size_expr: size::lower_size_expr(
                &encode_seq.size,
                &value::Renames::new(),
                size_locals,
            ),
            wire_encode_stmts: encode::lower_encode_expr(
                &encode_seq,
                &CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("wire"))),
                &value::Renames::new(),
                encode_locals,
            ),
        }
    }

    /// Per-field decode sequence for `field_name` in the given record,
    /// or `None` if the record isn't in the ABI catalog or its top-level
    /// decode op isn't `ReadOp::Record`.
    fn record_field_read_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<ReadSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.decode_ops.ops.first() {
                Some(ReadOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }

    /// Per-field encode sequence for `field_name` in the given record,
    /// or `None` if the record isn't in the ABI catalog or its top-level
    /// encode op isn't `WriteOp::Record`.
    fn record_field_write_seq(
        &self,
        record_id: &RecordId,
        field_name: &FieldName,
    ) -> Option<WriteSeq> {
        self.abi_record_for(record_id)
            .and_then(|record| match record.encode_ops.ops.first() {
                Some(WriteOp::Record { fields, .. }) => fields
                    .iter()
                    .find(|field| field.name == *field_name)
                    .map(|field| field.seq.clone()),
                _ => None,
            })
    }
}
