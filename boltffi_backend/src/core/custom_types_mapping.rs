use std::collections::BTreeMap;

use boltffi_binding::{Bindings, CustomTypeDecl, CustomTypeId, DeclarationRef, Surface};

use crate::core::{Error, Result};

/// Conversion used by a mapped custom type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CustomTypeConversion {
    /// A string representation mapped to a target UUID type.
    UuidString,
    /// A string representation mapped to a target URL type.
    UrlString,
}

/// A target-language type name used for a custom type mapping.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct TargetTypeName {
    value: String,
}

/// Public target type and conversion for one custom type.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CustomTypeMapping {
    target_type: TargetTypeName,
    conversion: CustomTypeConversion,
}

/// Configured custom type mappings before binding ids are known.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CustomTypeMappingSet {
    mappings: BTreeMap<String, CustomTypeMapping>,
}

/// Custom type mappings resolved against a binding contract.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct ResolvedCustomTypeMappings {
    mappings: BTreeMap<CustomTypeId, CustomTypeMapping>,
}

struct CustomTypeMappingTarget<'declaration> {
    declaration: &'declaration CustomTypeDecl,
    target_type_name: String,
    canonical_name: String,
}

impl TargetTypeName {
    /// Creates a target-language type name.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Returns the target-language spelling.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl CustomTypeMapping {
    /// Creates a mapping whose FFI representation is a UUID string.
    pub fn uuid_string(target_type: impl Into<String>) -> Self {
        Self {
            target_type: TargetTypeName::new(target_type),
            conversion: CustomTypeConversion::UuidString,
        }
    }

    /// Creates a mapping whose FFI representation is a URL string.
    pub fn url_string(target_type: impl Into<String>) -> Self {
        Self {
            target_type: TargetTypeName::new(target_type),
            conversion: CustomTypeConversion::UrlString,
        }
    }

    /// Returns the target-language type name.
    pub fn target_type(&self) -> &TargetTypeName {
        &self.target_type
    }

    /// Returns the representation conversion.
    pub fn conversion(&self) -> CustomTypeConversion {
        self.conversion
    }
}

impl CustomTypeMappingSet {
    /// Inserts a mapping keyed by either target type name or canonical custom type path.
    pub fn insert(&mut self, custom_type: impl Into<String>, mapping: CustomTypeMapping) {
        self.mappings.insert(custom_type.into(), mapping);
    }

    /// Resolves configured mappings to custom type ids.
    pub fn resolve<S: Surface>(
        &self,
        bindings: &Bindings<S>,
        target: &'static str,
        target_type_name: impl Fn(&CustomTypeDecl) -> String,
    ) -> Result<ResolvedCustomTypeMappings> {
        if self.mappings.is_empty() {
            return Ok(ResolvedCustomTypeMappings::default());
        }

        let targets = bindings
            .decls()
            .iter()
            .filter_map(|declaration| DeclarationRef::from(declaration).custom_type())
            .map(|declaration| {
                CustomTypeMappingTarget::new(declaration, target_type_name(declaration))
            })
            .collect::<Vec<_>>();
        let mappings = self.mappings.iter().try_fold(
            BTreeMap::new(),
            |mut resolved, (custom_type, mapping)| {
                let target_type = targets
                    .iter()
                    .find(|target_type| target_type.matches(custom_type))
                    .ok_or_else(|| Error::UnknownCustomTypeMapping {
                        target,
                        custom_type: custom_type.clone(),
                    })?;
                resolved.insert(target_type.id(), mapping.clone());
                Ok::<_, Error>(resolved)
            },
        )?;
        Ok(ResolvedCustomTypeMappings { mappings })
    }
}

impl ResolvedCustomTypeMappings {
    /// Returns the mapping for a custom type id.
    pub fn get(&self, id: CustomTypeId) -> Option<&CustomTypeMapping> {
        self.mappings.get(&id)
    }
}

impl<'declaration> CustomTypeMappingTarget<'declaration> {
    fn new(declaration: &'declaration CustomTypeDecl, target_type_name: String) -> Self {
        Self {
            declaration,
            target_type_name,
            canonical_name: declaration.name().as_path_string(),
        }
    }

    fn id(&self) -> CustomTypeId {
        self.declaration.id()
    }

    fn matches(&self, custom_type: &str) -> bool {
        self.target_type_name == custom_type || self.canonical_name == custom_type
    }
}
