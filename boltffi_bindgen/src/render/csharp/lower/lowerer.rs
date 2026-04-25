use std::collections::HashSet;

use boltffi_ffi_rules::naming;

use crate::ir::ids::{EnumId, RecordId};
use crate::ir::{AbiContract, FfiContract};

use super::super::CSharpOptions;
use super::super::ast::{CSharpClassName, CSharpNamespace};
use super::super::plan::{
    CFunctionName, CSharpEnumPlan, CSharpFunctionPlan, CSharpModulePlan, CSharpRecordPlan,
};

/// Produces a [`CSharpModulePlan`] from the IR contracts.
pub struct CSharpLowerer<'a> {
    pub(super) ffi: &'a FfiContract,
    pub(super) abi: &'a AbiContract,
    pub(super) options: &'a CSharpOptions,
    /// The C# namespace every generated file lands in. Used by
    /// `qualify_if_shadowed` to fully-qualify type references when shadowed.
    pub(super) namespace: CSharpNamespace,
    /// Records that are fully supported: every field resolves to a type the
    /// C# backend can render. Computed jointly with `supported_enums`
    /// up-front since admission can depend on other records and on data enums.
    pub(super) supported_records: HashSet<RecordId>,
    /// Enums (C-style and data) that are fully supported. C-style admit when
    /// their `repr` is a legal C# enum backing type. Data enums admit when
    /// every variant's payload fields resolve to supported types.
    pub(super) supported_enums: HashSet<EnumId>,
}

impl<'a> CSharpLowerer<'a> {
    pub fn new(ffi: &'a FfiContract, abi: &'a AbiContract, options: &'a CSharpOptions) -> Self {
        let (supported_records, supported_enums) = Self::compute_supported_sets(ffi);
        let namespace = CSharpNamespace::from_source(&ffi.package.name);
        Self {
            ffi,
            abi,
            options,
            namespace,
            supported_records,
            supported_enums,
        }
    }

    /// Walks the contracts and produces a C# module plan.
    pub fn lower(&self) -> CSharpModulePlan {
        let lib_name = self
            .options
            .library_name
            .clone()
            .unwrap_or_else(|| naming::library_name(&self.ffi.package.name));

        let class_name = CSharpClassName::from_source(&self.ffi.package.name);
        let namespace = self.namespace.clone();
        let free_buf_ffi_name = CFunctionName::new(format!("{}_free_buf", naming::ffi_prefix()));

        let records: Vec<CSharpRecordPlan> = self
            .ffi
            .catalog
            .all_records()
            .filter(|r| self.supported_records.contains(&r.id))
            .map(|r| self.lower_record(r))
            .collect();

        let enums: Vec<CSharpEnumPlan> = self
            .ffi
            .catalog
            .all_enums()
            .filter_map(|e| self.lower_enum(e))
            .collect();

        let functions: Vec<CSharpFunctionPlan> = self
            .ffi
            .functions
            .iter()
            .filter_map(|f| self.lower_function(f))
            .collect();

        CSharpModulePlan {
            namespace,
            class_name,
            lib_name,
            free_buf_ffi_name,
            records,
            enums,
            functions,
        }
    }
}
