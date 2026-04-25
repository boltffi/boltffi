//! C# `parameter` and `parameter_list` productions: a single
//! parameter (optional attributes, type, name) and a comma-separated
//! list of them inside the parens of a method declaration.

use std::fmt;

use super::{CSharpAttribute, CSharpParamName, CSharpType};

/// A single C# parameter declaration. Attributes (today: `MarshalAs`)
/// render before the type, separated by spaces. The name follows the
/// type, separated by one space.
#[derive(Debug, Clone)]
pub struct CSharpParameter {
    pub attributes: Vec<CSharpAttribute>,
    pub csharp_type: CSharpType,
    pub name: CSharpParamName,
}

impl CSharpParameter {
    /// A bare parameter with no attributes — the common case for
    /// public wrapper signatures.
    pub fn bare(csharp_type: CSharpType, name: CSharpParamName) -> Self {
        Self {
            attributes: vec![],
            csharp_type,
            name,
        }
    }
}

impl fmt::Display for CSharpParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for attr in &self.attributes {
            write!(f, "{attr} ")?;
        }
        write!(f, "{} {}", self.csharp_type, self.name)
    }
}

/// A typed C# parameter list. Display joins parameters with `, `;
/// an empty list renders as the empty string so call sites can drop
/// it directly between the open and close parens.
#[derive(Debug, Clone, Default)]
pub struct CSharpParameterList(Vec<CSharpParameter>);

impl CSharpParameterList {
    pub fn new(params: Vec<CSharpParameter>) -> Self {
        Self(params)
    }

    pub fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, param: CSharpParameter) {
        self.0.push(param);
    }

    pub fn extend(&mut self, params: impl IntoIterator<Item = CSharpParameter>) {
        self.0.extend(params);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for CSharpParameterList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, p) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{p}")?;
        }
        Ok(())
    }
}

impl IntoIterator for CSharpParameterList {
    type Item = CSharpParameter;
    type IntoIter = std::vec::IntoIter<CSharpParameter>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{
        CSharpAttribute, CSharpAttributeArg, CSharpClassName, CSharpExpression, CSharpPropertyName,
        CSharpTypeReference,
    };

    fn marshal_as(member: &str) -> CSharpAttribute {
        CSharpAttribute {
            name: CSharpClassName::new("MarshalAs"),
            args: vec![CSharpAttributeArg::Positional(CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                    CSharpClassName::new("UnmanagedType"),
                ))),
                name: CSharpPropertyName::from_source(member),
            })],
        }
    }

    fn param(name: &str, csharp_type: CSharpType) -> CSharpParameter {
        CSharpParameter::bare(csharp_type, CSharpParamName::from_source(name))
    }

    #[test]
    fn bare_parameter_renders_type_space_name() {
        let p = param("value", CSharpType::Int);
        assert_eq!(p.to_string(), "int value");
    }

    /// A parameter with one attribute renders the attribute, a single
    /// space, then the bare type-space-name. Matches today's `[MarshalAs(I1)] bool flag`.
    #[test]
    fn parameter_with_attribute_renders_attribute_then_type_name() {
        let p = CSharpParameter {
            attributes: vec![marshal_as("I1")],
            csharp_type: CSharpType::Bool,
            name: CSharpParamName::from_source("flag"),
        };
        assert_eq!(p.to_string(), "[MarshalAs(UnmanagedType.I1)] bool flag");
    }

    #[test]
    fn empty_list_renders_as_empty_string() {
        assert_eq!(CSharpParameterList::empty().to_string(), "");
    }

    #[test]
    fn single_param_renders_without_separator() {
        let list = CSharpParameterList::new(vec![param("v", CSharpType::String)]);
        assert_eq!(list.to_string(), "string v");
    }

    /// A mixed list pins the canonical DllImport shape: an attribute-
    /// decorated bool, a string split into two slots, and a primitive
    /// at the end. Templates rely on this exact spacing.
    #[test]
    fn mixed_list_pins_canonical_dllimport_param_spacing() {
        let list = CSharpParameterList::new(vec![
            CSharpParameter {
                attributes: vec![marshal_as("I1")],
                csharp_type: CSharpType::Bool,
                name: CSharpParamName::from_source("flag"),
            },
            param("v", CSharpType::Array(Box::new(CSharpType::Byte))),
            param("vLen", CSharpType::UIntPtr),
            param("count", CSharpType::UInt),
        ]);
        assert_eq!(
            list.to_string(),
            "[MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, UIntPtr vLen, uint count"
        );
    }
}
