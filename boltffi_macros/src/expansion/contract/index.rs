use std::collections::HashMap;

use boltffi_binding::{CustomTypeDecl, CustomTypeId, Decl, DeclarationId, DeclarationMap, Surface};

use super::pair::{PairedDeclaration, SourceDeclaration};
use crate::expansion::error::Error;

pub struct ExpansionIndex {
    binding_by_id: HashMap<DeclarationId, usize>,
}

impl ExpansionIndex {
    pub fn new<S: Surface>(declarations: &[Decl<S>]) -> Self {
        Self {
            binding_by_id: declarations
                .iter()
                .enumerate()
                .map(|(index, decl)| (decl.id(), index))
                .collect(),
        }
    }

    pub fn paired<'lowered, S: Surface>(
        &self,
        decls: &'lowered [Decl<S>],
        declarations: &DeclarationMap,
        source: SourceDeclaration<'lowered>,
    ) -> Result<PairedDeclaration<'lowered, S>, Error> {
        let source_id = source.id();
        let binding_id = declarations
            .get(&source_id)
            .ok_or_else(|| Error::MissingBinding(source_id.clone()))?;
        let binding_index = self
            .binding_by_id
            .get(&binding_id)
            .copied()
            .ok_or(Error::MissingDeclaration(binding_id))?;
        let binding = decls
            .get(binding_index)
            .ok_or(Error::MissingDeclaration(binding_id))?;
        source.pair(binding)
    }

    pub fn custom_type<'lowered, S: Surface>(
        &self,
        decls: &'lowered [Decl<S>],
        id: CustomTypeId,
    ) -> Result<&'lowered CustomTypeDecl, Error> {
        let declaration_id = DeclarationId::CustomType(id);
        let index = self
            .binding_by_id
            .get(&declaration_id)
            .copied()
            .ok_or(Error::MissingDeclaration(declaration_id))?;
        match decls.get(index) {
            Some(Decl::CustomType(custom)) => Ok(custom),
            _ => Err(Error::WrongDeclaration),
        }
    }
}
