use crate::ir::abi::{AbiParam, ParamRole};
use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::definitions::FunctionDef;
use crate::ir::ops::WriteOp;

use super::super::ast::{
    CSharpArgumentList, CSharpExpression, CSharpIdentity, CSharpLocalName, CSharpMethodName,
    CSharpParamName, CSharpStatement,
};
use super::super::plan::CSharpWireWriterPlan;
use super::lowerer::CSharpLowerer;
use super::{encode, size, value};

impl<'a> CSharpLowerer<'a> {
    /// Builds one [`CSharpWireWriterPlan`] per param that needs a
    /// `WireWriter` setup block (non-blittable records, data enums,
    /// encoded vecs, options), in param order. Returns `None` if the
    /// function's ABI call can't be found (shouldn't happen for validated
    /// contracts).
    pub(super) fn wire_writers_for_params(
        &self,
        function: &FunctionDef,
    ) -> Option<Vec<CSharpWireWriterPlan>> {
        let call = self.abi_call_for_function(function)?;
        // One size/encode context per function body so all `using var
        // _wire_*` declarations and `sizeOpt{n}` / `opt{n}` pattern
        // bindings get unique names within the same method scope.
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        Some(
            call.params
                .iter()
                .filter_map(|abi_param| {
                    self.wire_writer_for_param(abi_param, &mut size_locals, &mut encode_locals)
                })
                .collect(),
        )
    }

    /// Builds the [`CSharpWireWriterPlan`] for one param. Returns `None`
    /// when the param doesn't need wire-buffer setup (see
    /// [`Self::param_needs_wire_buffer`]).
    pub(super) fn wire_writer_for_param(
        &self,
        param: &AbiParam,
        size_locals: &mut size::SizeLocalCounters,
        encode_locals: &mut encode::EncodeLocalCounters,
    ) -> Option<CSharpWireWriterPlan> {
        let encode_ops = match &param.role {
            ParamRole::Input {
                encode_ops: Some(encode_ops),
                ..
            } => encode_ops.clone(),
            _ => return None,
        };
        if !self.param_needs_wire_buffer(encode_ops.ops.first()?) {
            return None;
        }
        let param_name: CSharpParamName = (&param.name).into();
        let binding_name = CSharpLocalName::for_wire_writer(&param_name);
        let bytes_binding_name = CSharpLocalName::for_bytes(&param_name);
        let writer = CSharpExpression::Identity(CSharpIdentity::Local(binding_name.clone()));
        let encode_stmts =
            encode::lower_encode_expr(&encode_ops, &writer, &value::Renames::new(), encode_locals);
        Some(CSharpWireWriterPlan {
            binding_name,
            bytes_binding_name,
            param_name,
            size_expr: size::lower_size_expr(&encode_ops.size, &value::Renames::new(), size_locals),
            encode_stmts,
        })
    }

    /// Whether a param's encode op requires a `WireWriter` setup block
    /// before the native call.
    ///
    /// Primitives pass as value types, strings go through the UTF-8 byte
    /// path, raw bytes ride as `byte[]` directly. Blittable records and
    /// C-style enums also pass by value. Variable-width payloads
    /// (non-blittable records, data enums, vecs) need a length-prefixed
    /// buffer serialized up front.
    fn param_needs_wire_buffer(&self, op: &WriteOp) -> bool {
        match op {
            WriteOp::Primitive { .. } | WriteOp::String { .. } | WriteOp::Bytes { .. } => false,
            WriteOp::Record { id, .. } => !self.is_blittable_record(id),
            WriteOp::Enum {
                layout: EnumLayout::Data { .. },
                ..
            } => true,
            WriteOp::Enum { .. } => false,
            WriteOp::Vec {
                layout: VecLayout::Blittable { .. },
                ..
            } => false,
            WriteOp::Vec {
                layout: VecLayout::Encoded,
                ..
            } => true,
            WriteOp::Option { .. } => true,
            WriteOp::Result { .. } | WriteOp::Builtin { .. } | WriteOp::Custom { .. } => {
                todo!("C# backend has not yet implemented param support for {op:?}")
            }
        }
    }
}

/// Synthesize the [`CSharpWireWriterPlan`] for an `InstanceNative`
/// receiver whose owner is wire-encoded (data enums today, non-
/// blittable records once their instance methods land). Lets the
/// template iterate `wire_writers` uniformly instead of hardcoding
/// the receiver's encode block as a special case. The size and encode
/// expressions root at `this` rather than going through the IR's
/// `Named("self")` reference.
///
/// Exposed at `pub(super)` so the templates' snapshot fixtures (which
/// build hand-rolled [`CSharpMethodPlan`]s) can mirror the lowerer's
/// new contract: an `InstanceNative` plan must include a self-writer
/// in `wire_writers[0]`.
pub(crate) fn self_wire_writer() -> CSharpWireWriterPlan {
    let self_param_name = CSharpParamName::new("self");
    let binding_name = CSharpLocalName::for_wire_writer(&self_param_name);
    let bytes_binding_name = CSharpLocalName::for_bytes(&self_param_name);
    let this_expr = CSharpExpression::Identity(CSharpIdentity::This);
    let size_expr = CSharpExpression::MethodCall {
        receiver: Box::new(this_expr.clone()),
        method: CSharpMethodName::from_source("wire_encoded_size"),
        type_args: vec![],
        args: CSharpArgumentList::default(),
    };
    let encode_stmts = vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
        receiver: Box::new(this_expr),
        method: CSharpMethodName::from_source("wire_encode_to"),
        type_args: vec![],
        args: vec![CSharpExpression::Identity(CSharpIdentity::Local(
            binding_name.clone(),
        ))]
        .into(),
    })];
    CSharpWireWriterPlan {
        binding_name,
        bytes_binding_name,
        param_name: self_param_name,
        size_expr,
        encode_stmts,
    }
}
