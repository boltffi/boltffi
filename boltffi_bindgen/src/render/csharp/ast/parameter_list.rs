//! A C# method's parameter declaration: a single parameter and the
//! comma-separated list of them between the method's parens.

use std::fmt;

use super::{CSharpAttribute, CSharpParamName, CSharpType};

/// A single C# parameter declaration: optional attributes, then the
/// type, then the name.
///
/// Examples:
/// ```csharp
/// int value
/// byte[] data
/// [MarshalAs(UnmanagedType.I1)] bool flag
/// ```
#[derive(Debug, Clone)]
pub(crate) struct CSharpParameter {
    pub(crate) attributes: Vec<CSharpAttribute>,
    pub(crate) csharp_type: CSharpType,
    pub(crate) name: CSharpParamName,
}

impl CSharpParameter {
    /// A bare parameter with no attributes. The common case for
    /// public wrapper signatures.
    pub(crate) fn bare(csharp_type: CSharpType, name: CSharpParamName) -> Self {
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

/// The comma-separated parameters between the parens of a C# method
/// declaration.
///
/// Examples:
/// ```csharp
/// // Empty
///
/// // Single
/// int value
///
/// // Multiple
/// [MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, uint count
/// ```
#[derive(Debug, Clone, Default)]
pub(crate) struct CSharpParameterList(Vec<CSharpParameter>);

impl CSharpParameterList {
    pub(crate) fn empty() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn push(&mut self, param: CSharpParameter) {
        self.0.push(param);
    }

    pub(crate) fn extend(&mut self, params: impl IntoIterator<Item = CSharpParameter>) {
        self.0.extend(params);
    }
}

impl From<Vec<CSharpParameter>> for CSharpParameterList {
    fn from(params: Vec<CSharpParameter>) -> Self {
        Self(params)
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
    use super::super::{
        CSharpAttribute, CSharpAttributeArg, CSharpClassName, CSharpExpression, CSharpPropertyName,
        CSharpTypeReference,
    };
    use super::*;

    fn marshal_as(member: &str) -> CSharpAttribute {
        CSharpAttribute {
            name: CSharpClassName::new("MarshalAs"),
            args: vec![CSharpAttributeArg::Positional(
                CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                        CSharpClassName::new("UnmanagedType"),
                    ))),
                    name: CSharpPropertyName::from_source(member),
                },
            )],
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
        let list: CSharpParameterList = vec![param("v", CSharpType::String)].into();
        assert_eq!(list.to_string(), "string v");
    }

    /// A mixed list pins the canonical DllImport shape: an attribute-
    /// decorated bool, a string split into two slots, and a primitive
    /// at the end. Templates rely on this exact spacing.
    #[test]
    fn mixed_list_pins_canonical_dllimport_param_spacing() {
        let list: CSharpParameterList = vec![
            CSharpParameter {
                attributes: vec![marshal_as("I1")],
                csharp_type: CSharpType::Bool,
                name: CSharpParamName::from_source("flag"),
            },
            param("v", CSharpType::Array(Box::new(CSharpType::Byte))),
            param("vLen", CSharpType::UIntPtr),
            param("count", CSharpType::UInt),
        ]
        .into();
        assert_eq!(
            list.to_string(),
            "[MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, UIntPtr vLen, uint count"
        );
    }
}
