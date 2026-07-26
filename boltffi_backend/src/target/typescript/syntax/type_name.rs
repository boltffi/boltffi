use std::fmt;

use crate::core::syntax::sealed;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(String);

impl TypeName {
    pub fn void() -> Self {
        Self("void".to_owned())
    }

    pub fn boolean() -> Self {
        Self("boolean".to_owned())
    }

    pub fn number() -> Self {
        Self("number".to_owned())
    }

    pub fn bigint() -> Self {
        Self("bigint".to_owned())
    }

    pub fn string() -> Self {
        Self("string".to_owned())
    }

    pub fn named(name: impl fmt::Display) -> Self {
        Self(name.to_string())
    }

    pub fn generic(name: impl fmt::Display, arguments: impl IntoIterator<Item = Self>) -> Self {
        Self(format!(
            "{name}<{}>",
            arguments
                .into_iter()
                .map(|argument| argument.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn tuple(elements: impl IntoIterator<Item = Self>) -> Self {
        Self(format!(
            "[{}]",
            elements
                .into_iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn array(element: Self) -> Self {
        Self(format!("{element}[]"))
    }

    pub fn readonly_array(element: Self) -> Self {
        Self(format!("readonly {element}[]"))
    }

    pub fn union(left: Self, right: Self) -> Self {
        Self(format!("{left} | {right}"))
    }

    pub fn nullable(self) -> Self {
        Self::union(self, Self::named("null"))
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for TypeName {}
