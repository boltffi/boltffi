use boltffi_ffi_rules::naming;

use crate::ir::abi::{AbiCall, CallId};
use crate::ir::definitions::{ClassDef, ConstructorDef};

use super::super::ast::{CSharpClassName, CSharpMethodName};
use super::super::plan::{
    CSharpClassPlan, CSharpConstructorKind, CSharpConstructorPlan, CSharpParamPlan,
};
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

        CSharpClassPlan {
            class_name,
            ffi_free,
            native_free_method_name,
            constructors,
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
            ConstructorDef::Default { .. } => CSharpConstructorKind::Primary,
            ConstructorDef::NamedFactory { name, .. } | ConstructorDef::NamedInit { name, .. } => {
                CSharpConstructorKind::StaticFactory {
                    name: CSharpMethodName::from_source(name.as_str()),
                }
            }
        };

        let surface_name = match &kind {
            CSharpConstructorKind::Primary => CSharpMethodName::new("New"),
            CSharpConstructorKind::StaticFactory { name } => name.clone(),
        };
        let native_method_name = CSharpMethodName::native_for_owner(class_name, &surface_name);
        let helper_method_name = match &kind {
            CSharpConstructorKind::Primary => {
                CSharpMethodName::new(format!("{class_name}NewHandle"))
            }
            CSharpConstructorKind::StaticFactory { .. } => CSharpMethodName::new(""),
        };

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
            kind,
            native_method_name,
            helper_method_name,
            ffi_name: (&call.symbol).into(),
            params,
            wire_writers,
        })
    }
}
