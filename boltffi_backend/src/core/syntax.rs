//! Typed syntax fragments for backend renderers.

use std::fmt;

pub(crate) mod sealed {
    pub trait SyntaxFragment {}
    pub trait LanguageSyntax {}
}

/// A rendered syntax fragment with a language-owned grammar role.
///
/// Backend models use syntax fragments instead of raw strings when a
/// value represents generated source code. Each language decides which
/// concrete fragment types are valid for identifiers, types, expressions,
/// statements, and literals.
pub trait SyntaxFragment: fmt::Display + Clone + sealed::SyntaxFragment {}

impl<T> SyntaxFragment for T where T: fmt::Display + Clone + sealed::SyntaxFragment {}

/// Syntax fragment family for one generated language.
///
/// The associated types keep target render models from mixing generated
/// code roles. An identifier field cannot receive an expression unless
/// the language explicitly models that value as an identifier.
pub trait LanguageSyntax: sealed::LanguageSyntax {
    /// Keyword table for the default predicate.
    /// Parser-backed languages can override `keyword` instead.
    const KEYWORDS: &'static [&'static str] = &[];

    /// Identifier syntax accepted by the language.
    type Identifier: SyntaxFragment;
    /// Type syntax accepted by the language.
    type Type: SyntaxFragment;
    /// Expression syntax accepted by the language.
    type Expr: SyntaxFragment;
    /// Statement syntax accepted by the language.
    type Stmt: SyntaxFragment;
    /// Literal syntax accepted by the language.
    type Literal: SyntaxFragment;
    /// Function or call argument list syntax accepted by the language.
    type Arguments: SyntaxFragment;

    /// Returns whether the text is reserved by the language grammar.
    fn keyword(identifier: &str) -> bool {
        Self::KEYWORDS.contains(&identifier)
    }
}
