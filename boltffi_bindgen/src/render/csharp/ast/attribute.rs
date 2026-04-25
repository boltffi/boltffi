//! C# attribute application: the `[Name(args)]` decoration that
//! precedes a declaration.

use std::fmt;

use super::{CSharpClassName, CSharpExpression, CSharpPropertyName};

/// A C# attribute application: a name and an optional argument list,
/// rendered between square brackets.
///
/// Examples:
/// ```csharp
/// [Serializable]
/// [MarshalAs(UnmanagedType.I1)]
/// [MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)]
/// ```
#[derive(Debug, Clone)]
pub(crate) struct CSharpAttribute {
    pub(crate) name: CSharpClassName,
    pub(crate) args: Vec<CSharpAttributeArg>,
}

/// An argument inside an attribute's parens. Either positional or
/// the `name = value` named form.
///
/// Examples:
/// ```csharp
/// // Positional
/// UnmanagedType.I1
///
/// // Named
/// ArraySubType = UnmanagedType.U1
/// ```
#[derive(Debug, Clone)]
pub(crate) enum CSharpAttributeArg {
    Positional(CSharpExpression),
    Named {
        name: CSharpPropertyName,
        value: CSharpExpression,
    },
}

impl fmt::Display for CSharpAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}", self.name)?;
        if !self.args.is_empty() {
            f.write_str("(")?;
            for (i, a) in self.args.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{a}")?;
            }
            f.write_str(")")?;
        }
        f.write_str("]")
    }
}

impl fmt::Display for CSharpAttributeArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Positional(expr) => expr.fmt(f),
            Self::Named { name, value } => write!(f, "{name} = {value}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CSharpExpression, CSharpTypeReference};
    use super::*;

    fn unmanaged_type_member(member: &str) -> CSharpExpression {
        CSharpExpression::MemberAccess {
            receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                CSharpClassName::new("UnmanagedType"),
            ))),
            name: CSharpPropertyName::from_source(member),
        }
    }

    #[test]
    fn no_arg_attribute_renders_with_empty_brackets() {
        let attr = CSharpAttribute {
            name: CSharpClassName::new("Serializable"),
            args: vec![],
        };
        assert_eq!(attr.to_string(), "[Serializable]");
    }

    /// `[MarshalAs(UnmanagedType.I1)]` is the canonical positional-only
    /// shape used on bool params so P/Invoke marshals one byte instead
    /// of the 4-byte Win32 BOOL default.
    #[test]
    fn single_positional_attribute_renders_canonical_marshal_as_form() {
        let attr = CSharpAttribute {
            name: CSharpClassName::new("MarshalAs"),
            args: vec![CSharpAttributeArg::Positional(unmanaged_type_member("I1"))],
        };
        assert_eq!(attr.to_string(), "[MarshalAs(UnmanagedType.I1)]");
    }

    /// `[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)]`
    /// mixes one positional and one named arg, with `name = value`
    /// formatting on the named one.
    #[test]
    fn positional_plus_named_attribute_renders_mixed_arg_list() {
        let attr = CSharpAttribute {
            name: CSharpClassName::new("MarshalAs"),
            args: vec![
                CSharpAttributeArg::Positional(unmanaged_type_member("LPArray")),
                CSharpAttributeArg::Named {
                    name: CSharpPropertyName::from_source("array_sub_type"),
                    value: unmanaged_type_member("U1"),
                },
            ],
        };
        assert_eq!(
            attr.to_string(),
            "[MarshalAs(UnmanagedType.LPArray, ArraySubType = UnmanagedType.U1)]"
        );
    }
}
