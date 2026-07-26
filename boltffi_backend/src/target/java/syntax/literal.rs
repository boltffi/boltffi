use std::fmt::{self, Write};

use boltffi_binding::DocComment;

use crate::core::syntax::sealed;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StringLiteral(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Javadoc(String);

impl StringLiteral {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn format_contents(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.chars().try_for_each(|character| match character {
            '\u{0008}' => formatter.write_str("\\b"),
            '\t' => formatter.write_str("\\t"),
            '\n' => formatter.write_str("\\n"),
            '\u{000c}' => formatter.write_str("\\f"),
            '\r' => formatter.write_str("\\r"),
            '"' => formatter.write_str("\\\""),
            '\\' => formatter.write_str("\\\\"),
            control if control.is_control() && u32::from(control) <= u32::from(u8::MAX) => {
                write!(formatter, "\\{:03o}", u32::from(control))
            }
            control if control.is_control() => {
                let mut encoded = [0; 2];
                control
                    .encode_utf16(&mut encoded)
                    .iter()
                    .try_for_each(|unit| write!(formatter, "\\u{unit:04x}"))
            }
            other => formatter.write_char(other),
        })
    }
}

impl Javadoc {
    pub fn new(doc: &DocComment) -> Self {
        let body = doc
            .as_str()
            .lines()
            .map(|line| line.replace('\\', "&#92;").replace("*/", "*&#47;"))
            .map(|line| format!("     * {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let body = match body.is_empty() {
            true => "     *".to_owned(),
            false => body,
        };
        Self(format!("    /**\n{body}\n     */"))
    }
}

impl fmt::Display for StringLiteral {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("\"")?;
        self.format_contents(formatter)?;
        formatter.write_str("\"")
    }
}

impl fmt::Display for Javadoc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for StringLiteral {}

#[cfg(test)]
mod tests {
    use boltffi_binding::DocComment;

    use super::{Javadoc, StringLiteral};

    #[test]
    fn renders_safe_java_string_literals() {
        assert_eq!(
            StringLiteral::new("quote\" slash\\ line\n\0").to_string(),
            "\"quote\\\" slash\\\\ line\\n\\000\""
        );
    }

    #[test]
    fn renders_javadoc_without_lexical_escape_hatches() {
        let rendered = Javadoc::new(&DocComment::new("safe */ text \\u002a\\u002f")).to_string();
        assert!(rendered.contains("safe *&#47; text &#92;u002a&#92;u002f"));
        assert!(!rendered[4..rendered.len() - 3].contains("*/"));
    }
}
