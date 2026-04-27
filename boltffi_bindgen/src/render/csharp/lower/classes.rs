use boltffi_ffi_rules::naming;

use crate::ir::definitions::ClassDef;

use super::super::ast::{CSharpClassName, CSharpMethodName};
use super::super::plan::CSharpClassPlan;
use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Lowers a Rust class definition to a [`CSharpClassPlan`].
    ///
    /// At this stage the plan only carries the names needed to emit
    /// the `IDisposable` wrapper around the native handle. Constructor,
    /// method, and stream lowering are tracked as follow-up work.
    pub(super) fn lower_class(&self, class: &ClassDef) -> CSharpClassPlan {
        let class_name = CSharpClassName::from_source(class.id.as_str());
        let ffi_free = naming::class_ffi_free(class.id.as_str()).into();
        let native_free_method_name =
            CSharpMethodName::native_for_owner(&class_name, &CSharpMethodName::new("Free"));

        CSharpClassPlan {
            class_name,
            ffi_free,
            native_free_method_name,
        }
    }
}
