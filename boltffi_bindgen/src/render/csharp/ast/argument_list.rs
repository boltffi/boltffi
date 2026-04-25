use std::fmt;

use super::CSharpExpression;

/// The list of comma-separated arguments needed when calling (invoking) a method.
///
/// Examples:
/// ```csharp
/// // A single argument
/// v
///
/// // Multiple arguments
/// v, 16, count
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CSharpArgumentList(Vec<CSharpExpression>);

impl CSharpArgumentList {
    pub(crate) fn empty() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn push(&mut self, arg: CSharpExpression) {
        self.0.push(arg);
    }

    pub(crate) fn extend(&mut self, args: impl IntoIterator<Item = CSharpExpression>) {
        self.0.extend(args);
    }
}

impl From<Vec<CSharpExpression>> for CSharpArgumentList {
    fn from(args: Vec<CSharpExpression>) -> Self {
        Self(args)
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
    use super::super::{CSharpIdentity, CSharpLiteral, CSharpLocalName};
    use super::*;

    fn ident(name: &str) -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(name)))
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
        let list: CSharpArgumentList = vec![ident("value")].into();
        assert_eq!(list.to_string(), "value");
    }

    #[test]
    fn multiple_args_join_with_comma_space() {
        let list: CSharpArgumentList = vec![ident("v"), int(16), ident("count")].into();
        assert_eq!(list.to_string(), "v, 16, count");
    }

    #[test]
    fn extend_appends_to_existing_list() {
        let mut list: CSharpArgumentList = vec![ident("self")].into();
        list.extend(vec![ident("x"), ident("y")]);
        assert_eq!(list.to_string(), "self, x, y");
    }
}
