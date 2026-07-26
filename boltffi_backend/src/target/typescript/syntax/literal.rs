use std::fmt;

use crate::core::syntax::sealed;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StringLiteral(String);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IntegerLiteral {
    value: i128,
    bigint: bool,
}

impl StringLiteral {
    pub fn new(value: &str) -> Self {
        Self(serde_json::to_string(value).expect("string literal serializes"))
    }
}

impl IntegerLiteral {
    pub fn number(value: i128) -> Self {
        Self {
            value,
            bigint: false,
        }
    }

    pub fn bigint(value: i128) -> Self {
        Self {
            value,
            bigint: true,
        }
    }
}

impl fmt::Display for StringLiteral {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for IntegerLiteral {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}",
            self.value,
            if self.bigint { "n" } else { "" }
        )
    }
}

impl sealed::SyntaxFragment for StringLiteral {}
impl sealed::SyntaxFragment for IntegerLiteral {}
