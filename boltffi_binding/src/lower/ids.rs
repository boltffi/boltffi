use std::collections::HashMap;

use boltffi_ast::{
    ClassId as SourceClassId, ConstantId as SourceConstantId, ConstantOwner as SourceConstantOwner,
    CustomTypeId as SourceCustomTypeId, DeclarationId as SourceDeclarationId,
    EnumId as SourceEnumId, FunctionId as SourceFunctionId, RecordId as SourceRecordId,
    SourceContract, StreamId as SourceStreamId, TraitId as SourceTraitId,
};

use crate::{
    CallbackId, ClassId, ConstantId, ConstantOwner, CustomTypeId, DeclarationId, EnumId,
    FunctionId, RecordId, StreamId,
};

use super::{LowerError, error::DeclarationFamily};

/// Source declaration ids mapped to the typed binding ids the IR carries.
///
/// Built once before the pass walks any declaration. Source contracts
/// with two declarations sharing one id in the same family fail
/// construction, so a successful build proves every later lookup that
/// hits a known id will resolve.
pub struct DeclarationIds {
    declarations: HashMap<SourceDeclarationId, DeclarationId>,
}

impl DeclarationIds {
    pub fn from_source(source: &SourceContract) -> Result<Self, LowerError> {
        DeclarationIdsBuilder::new()
            .insert_family(
                source.records.iter(),
                DeclarationFamily::Records,
                |record| SourceDeclarationId::Record(record.id.clone()),
                |index| DeclarationId::Record(RecordId::from_raw(index)),
            )?
            .insert_family(
                source.enums.iter(),
                DeclarationFamily::Enums,
                |enumeration| SourceDeclarationId::Enum(enumeration.id.clone()),
                |index| DeclarationId::Enum(EnumId::from_raw(index)),
            )?
            .insert_family(
                source.classes.iter(),
                DeclarationFamily::Classes,
                |class| SourceDeclarationId::Class(class.id.clone()),
                |index| DeclarationId::Class(ClassId::from_raw(index)),
            )?
            .insert_family(
                source.traits.iter(),
                DeclarationFamily::Traits,
                |source_trait| SourceDeclarationId::Trait(source_trait.id.clone()),
                |index| DeclarationId::Callback(CallbackId::from_raw(index)),
            )?
            .insert_family(
                source.customs.iter(),
                DeclarationFamily::CustomTypes,
                |custom| SourceDeclarationId::CustomType(custom.id.clone()),
                |index| DeclarationId::CustomType(CustomTypeId::from_raw(index)),
            )?
            .insert_family(
                source.constants.iter(),
                DeclarationFamily::Constants,
                |constant| SourceDeclarationId::Constant(constant.id.clone()),
                |index| DeclarationId::Constant(ConstantId::from_raw(index)),
            )?
            .insert_family(
                source.streams.iter(),
                DeclarationFamily::Streams,
                |stream| SourceDeclarationId::Stream(stream.id.clone()),
                |index| DeclarationId::Stream(StreamId::from_raw(index)),
            )?
            .insert_family(
                source.functions.iter(),
                DeclarationFamily::Functions,
                |function| SourceDeclarationId::Function(function.id.clone()),
                |index| DeclarationId::Function(FunctionId::from_raw(index)),
            )?
            .finish()
    }

    pub fn record(&self, id: &SourceRecordId) -> Result<RecordId, LowerError> {
        self.lookup(
            SourceDeclarationId::Record(id.clone()),
            || LowerError::unknown_record(id),
            |declaration| match declaration {
                DeclarationId::Record(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn enumeration(&self, id: &SourceEnumId) -> Result<EnumId, LowerError> {
        self.lookup(
            SourceDeclarationId::Enum(id.clone()),
            || LowerError::unknown_enum(id),
            |declaration| match declaration {
                DeclarationId::Enum(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn class(&self, id: &SourceClassId) -> Result<ClassId, LowerError> {
        self.lookup(
            SourceDeclarationId::Class(id.clone()),
            || LowerError::unknown_class(id),
            |declaration| match declaration {
                DeclarationId::Class(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn callback(&self, id: &SourceTraitId) -> Result<CallbackId, LowerError> {
        self.lookup(
            SourceDeclarationId::Trait(id.clone()),
            || LowerError::unknown_callback(id),
            |declaration| match declaration {
                DeclarationId::Callback(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn custom(&self, id: &SourceCustomTypeId) -> Result<CustomTypeId, LowerError> {
        self.lookup(
            SourceDeclarationId::CustomType(id.clone()),
            || LowerError::unknown_custom(id),
            |declaration| match declaration {
                DeclarationId::CustomType(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn constant(&self, id: &SourceConstantId) -> Result<ConstantId, LowerError> {
        self.lookup(
            SourceDeclarationId::Constant(id.clone()),
            || LowerError::unknown_constant(id),
            |declaration| match declaration {
                DeclarationId::Constant(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn constant_owner(&self, owner: &SourceConstantOwner) -> Result<ConstantOwner, LowerError> {
        match owner {
            SourceConstantOwner::Record(id) => self.record(id).map(ConstantOwner::Record),
            SourceConstantOwner::Enum(id) => self.enumeration(id).map(ConstantOwner::Enum),
            SourceConstantOwner::Class(id) => self.class(id).map(ConstantOwner::Class),
        }
    }

    pub fn stream(&self, id: &SourceStreamId) -> Result<StreamId, LowerError> {
        self.lookup(
            SourceDeclarationId::Stream(id.clone()),
            || LowerError::unknown_stream(id),
            |declaration| match declaration {
                DeclarationId::Stream(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn function(&self, id: &SourceFunctionId) -> Result<FunctionId, LowerError> {
        self.lookup(
            SourceDeclarationId::Function(id.clone()),
            || LowerError::unknown_function(id),
            |declaration| match declaration {
                DeclarationId::Function(id) => Some(id),
                _ => None,
            },
        )
    }

    pub fn declaration_map(&self) -> DeclarationMap {
        DeclarationMap {
            declarations: self.declarations.clone(),
        }
    }

    fn lookup<T>(
        &self,
        source: SourceDeclarationId,
        unknown: impl FnOnce() -> LowerError,
        extract: impl FnOnce(DeclarationId) -> Option<T>,
    ) -> Result<T, LowerError> {
        self.declarations
            .get(&source)
            .copied()
            .and_then(extract)
            .ok_or_else(unknown)
    }
}

/// Source-to-binding declaration id map produced by lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationMap {
    declarations: HashMap<SourceDeclarationId, DeclarationId>,
}

impl DeclarationMap {
    /// Returns the binding declaration id for a source declaration id.
    pub fn get(&self, source: &SourceDeclarationId) -> Option<DeclarationId> {
        self.declarations.get(source).copied()
    }
}

struct DeclarationIdsBuilder {
    declarations: HashMap<SourceDeclarationId, DeclarationId>,
}

impl DeclarationIdsBuilder {
    fn new() -> Self {
        Self {
            declarations: HashMap::new(),
        }
    }

    fn insert_family<'item, Item>(
        self,
        items: impl Iterator<Item = &'item Item>,
        family: DeclarationFamily,
        source_declaration: impl Fn(&Item) -> SourceDeclarationId,
        binding_declaration: impl Fn(u32) -> DeclarationId,
    ) -> Result<Self, LowerError>
    where
        Item: 'item,
    {
        items
            .enumerate()
            .try_fold(self, |mut builder, (index, item)| {
                let source = source_declaration(item);
                let id = source_id_string(&source);
                match builder
                    .declarations
                    .insert(source, binding_declaration(index as u32))
                {
                    Some(_) => Err(LowerError::duplicate_source_id(family, id)),
                    None => Ok(builder),
                }
            })
    }

    fn finish(self) -> Result<DeclarationIds, LowerError> {
        Ok(DeclarationIds {
            declarations: self.declarations,
        })
    }
}

fn source_id_string(source: &SourceDeclarationId) -> String {
    match source {
        SourceDeclarationId::Record(id) => id.as_str().to_owned(),
        SourceDeclarationId::Enum(id) => id.as_str().to_owned(),
        SourceDeclarationId::Class(id) => id.as_str().to_owned(),
        SourceDeclarationId::Function(id) => id.as_str().to_owned(),
        SourceDeclarationId::Trait(id) => id.as_str().to_owned(),
        SourceDeclarationId::Stream(id) => id.as_str().to_owned(),
        SourceDeclarationId::Constant(id) => id.as_str().to_owned(),
        SourceDeclarationId::CustomType(id) => id.as_str().to_owned(),
        _ => format!("{source:?}"),
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, PackageInfo as SourcePackage, RecordDef, SourceContract,
    };

    use super::super::{DeclarationFamily, LowerErrorKind};
    use super::DeclarationIds;

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn record(id: &str, record_name: &str) -> RecordDef {
        RecordDef::new(id.into(), name(record_name))
    }

    #[test]
    fn rejects_duplicate_record_source_ids() {
        let mut contract = package();
        contract.records.push(record("demo::Point", "point"));
        contract.records.push(record("demo::Point", "point_copy"));

        let error = match DeclarationIds::from_source(&contract) {
            Ok(_) => panic!("duplicate id should fail"),
            Err(error) => error,
        };

        match error.kind() {
            LowerErrorKind::DuplicateSourceId { family, id } => {
                assert_eq!(*family, DeclarationFamily::Records);
                assert_eq!(id, "demo::Point");
            }
            other => panic!("expected duplicate record id error, got {other:?}"),
        }
    }
}
