use std::collections::{HashMap, HashSet};

use boltffi_ast::{
    ClassDef as SourceClass, ClassId as SourceClassId, CustomTypeDef as SourceCustom,
    CustomTypeId as SourceCustomId, EnumDef as SourceEnum, EnumId as SourceEnumId,
    FunctionDef as SourceFunction, RecordDef as SourceRecord, RecordId as SourceRecordId,
    SourceContract, TraitDef as SourceTrait, TraitId as SourceTraitId,
};

/// Borrowed view over a [`SourceContract`] with lookup tables for the
/// declarations the lowering pass dereferences while walking type
/// expressions.
///
/// The pass needs to read the source record or enum behind a
/// `TypeExpr::Record(id)` or `TypeExpr::Enum(id)` to decide whether a
/// nested reference codes as direct memory or encoded bytes. Storing
/// references rather than copying the source keeps construction cheap
/// and ties every lookup to the lifetime of the input.
pub struct Index<'src> {
    source: &'src SourceContract,
    records: HashMap<&'src str, &'src SourceRecord>,
    enums: HashMap<&'src str, &'src SourceEnum>,
    classes: HashMap<&'src str, &'src SourceClass>,
    traits: HashMap<&'src str, &'src SourceTrait>,
    customs: HashMap<&'src str, &'src SourceCustom>,
    lowered: Option<&'src HashSet<String>>,
    shallow: Option<&'src HashSet<String>>,
}

impl<'src> Index<'src> {
    pub fn new(source: &'src SourceContract) -> Self {
        Self {
            source,
            records: source
                .records
                .iter()
                .map(|record| (record.id.as_str(), record))
                .collect(),
            enums: source
                .enums
                .iter()
                .map(|enumeration| (enumeration.id.as_str(), enumeration))
                .collect(),
            classes: source
                .classes
                .iter()
                .map(|class| (class.id.as_str(), class))
                .collect(),
            traits: source
                .traits
                .iter()
                .map(|r#trait| (r#trait.id.as_str(), r#trait))
                .collect(),
            customs: source
                .customs
                .iter()
                .map(|custom| (custom.id.as_str(), custom))
                .collect(),
            lowered: None,
            shallow: None,
        }
    }

    /// Restricts lowering to `lowered`; every other declaration stays lookup-only. The
    /// `shallow` ones lower without fields or variants, since only their methods render.
    pub fn lowering(
        mut self,
        lowered: &'src HashSet<String>,
        shallow: &'src HashSet<String>,
    ) -> Self {
        self.lowered = Some(lowered);
        self.shallow = Some(shallow);
        self
    }

    pub fn lowers(&self, id: &str) -> bool {
        self.lowered.is_none_or(|lowered| lowered.contains(id))
    }

    pub fn is_shallow(&self, id: &str) -> bool {
        self.shallow.is_some_and(|shallow| shallow.contains(id))
    }

    pub fn source(&self) -> &'src SourceContract {
        self.source
    }

    pub fn records(&self) -> &'src [SourceRecord] {
        &self.source.records
    }

    pub fn enums(&self) -> &'src [SourceEnum] {
        &self.source.enums
    }

    pub fn classes(&self) -> &'src [SourceClass] {
        &self.source.classes
    }

    pub fn traits(&self) -> &'src [SourceTrait] {
        &self.source.traits
    }

    pub fn functions(&self) -> &'src [SourceFunction] {
        &self.source.functions
    }

    pub fn constants(&self) -> &'src [boltffi_ast::ConstantDef] {
        &self.source.constants
    }

    pub fn customs(&self) -> &'src [boltffi_ast::CustomTypeDef] {
        &self.source.customs
    }

    pub fn streams(&self) -> &'src [boltffi_ast::StreamDef] {
        &self.source.streams
    }

    pub fn record(&self, id: &SourceRecordId) -> Option<&'src SourceRecord> {
        self.records.get(id.as_str()).copied()
    }

    pub fn enumeration(&self, id: &SourceEnumId) -> Option<&'src SourceEnum> {
        self.enums.get(id.as_str()).copied()
    }

    pub fn class(&self, id: &SourceClassId) -> Option<&'src SourceClass> {
        self.classes.get(id.as_str()).copied()
    }

    pub fn r#trait(&self, id: &SourceTraitId) -> Option<&'src SourceTrait> {
        self.traits.get(id.as_str()).copied()
    }

    pub fn custom(&self, id: &SourceCustomId) -> Option<&'src SourceCustom> {
        self.customs.get(id.as_str()).copied()
    }
}
