use crate::ir::abi::{AbiCall, CallId};
use crate::ir::definitions::{ConstructorDef, FieldDef, MethodDef, Receiver, RecordDef};
use crate::ir::ids::{FieldName, RecordId};
use crate::ir::ops::{ReadOp, ReadSeq, WriteOp, WriteSeq};

use super::super::ast::{
    CSharpClassName, CSharpComment, CSharpExpression, CSharpIdentity, CSharpLocalName,
    CSharpMethodName, CSharpType,
};
use super::super::plan::{
    CSharpFieldPlan, CSharpMethodPlan, CSharpParamPlan, CSharpReceiver, CSharpRecordPlan,
    CSharpReturnKind,
};
use super::lowerer::CSharpLowerer;
use super::wire_writers::self_wire_writer;
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
        let methods = self.lower_record_methods(record, &class_name, is_blittable);
        CSharpRecordPlan {
            summary_doc: CSharpComment::from_str_option(record.doc.as_deref()),
            class_name,
            fields,
            is_blittable,
            methods,
            is_error: record.is_error,
        }
    }

    /// Walks a record's `#[data(impl)]` constructors and methods and
    /// produces the corresponding [`CSharpMethodPlan`]s. Same filter as
    /// the enum lowerer: fallible/optional constructors, async methods,
    /// and `&mut self` / `self` receivers are dropped silently.
    fn lower_record_methods(
        &self,
        record: &RecordDef,
        record_class_name: &CSharpClassName,
        owner_is_blittable: bool,
    ) -> Vec<CSharpMethodPlan> {
        let mut methods = Vec::new();

        for (index, ctor) in record.constructors.iter().enumerate() {
            if ctor.is_fallible() || ctor.is_optional() {
                continue;
            }
            let call_id = CallId::RecordConstructor {
                record_id: record.id.clone(),
                index,
            };
            let Some(call) = self.abi.calls.iter().find(|c| c.id == call_id) else {
                continue;
            };
            if let Some(method) =
                self.lower_record_constructor(ctor, call, record_class_name, owner_is_blittable)
            {
                methods.push(method);
            }
        }

        for method_def in &record.methods {
            if method_def.is_async() {
                continue;
            }
            if matches!(
                method_def.receiver,
                Receiver::RefMutSelf | Receiver::OwnedSelf
            ) {
                continue;
            }
            let call_id = CallId::RecordMethod {
                record_id: record.id.clone(),
                method_id: method_def.id.clone(),
            };
            let Some(call) = self.abi.calls.iter().find(|c| c.id == call_id) else {
                continue;
            };
            if let Some(method) =
                self.lower_record_method(method_def, call, record_class_name, owner_is_blittable)
            {
                methods.push(method);
            }
        }

        methods
    }

    /// Lowers a `#[data(impl)]` constructor into a static factory method
    /// on the record struct. Blittable records return the struct
    /// directly across P/Invoke; non-blittable records return an
    /// `FfiBuf` that we decode through the record's `WireReader`.
    fn lower_record_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        record_class_name: &CSharpClassName,
        owner_is_blittable: bool,
    ) -> Option<CSharpMethodPlan> {
        let raw_name: &str = match ctor.name() {
            Some(id) => id.as_str(),
            None => "new",
        };
        let name = CSharpMethodName::from_source(raw_name);
        let return_type = CSharpType::Record(record_class_name.clone().into());
        let return_kind = if owner_is_blittable {
            CSharpReturnKind::Direct
        } else {
            CSharpReturnKind::WireDecodeObject {
                class_name: record_class_name.clone(),
            }
        };
        let mut ctor_size_locals = size::SizeLocalCounters::default();
        let mut ctor_encode_locals = encode::EncodeLocalCounters::default();
        let wire_writers: Vec<_> = call
            .params
            .iter()
            .filter_map(|p| {
                self.wire_writer_for_param(p, &mut ctor_size_locals, &mut ctor_encode_locals)
            })
            .collect();
        let param_defs = ctor.params();
        let params: Vec<CSharpParamPlan> = param_defs
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        Some(CSharpMethodPlan {
            summary_doc: CSharpComment::from_str_option(ctor.doc()),
            native_method_name: CSharpMethodName::native_for_owner(record_class_name, &name),
            name,
            ffi_name: (&call.symbol).into(),
            async_call: None,
            receiver: CSharpReceiver::Static,
            params,
            return_type,
            return_kind,
            wire_writers,
            // Static factories don't reference `this`; the flag only
            // matters for `InstanceNative` receivers.
            owner_is_blittable: false,
        })
    }

    /// Lowers a `#[data(impl)]` instance or static method on a record.
    /// Static methods land as `Static`; `&self` / `&mut self` / `self`
    /// receivers all land as `InstanceNative`. The blittable owner flag
    /// drives whether the body wire-encodes `this` or passes it by value.
    fn lower_record_method(
        &self,
        method_def: &MethodDef,
        call: &AbiCall,
        record_class_name: &CSharpClassName,
        owner_is_blittable: bool,
    ) -> Option<CSharpMethodPlan> {
        let name: CSharpMethodName = (&method_def.id).into();
        let return_type = self.lower_return(&method_def.returns)?;
        let return_kind = self.return_kind(
            &method_def.returns,
            &return_type,
            call.returns.decode_ops.as_ref(),
            None,
        );

        let receiver = match method_def.receiver {
            Receiver::Static => CSharpReceiver::Static,
            Receiver::RefSelf | Receiver::RefMutSelf | Receiver::OwnedSelf => {
                CSharpReceiver::InstanceNative
            }
        };
        // Instance methods have a synthetic `self` prepended to the ABI
        // param list; skip it when building wire writers and mapping
        // back to the explicit IR params, which never include `self`.
        let explicit_abi_params = if matches!(receiver, CSharpReceiver::Static) {
            &call.params[..]
        } else {
            &call.params[1..]
        };
        let mut method_size_locals = size::SizeLocalCounters::default();
        let mut method_encode_locals = encode::EncodeLocalCounters::default();
        let mut wire_writers: Vec<_> = Vec::new();
        // Wire-encoded receivers (non-blittable records) need a self
        // wire writer to encode `this` before the call. Blittable
        // records pass `this` by value across P/Invoke and need none.
        if matches!(receiver, CSharpReceiver::InstanceNative) && !owner_is_blittable {
            wire_writers.push(self_wire_writer());
        }
        wire_writers.extend(explicit_abi_params.iter().filter_map(|p| {
            self.wire_writer_for_param(p, &mut method_size_locals, &mut method_encode_locals)
        }));
        let params: Vec<CSharpParamPlan> = method_def
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;
        let owner_is_blittable_for_call =
            matches!(receiver, CSharpReceiver::InstanceNative) && owner_is_blittable;
        Some(CSharpMethodPlan {
            summary_doc: CSharpComment::from_str_option(method_def.doc.as_deref()),
            native_method_name: CSharpMethodName::native_for_owner(record_class_name, &name),
            name,
            ffi_name: (&call.symbol).into(),
            async_call: None,
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers,
            owner_is_blittable: owner_is_blittable_for_call,
        })
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
            summary_doc: CSharpComment::from_str_option(field.doc.as_deref()),
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
