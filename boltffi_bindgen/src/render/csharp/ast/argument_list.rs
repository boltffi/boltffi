//! C# `argument_list` production: a comma-separated sequence of
//! arguments inside a method invocation's parentheses. Display joins
//! with `, `; an empty list renders as the empty string so call sites
//! can drop it directly between the open and close parens.

use std::fmt;

use super::CSharpExpression;

/// A typed C# argument list, used wherever the lowerer pre-computes
/// the arguments handed to a method call (today: the `[DllImport]`
/// invocation's argument list, possibly prefixed with a receiver-self
/// argument).
#[derive(Debug, Clone, Default)]
pub struct CSharpArgumentList(Vec<CSharpExpression>);

impl CSharpArgumentList {
    pub fn new(args: Vec<CSharpExpression>) -> Self {
        Self(args)
    }

    pub fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, arg: CSharpExpression) {
        self.0.push(arg);
    }

    pub fn extend(&mut self, args: impl IntoIterator<Item = CSharpExpression>) {
        self.0.extend(args);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CSharpArgumentList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, arg) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{arg}")?;
        }
        Ok(())
    }
}

impl IntoIterator for CSharpArgumentList {
    type Item = CSharpExpression;
    type IntoIter = std::vec::IntoIter<CSharpExpression>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{CSharpIdent, CSharpLiteral, CSharpLocalName};

    fn ident(name: &str) -> CSharpExpression {
        CSharpExpression::Ident(CSharpIdent::Local(CSharpLocalName::new(name)))
    }

    fn int(v: i64) -> CSharpExpression {
        CSharpExpression::Literal(CSharpLiteral::Int(v))
    }

    #[test]
    fn empty_list_renders_as_empty_string() {
        assert_eq!(CSharpArgumentList::empty().to_string(), "");
    }

    #[test]
    fn single_arg_renders_without_separator() {
        let list = CSharpArgumentList::new(vec![ident("value")]);
        assert_eq!(list.to_string(), "value");
    }

    #[test]
    fn multiple_args_join_with_comma_space() {
        let list = CSharpArgumentList::new(vec![ident("v"), int(16), ident("count")]);
        assert_eq!(list.to_string(), "v, 16, count");
    }

    #[test]
    fn extend_appends_to_existing_list() {
        let mut list = CSharpArgumentList::new(vec![ident("self")]);
        list.extend(vec![ident("x"), ident("y")]);
        assert_eq!(list.to_string(), "self, x, y");
    }
}
