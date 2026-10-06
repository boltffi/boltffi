use std::fmt;

use crate::{
    bridge::c,
    core::{LanguageSyntax, syntax::sealed},
};

/// Ruby syntax fragment family.
///
/// The Ruby target emits a C extension, so its types, expressions,
/// statements, and argument lists are the C bridge fragments. The Ruby-facing
/// names inside that C (method names and `Data` member names) and the
/// literals in the Ruby package files use the fragments below. Each fragment
/// gets its constructor with the renderer that first builds it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A Ruby method or `Data` member name, such as `echo_string`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

/// A double-quoted Ruby string literal.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "BEGIN",
        "END",
        "__ENCODING__",
        "__FILE__",
        "__LINE__",
        "alias",
        "and",
        "begin",
        "break",
        "case",
        "class",
        "def",
        "defined?",
        "do",
        "else",
        "elsif",
        "end",
        "ensure",
        "false",
        "for",
        "if",
        "in",
        "module",
        "next",
        "nil",
        "not",
        "or",
        "redo",
        "rescue",
        "retry",
        "return",
        "self",
        "super",
        "then",
        "true",
        "undef",
        "unless",
        "until",
        "when",
        "while",
        "yield",
    ];

    type Identifier = Identifier;
    type Type = c::TypeFragment;
    type Expr = c::Expression;
    type Stmt = c::Statement;
    type Literal = Literal;
    type Arguments = c::ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Identifier {}
impl sealed::SyntaxFragment for Literal {}
