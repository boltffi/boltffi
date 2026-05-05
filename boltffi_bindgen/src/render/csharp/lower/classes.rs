use boltffi_ffi_rules::naming;

use crate::ir::abi::{AbiCall, CallId, CallMode};
use crate::ir::definitions::{ClassDef, ConstructorDef, MethodDef, Receiver};

use super::super::ast::{CSharpClassName, CSharpComment, CSharpMethodName};
use super::super::plan::{
    CSharpClassPlan, CSharpConstructorKind, CSharpConstructorPlan, CSharpMethodPlan,
    CSharpParamPlan, CSharpReceiver,
};
use super::functions::csharp_async_call_plan;
use super::lowerer::CSharpLowerer;
use super::{encode, size};

impl<'a> CSharpLowerer<'a> {
    /// Lowers a Rust class definition to a [`CSharpClassPlan`].
    ///
    /// The plan carries the names needed to emit the `IDisposable`
    /// wrapper plus any public constructors. Method and stream
    /// lowering are tracked as follow-up work.
    pub(super) fn lower_class(&self, class: &ClassDef) -> CSharpClassPlan {
        let class_name = CSharpClassName::from_source(class.id.as_str());
        let ffi_free = naming::class_ffi_free(class.id.as_str()).into();
        let native_free_method_name =
            CSharpMethodName::native_for_owner(&class_name, &CSharpMethodName::new("Free"));
        let constructors = self.lower_class_constructors(class, &class_name);
        let methods = self.lower_class_methods(class, &class_name);

        CSharpClassPlan {
            summary_doc: CSharpComment::from_str_option(class.doc.as_deref()),
            class_name,
            ffi_free,
            native_free_method_name,
            constructors,
            methods,
        }
    }

    /// Walks `class.constructors` and produces the corresponding
    /// [`CSharpConstructorPlan`]s. Fallible (`Result<Self, _>`) and
    /// optional (`Option<Self>`) constructors are dropped silently;
    /// the C# backend doesn't model failure paths yet, matching how
    /// enum constructor lowering handles them.
    fn lower_class_constructors(
        &self,
        class: &ClassDef,
        class_name: &CSharpClassName,
    ) -> Vec<CSharpConstructorPlan> {
        class
            .constructors
            .iter()
            .enumerate()
            .filter(|(_, ctor)| !ctor.is_fallible() && !ctor.is_optional())
            .filter_map(|(index, ctor)| {
                let call = self.abi.calls.iter().find(|c| {
                    c.id == CallId::Constructor {
                        class_id: class.id.clone(),
                        index,
                    }
                })?;
                self.lower_class_constructor(ctor, call, class_name)
            })
            .collect()
    }

    /// Lowers one constructor. Default constructors become C# primary
    /// constructors; named factories and named-init constructors
    /// become static factories. Returns `None` if any param fails to
    /// lower (e.g., references an unsupported type).
    fn lower_class_constructor(
        &self,
        ctor: &ConstructorDef,
        call: &AbiCall,
        class_name: &CSharpClassName,
    ) -> Option<CSharpConstructorPlan> {
        let kind = match ctor {
            ConstructorDef::Default { .. } => CSharpConstructorKind::Primary {
                helper_method_name: CSharpMethodName::new(format!("{class_name}NewHandle")),
            },
            ConstructorDef::NamedFactory { name, .. } | ConstructorDef::NamedInit { name, .. } => {
                CSharpConstructorKind::StaticFactory {
                    name: CSharpMethodName::from_source(name.as_str()),
                }
            }
        };

        let surface_name = match &kind {
            CSharpConstructorKind::Primary { .. } => CSharpMethodName::new("New"),
            CSharpConstructorKind::StaticFactory { name } => name.clone(),
        };
        let native_method_name = CSharpMethodName::native_for_owner(class_name, &surface_name);

        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let wire_writers: Vec<_> = call
            .params
            .iter()
            .filter_map(|p| self.wire_writer_for_param(p, &mut size_locals, &mut encode_locals))
            .collect();

        let params: Vec<CSharpParamPlan> = ctor
            .params()
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<_>>()?;

        Some(CSharpConstructorPlan {
            summary_doc: CSharpComment::from_str_option(ctor.doc()),
            kind,
            native_method_name,
            ffi_name: (&call.symbol).into(),
            params,
            wire_writers,
        })
    }

    /// Walks `class.methods` and produces the corresponding
    /// [`CSharpMethodPlan`]s. Skips `OwnedSelf` receivers (consume the
    /// wrapper, complex lifecycle).
    fn lower_class_methods(
        &self,
        class: &ClassDef,
        class_name: &CSharpClassName,
    ) -> Vec<CSharpMethodPlan> {
        class
            .methods
            .iter()
            .filter(|m| !matches!(m.receiver, Receiver::OwnedSelf))
            .filter_map(|method_def| {
                let call = self.abi.calls.iter().find(|c| {
                    c.id == CallId::Method {
                        class_id: class.id.clone(),
                        method_id: method_def.id.clone(),
                    }
                })?;
                self.lower_class_method(method_def, call, class_name)
            })
            .collect()
    }

    /// Lowers a single class method. `Static` receivers stay static;
    /// `RefSelf` and `RefMutSelf` both lift to
    /// [`CSharpReceiver::ClassInstance`]; `OwnedSelf` is filtered out
    /// upstream. Returns `None` if any param fails to lower.
    fn lower_class_method(
        &self,
        method_def: &MethodDef,
        call: &AbiCall,
        class_name: &CSharpClassName,
    ) -> Option<CSharpMethodPlan> {
        let receiver = match method_def.receiver {
            Receiver::Static => CSharpReceiver::Static,
            Receiver::RefSelf | Receiver::RefMutSelf => CSharpReceiver::ClassInstance,
            Receiver::OwnedSelf => return None,
        };

        let return_type = self.lower_return(&method_def.returns)?;
        let complete_decode_ops = match &call.mode {
            CallMode::Sync => call.returns.decode_ops.as_ref(),
            CallMode::Async(async_call) => async_call.result.decode_ops.as_ref(),
        };
        let return_kind =
            self.return_kind(&method_def.returns, &return_type, complete_decode_ops, None);

        // Instance methods carry a synthetic `self` at the head of the
        // ABI param list. Skip it when building wire writers and when
        // mapping back to the explicit Rust params (which never include
        // `self`). Static methods don't have this prefix.
        let explicit_abi_params = if matches!(receiver, CSharpReceiver::Static) {
            &call.params[..]
        } else {
            &call.params[1..]
        };
        let mut size_locals = size::SizeLocalCounters::default();
        let mut encode_locals = encode::EncodeLocalCounters::default();
        let wire_writers: Vec<_> = explicit_abi_params
            .iter()
            .filter_map(|p| self.wire_writer_for_param(p, &mut size_locals, &mut encode_locals))
            .collect();

        let params: Vec<CSharpParamPlan> = method_def
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<_>>()?;

        let name: CSharpMethodName = (&method_def.id).into();
        let native_method_name = CSharpMethodName::native_for_owner(class_name, &name);
        let async_call = match &call.mode {
            CallMode::Sync => None,
            CallMode::Async(async_call) => {
                Some(csharp_async_call_plan(async_call, &native_method_name))
            }
        };
        Some(CSharpMethodPlan {
            summary_doc: CSharpComment::from_str_option(method_def.doc.as_deref()),
            native_method_name,
            name,
            ffi_name: (&call.symbol).into(),
            async_call,
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers,
            owner_is_blittable: false,
        })
    }
}
