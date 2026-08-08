use std::fmt;

use crate::core::{LanguageSyntax, syntax::sealed};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Fragment(String);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[];

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
