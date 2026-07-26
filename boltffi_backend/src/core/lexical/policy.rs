use crate::core::{LanguageSyntax, Result};

use super::{IdentifierKey, NameOrdinal, NameStem};

pub trait LexicalPolicy: LanguageSyntax {
    type ScopeForm: Copy;

    fn key(identifier: &Self::Identifier) -> IdentifierKey;
    fn generated(stem: &NameStem, ordinal: NameOrdinal) -> Result<Self::Identifier>;
    fn shadowing(form: Self::ScopeForm) -> Shadowing;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shadowing {
    Allow,
    Forbid,
}
