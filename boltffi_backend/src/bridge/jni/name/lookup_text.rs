use crate::bridge::c::Literal;

use super::ModifiedUtf8;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LookupText {
    lookup: Literal,
    diagnostic: Literal,
}

impl LookupText {
    pub fn new(value: &str) -> Self {
        Self {
            lookup: ModifiedUtf8::new(value).literal(),
            diagnostic: Literal::byte_string(value.as_bytes()),
        }
    }

    pub fn lookup(&self) -> &Literal {
        &self.lookup
    }

    pub fn diagnostic(&self) -> &Literal {
        &self.diagnostic
    }
}

#[cfg(test)]
mod tests {
    use super::LookupText;

    #[test]
    fn separates_modified_utf8_lookups_from_utf8_diagnostics() {
        let text = LookupText::new("class_𐐀😀");

        assert_eq!(
            text.lookup().to_string(),
            "\"class_\\355\\240\\201\\355\\260\\200\\355\\240\\275\\355\\270\\200\""
        );
        assert_eq!(
            text.diagnostic().to_string(),
            "\"class_\\360\\220\\220\\200\\360\\237\\230\\200\""
        );
    }
}
