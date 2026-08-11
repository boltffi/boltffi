//! Minimal Dart source-text helpers: identifier escaping and string
//! literal quoting. Unlike `target::typescript`'s typed AST (`syntax::*`),
//! this target builds source as plain strings — its job is thin JS
//! interop glue over an already-generated module, not a marshalling
//! codec, so a typed AST buys little here.

/// Reserved words that cannot be used as a plain Dart identifier.
/// (Built-in identifiers like `abstract`/`async` are contextually legal
/// and deliberately excluded — only the unconditionally reserved set
/// needs escaping.)
pub const DART_KEYWORDS: &[&str] = &[
    "assert", "break", "case", "catch", "class", "const", "continue", "default", "do", "else",
    "enum", "extends", "false", "final", "finally", "for", "if", "in", "is", "new", "null",
    "rethrow", "return", "super", "switch", "this", "throw", "true", "try", "var", "void", "while",
    "with",
];

/// Escapes `value` if it collides with a reserved Dart identifier by
/// appending an underscore, matching the convention already used by
/// `target::dart` and `target::typescript`.
pub fn escape_dart_identifier(value: impl Into<String>) -> String {
    let value = value.into();
    if DART_KEYWORDS.contains(&value.as_str()) {
        format!("{value}_")
    } else {
        value
    }
}

/// Quotes a Dart string literal, escaping backslashes, quotes, `$`
/// (Dart string interpolation), and control characters.
pub fn dart_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

// -- Minimal `LanguageSyntax` conformance ---------------------------------
//
// This target renders plain strings rather than a typed AST (see
// `render.rs`'s doc comment for why), so every associated type below is
// the same thin `Fragment(String)` wrapper. The trait still has to be
// satisfied because `host::HostBackend::Syntax` requires it.

use std::fmt;

use crate::core::syntax::{LanguageSyntax, sealed};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Fragment(String);

impl Fragment {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for Fragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Fragment {}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

impl sealed::LanguageSyntax for Syntax {}

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = DART_KEYWORDS;

    type Identifier = Fragment;
    type Type = Fragment;
    type Expr = Fragment;
    type Stmt = Fragment;
    type Literal = Fragment;
    type Arguments = Fragment;
}
