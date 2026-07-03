use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Visibility, braced};

use crate::ScanError;
use crate::marked::MarkedInternedStringPool;

pub struct Spec {
    name: Ident,
    values: Vec<String>,
}

struct ParsedSpec {
    _visibility: Visibility,
    name: Ident,
    entries: Vec<ParsedEntry>,
}

struct ParsedEntry {
    _name: Ident,
    value: LitStr,
}

impl Spec {
    pub fn parse(marked: &MarkedInternedStringPool<'_>) -> Result<Self, ScanError> {
        syn::parse2::<ParsedSpec>(marked.item().mac.tokens.clone())
            .map_err(|err| ScanError::InvalidCustomType {
                message: format!("invalid interned_string_pool invocation: {err}"),
            })
            .map(|parsed| Self {
                name: parsed.name,
                values: parsed
                    .entries
                    .into_iter()
                    .map(|entry| entry.value.value())
                    .collect(),
            })
    }

    pub fn name(&self) -> &Ident {
        &self.name
    }

    pub fn values(&self) -> &[String] {
        &self.values
    }
}

impl Parse for ParsedSpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let visibility = input.parse()?;
        let name = input.parse()?;
        let content;
        braced!(content in input);
        let mut entries = Vec::new();
        while !content.is_empty() {
            let entry_name = content.parse()?;
            content.parse::<Token![=]>()?;
            let value = content.parse()?;
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
            entries.push(ParsedEntry {
                _name: entry_name,
                value,
            });
        }
        Ok(Self {
            _visibility: visibility,
            name,
            entries,
        })
    }
}
