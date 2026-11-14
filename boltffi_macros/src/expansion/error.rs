use std::{error, fmt};

use boltffi_ast::DeclarationId as SourceDeclarationId;
use boltffi_binding::{DeclarationId, LowerError, SourceLocation, SourcePosition};

#[derive(Debug)]
pub enum Error {
    Lower(LowerError),

    MissingBinding(SourceDeclarationId),

    MissingDeclaration(DeclarationId),

    WrongDeclaration,

    SourceSyntaxMismatch(&'static str),

    UnsupportedExpansion(&'static str),

    Located(Box<Error>, SourceLocation),
}

impl Error {
    /// Where in the source the error was raised, when the lowering pass or expansion knew it.
    pub fn location(&self) -> Option<&SourceLocation> {
        match self {
            Self::Lower(error) => Some(error.location()),
            Self::Located(_, location) => Some(location),
            _ => None,
        }
    }

    pub fn in_declaration(self, id: impl fmt::Display) -> Self {
        self.locate(|location| location.in_declaration(id))
    }

    pub fn in_member(self, name: impl fmt::Display) -> Self {
        self.locate(|location| location.in_member(name))
    }

    pub fn at(self, position: SourcePosition) -> Self {
        self.locate(|location| location.at(position))
    }

    fn locate(self, update: impl FnOnce(SourceLocation) -> SourceLocation) -> Self {
        match self {
            Self::Lower(error) => Self::Lower(error),
            Self::Located(error, location) => Self::Located(error, update(location)),
            error => Self::Located(Box::new(error), update(SourceLocation::default())),
        }
    }
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
            Self::Located(error, _) => error.fmt(formatter),
        }
    }
}

impl error::Error for Error {}
