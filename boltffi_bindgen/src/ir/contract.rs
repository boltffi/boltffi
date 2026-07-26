use indexmap::IndexMap;

use crate::ir::abi::CallId;
use crate::ir::definitions::{
    CallbackTraitDef, ClassDef, CustomTypeDef, EnumDef, FunctionDef, ParamDef, RecordDef,
};
use crate::ir::ids::{BuiltinId, CallbackId, ClassId, CustomTypeId, EnumId, RecordId};
use crate::ir::types::BuiltinDef;

/// The Rust crate's public API, extracted from parsed module definitions.
///
/// Records, enums, classes, callbacks, free functions. What the crate exports.
/// Nothing here knows about wire encoding or parameter passing. That happens
/// when [`Lowerer`](crate::ir::Lowerer) turns this into an [`AbiContract`](crate::ir::abi::AbiContract).
#[derive(Debug, Clone)]
pub struct FfiContract {
    pub package: PackageInfo,
    pub catalog: TypeCatalog,
    pub functions: Vec<FunctionDef>,
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeCatalog {
    records: IndexMap<RecordId, RecordDef>,
    enums: IndexMap<EnumId, EnumDef>,
    callbacks: IndexMap<CallbackId, CallbackTraitDef>,
    custom_types: IndexMap<CustomTypeId, CustomTypeDef>,
    builtins: IndexMap<BuiltinId, BuiltinDef>,
    classes: IndexMap<ClassId, ClassDef>,
}

impl TypeCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_record(&mut self, def: RecordDef) {
        self.records.insert(def.id.clone(), def);
    }

    pub fn insert_enum(&mut self, def: EnumDef) {
        self.enums.insert(def.id.clone(), def);
    }

    pub fn insert_callback(&mut self, def: CallbackTraitDef) {
        self.callbacks.insert(def.id.clone(), def);
    }

    pub fn insert_custom(&mut self, def: CustomTypeDef) {
        self.custom_types.insert(def.id.clone(), def);
    }

    pub fn insert_builtin(&mut self, def: BuiltinDef) {
        self.builtins.insert(def.id.clone(), def);
    }

    pub fn insert_class(&mut self, def: ClassDef) {
        self.classes.insert(def.id.clone(), def);
    }

    pub fn resolve_record(&self, id: &RecordId) -> Option<&RecordDef> {
        self.records.get(id)
    }

    pub fn resolve_enum(&self, id: &EnumId) -> Option<&EnumDef> {
        self.enums.get(id)
    }

    pub fn resolve_callback(&self, id: &CallbackId) -> Option<&CallbackTraitDef> {
        self.callbacks.get(id)
    }

    pub fn resolve_custom(&self, id: &CustomTypeId) -> Option<&CustomTypeDef> {
        self.custom_types.get(id)
    }

    pub fn resolve_builtin(&self, id: &BuiltinId) -> Option<&BuiltinDef> {
        self.builtins.get(id)
    }

    pub fn resolve_class(&self, id: &ClassId) -> Option<&ClassDef> {
        self.classes.get(id)
    }

    pub fn all_records(&self) -> impl Iterator<Item = &RecordDef> {
        self.records.values()
    }

    pub fn all_enums(&self) -> impl Iterator<Item = &EnumDef> {
        self.enums.values()
    }

    pub fn all_callbacks(&self) -> impl Iterator<Item = &CallbackTraitDef> {
        self.callbacks.values()
    }

    pub fn all_classes(&self) -> impl Iterator<Item = &ClassDef> {
        self.classes.values()
    }

    pub fn all_custom_types(&self) -> impl Iterator<Item = &CustomTypeDef> {
        self.custom_types.values()
    }

    pub fn all_builtins(&self) -> impl Iterator<Item = &BuiltinDef> {
        self.builtins.values()
    }

    pub fn params_for_value_call(&self, call_id: &CallId) -> Vec<&ParamDef> {
        match call_id {
            CallId::RecordConstructor { record_id, index } => {
                self.resolve_record(record_id).unwrap().constructors[*index].params()
            }
            CallId::RecordMethod {
                record_id,
                method_id,
            } => self
                .resolve_record(record_id)
                .unwrap()
                .methods
                .iter()
                .find(|m| m.id == *method_id)
                .unwrap()
                .params
                .iter()
                .collect(),
            CallId::EnumConstructor { enum_id, index } => {
                self.resolve_enum(enum_id).unwrap().constructors[*index].params()
            }
            CallId::EnumMethod { enum_id, method_id } => self
                .resolve_enum(enum_id)
                .unwrap()
                .methods
                .iter()
                .find(|m| m.id == *method_id)
                .unwrap()
                .params
                .iter()
                .collect(),
            _ => vec![],
        }
    }
}
