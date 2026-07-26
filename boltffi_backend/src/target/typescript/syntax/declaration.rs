use std::fmt;

use crate::core::syntax::sealed;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MethodDeclaration(String);

impl MethodDeclaration {
    pub fn new(source: impl Into<String>) -> Self {
        Self(source.into())
    }
}

impl fmt::Display for MethodDeclaration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for MethodDeclaration {}
