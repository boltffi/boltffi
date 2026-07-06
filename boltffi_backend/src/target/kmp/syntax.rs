use std::fmt;

use crate::core::{LanguageSyntax, syntax::sealed};

/// Kotlin syntax fragment family used by the KMP backend.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// Opaque Kotlin syntax fragment placeholder.
///
/// The current KMP emitter renders directly from the KMP module plan. This
/// fragment type keeps the backend trait implementation typed while future
/// renderers can introduce role-specific fragments where they are useful.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Fragment(String);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "as",
        "break",
        "class",
        "continue",
        "do",
        "else",
        "false",
        "for",
        "fun",
        "if",
        "in",
        "interface",
        "is",
        "null",
        "object",
        "package",
        "return",
        "super",
        "this",
        "throw",
        "true",
        "try",
        "typealias",
        "typeof",
        "val",
        "var",
        "when",
        "while",
    ];

    type Identifier = Fragment;
    type Type = Fragment;
    type Expr = Fragment;
    type Stmt = Fragment;
    type Literal = Fragment;
    type Arguments = Fragment;
}

impl sealed::LanguageSyntax for Syntax {}
impl sealed::SyntaxFragment for Fragment {}

impl fmt::Display for Fragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
