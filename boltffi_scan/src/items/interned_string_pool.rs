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
        Self::parse_tokens(marked.item().mac.tokens.clone())
    }

    fn parse_tokens(tokens: proc_macro2::TokenStream) -> Result<Self, ScanError> {
        let parsed =
            syn::parse2::<ParsedSpec>(tokens).map_err(|err| ScanError::InvalidCustomType {
                message: format!("invalid interned_string_pool invocation: {err}"),
            })?;
        Self::from_parsed(parsed)
    }

    fn from_parsed(parsed: ParsedSpec) -> Result<Self, ScanError> {
        let mut seen = std::collections::HashSet::<String>::new();
        for entry in &parsed.entries {
            let value = entry.value.value();
            if !seen.insert(value.clone()) {
                return Err(ScanError::InvalidCustomType {
                    message: format!(
                        "duplicate string value {value:?} in interned_string_pool `{}`",
                        parsed.name
                    ),
                });
            }
        }
        Ok(Self {
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

#[cfg(test)]
mod tests {
    use quote::quote;

    use boltffi_ast::PackageInfo;

    use crate::ScanError;

    #[test]
    fn scan_file_rejects_duplicate_string_values() {
        let file = syn::parse2(quote! {
            boltffi::interned_string_pool! {
                pub BrowserName {
                    CHROME = "Chrome",
                    CHROMIUM = "Chrome",
                }
            }
        })
        .expect("valid Rust syntax");
        let error = match crate::scan_file(file, PackageInfo::new("demo", None)) {
            Ok(_) => panic!("duplicate value must fail"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            ScanError::InvalidCustomType { ref message }
                if message.contains("duplicate string value \"Chrome\"")
                    && message.contains("BrowserName")
        ));
    }
}
