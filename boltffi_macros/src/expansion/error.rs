use std::{error, fmt};

use boltffi_ast::DeclarationId as SourceDeclarationId;
use boltffi_binding::{DeclarationId, LowerError};

#[derive(Debug)]
pub enum Error {
    Lower(LowerError),

    MissingBinding(SourceDeclarationId),

    MissingDeclaration(DeclarationId),

    WrongDeclaration,

    SourceSyntaxMismatch(&'static str),

    UnsupportedExpansion(&'static str),
}

impl From<LowerError> for Error {
    fn from(error: LowerError) -> Self {
        Self::Lower(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lower(error) => error.fmt(formatter),
            Self::MissingBinding(id) => write!(formatter, "missing binding declaration for {id:?}"),
            Self::MissingDeclaration(id) => {
                write!(formatter, "missing lowered declaration for {id:?}")
            }
            Self::WrongDeclaration => {
                formatter.write_str("lowered declaration has the wrong source kind")
            }
            Self::SourceSyntaxMismatch(message) => formatter.write_str(message),
            Self::UnsupportedExpansion(kind) => write!(formatter, "unsupported expansion: {kind}"),
        }
    }
}

impl error::Error for Error {}
