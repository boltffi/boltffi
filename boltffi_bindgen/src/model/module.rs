use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use boltffi_ffi_rules::callable::ExecutionKind;

use super::callback_trait::CallbackTrait;
use super::class::Class;
use super::custom_type::CustomType;
use super::enum_layout::DataEnumLayout;
use super::enumeration::Enumeration;
use super::function::Function;
use super::record::Record;
use super::types::{BuiltinId, ClosureSignature, ReturnType, Type};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub classes: Vec<Class>,
    pub records: Vec<Record>,
    pub enums: Vec<Enumeration>,
    pub functions: Vec<Function>,
    pub callback_traits: Vec<CallbackTrait>,
    #[serde(default)]
    pub custom_types: Vec<CustomType>,
    #[serde(default)]
    pub used_builtins: HashSet<BuiltinId>,
    #[serde(default)]
    pub closures: HashMap<String, ClosureSignature>,
}

impl Module {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            classes: Vec::new(),
            records: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            callback_traits: Vec::new(),
            custom_types: Vec::new(),
            used_builtins: HashSet::new(),
            closures: HashMap::new(),
        }
    }

    pub fn with_class(mut self, class: Class) -> Self {
        self.classes.push(class);
        self
    }

    pub fn with_record(mut self, record: Record) -> Self {
        self.records.push(record);
        self
    }

    pub fn with_enum(mut self, enumeration: Enumeration) -> Self {
        self.enums.push(enumeration);
        self
    }

    pub fn with_function(mut self, function: Function) -> Self {
        self.functions.push(function);
        self
    }

    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|class| class.name == name)
    }

    pub fn find_record(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|record| record.name == name)
    }

    pub fn find_enum(&self, name: &str) -> Option<&Enumeration> {
        self.enums
            .iter()
            .find(|enumeration| enumeration.name == name)
    }

    pub fn with_callback_trait(mut self, callback_trait: CallbackTrait) -> Self {
        self.callback_traits.push(callback_trait);
        self
    }

    pub fn with_custom_type(mut self, custom_type: CustomType) -> Self {
        self.custom_types.push(custom_type);
        self
    }

    pub fn merge_exports(&mut self, other: Self) -> Result<(), String> {
        other
            .classes
            .iter()
            .map(|class| class.name.as_str())
            .chain(other.records.iter().map(|record| record.name.as_str()))
            .chain(
                other
                    .enums
                    .iter()
                    .map(|enumeration| enumeration.name.as_str()),
            )
            .chain(
                other
                    .callback_traits
                    .iter()
                    .map(|callback_trait| callback_trait.name.as_str()),
            )
            .chain(
                other
                    .custom_types
                    .iter()
                    .map(|custom_type| custom_type.name.as_str()),
            )
            .try_for_each(|name| self.require_available_type_name(name))?;

        self.classes.extend(other.classes);
        self.records.extend(other.records);
        self.enums.extend(other.enums);
        self.functions.extend(other.functions);
        self.callback_traits.extend(other.callback_traits);
        self.custom_types.extend(other.custom_types);
        self.used_builtins.extend(other.used_builtins);
        self.closures.extend(other.closures);
        self.collect_derived_types();
        Ok(())
    }

    pub fn find_callback_trait(&self, name: &str) -> Option<&CallbackTrait> {
        self.callback_traits
            .iter()
            .find(|callback_trait| callback_trait.name == name)
    }

    pub fn find_custom_type(&self, name: &str) -> Option<&CustomType> {
        self.custom_types
            .iter()
            .find(|custom_type| custom_type.name == name)
    }

    fn require_available_type_name(&self, name: &str) -> Result<(), String> {
        let exists = self.classes.iter().any(|class| class.name == name)
            || self.records.iter().any(|record| record.name == name)
            || self
                .enums
                .iter()
                .any(|enumeration| enumeration.name == name)
            || self
                .callback_traits
                .iter()
                .any(|callback_trait| callback_trait.name == name)
            || self
                .custom_types
                .iter()
                .any(|custom_type| custom_type.name == name);

        (!exists)
            .then_some(())
            .ok_or_else(|| format!("duplicate exported type name `{name}` across legacy crates"))
    }

    pub fn has_exports(&self) -> bool {
        !self.classes.is_empty()
            || !self.functions.is_empty()
            || !self.enums.is_empty()
            || !self.callback_traits.is_empty()
            || !self.custom_types.is_empty()
    }

    pub fn has_async(&self) -> bool {
        self.functions
            .iter()
            .any(|function| function.execution_kind == ExecutionKind::Async)
            || self.classes.iter().any(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.execution_kind == ExecutionKind::Async)
            })
            || self.records.iter().any(|record| {
                record
                    .methods
                    .iter()
                    .any(|method| method.execution_kind == ExecutionKind::Async)
            })
    }

    pub fn has_streams(&self) -> bool {
        self.classes.iter().any(|c| !c.streams.is_empty())
    }

    pub fn struct_size(&self, name: &str) -> usize {
        self.records
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.struct_size().as_usize())
            .or_else(|| {
                self.enums
                    .iter()
                    .find(|e| e.name == name && e.is_data_enum())
                    .and_then(DataEnumLayout::from_enum)
                    .map(|l| l.struct_size().as_usize())
            })
            .unwrap_or(0)
    }

    pub fn is_data_enum(&self, name: &str) -> bool {
        self.enums
            .iter()
            .find(|e| e.name == name)
            .is_some_and(|e| e.is_data_enum())
    }

    pub fn collect_derived_types(&mut self) {
        let mut collector = DerivedTypeCollector::new();

        self.records
            .iter()
            .flat_map(|r| r.fields.iter().map(|f| &f.field_type))
            .for_each(|ty| collector.visit(ty));

        self.records.iter().for_each(|r| {
            r.constructors.iter().for_each(|ctor| {
                ctor.inputs
                    .iter()
                    .map(|p| &p.param_type)
                    .for_each(|ty| collector.visit(ty));
            });
            r.methods.iter().for_each(|m| {
                m.inputs
                    .iter()
                    .map(|p| &p.param_type)
                    .for_each(|ty| collector.visit(ty));
                collector.visit_return_type(&m.returns);
            });
        });

        self.enums
            .iter()
            .flat_map(|e| e.variants.iter())
            .flat_map(|v| v.fields.iter().map(|f| &f.field_type))
            .for_each(|ty| collector.visit(ty));

        self.functions.iter().for_each(|f| {
            f.inputs
                .iter()
                .map(|p| &p.param_type)
                .for_each(|ty| collector.visit(ty));
            collector.visit_return_type(&f.returns);
        });

        self.classes.iter().for_each(|c| {
            c.constructors.iter().for_each(|ctor| {
                ctor.inputs
                    .iter()
                    .map(|p| &p.param_type)
                    .for_each(|ty| collector.visit(ty));
            });
            c.methods.iter().for_each(|m| {
                m.inputs
                    .iter()
                    .map(|p| &p.param_type)
                    .for_each(|ty| collector.visit(ty));
                collector.visit_return_type(&m.returns);
            });
            c.streams.iter().for_each(|s| collector.visit(&s.item_type));
        });

        self.callback_traits.iter().for_each(|cb| {
            cb.methods.iter().for_each(|m| {
                m.inputs
                    .iter()
                    .map(|p| &p.param_type)
                    .for_each(|ty| collector.visit(ty));
                collector.visit_return_type(&m.returns);
            });
        });

        self.custom_types
            .iter()
            .map(|ct| &ct.repr)
            .for_each(|ty| collector.visit(ty));

        self.used_builtins = collector.builtins;
        self.closures = collector.closures;
    }
}

struct DerivedTypeCollector {
    builtins: HashSet<BuiltinId>,
    closures: HashMap<String, ClosureSignature>,
}

impl DerivedTypeCollector {
    fn new() -> Self {
        Self {
            builtins: HashSet::new(),
            closures: HashMap::new(),
        }
    }

    fn visit(&mut self, ty: &Type) {
        match ty {
            Type::Builtin(id) => {
                self.builtins.insert(*id);
            }
            Type::Closure(sig) => {
                let sig_id = format!("__Closure_{}", sig.signature_id());
                self.closures.entry(sig_id).or_insert_with(|| sig.clone());
                sig.params.iter().for_each(|p| self.visit(p));
                self.visit(&sig.returns);
            }
            Type::Vec(inner) | Type::Option(inner) | Type::Slice(inner) | Type::MutSlice(inner) => {
                self.visit(inner);
            }
            Type::Result { ok, err } => {
                self.visit(ok);
                self.visit(err);
            }
            Type::Custom { repr, .. } => {
                self.visit(repr);
            }
            Type::Primitive(_)
            | Type::String
            | Type::Str
            | Type::Record(_)
            | Type::Enum(_)
            | Type::Object(_)
            | Type::BoxedTrait(_)
            | Type::Void => {}
        }
    }

    fn visit_return_type(&mut self, ret: &ReturnType) {
        match ret {
            ReturnType::Void => {}
            ReturnType::Value(ty) => self.visit(ty),
            ReturnType::Fallible { ok, err } => {
                self.visit(ok);
                self.visit(err);
            }
        }
    }
}
