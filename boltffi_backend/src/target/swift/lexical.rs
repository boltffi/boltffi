use crate::{
    core::{
        Result,
        lexical::{IdentifierKey, LexicalPolicy, NameOrdinal, NameStem, Shadowing},
        name_case,
    },
    target::swift::syntax::{Identifier, Syntax},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeForm {
    Closure,
    GuardContinuation,
}

impl LexicalPolicy for Syntax {
    type ScopeForm = ScopeForm;

    fn key(identifier: &Self::Identifier) -> IdentifierKey {
        IdentifierKey::new(identifier.as_str())
    }

    fn generated(stem: &NameStem, ordinal: NameOrdinal) -> Result<Self::Identifier> {
        let stem = stem
            .parts()
            .map(name_case::upper_camel_from_snake)
            .collect::<String>();
        let ordinal = if ordinal.get() > 1 {
            ordinal.get().to_string()
        } else {
            String::new()
        };
        Identifier::parse(format!("boltffi{stem}{ordinal}"))
    }

    fn shadowing(form: Self::ScopeForm) -> Shadowing {
        match form {
            ScopeForm::Closure | ScopeForm::GuardContinuation => Shadowing::Allow,
        }
    }
}
