use std::collections::HashSet;

use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Visibility, braced};

use crate::ScanError;
use crate::capture::CapturedPool;

struct ParsedSpec {
    name: Ident,
    values: Vec<LitStr>,
}

pub(crate) fn parse_tokens(tokens: proc_macro2::TokenStream) -> Result<CapturedPool, ScanError> {
    let parsed = syn::parse2::<ParsedSpec>(tokens).map_err(|err| ScanError::InvalidCustomType {
        message: format!("invalid interned_string_pool invocation: {err}"),
    })?;
    let values = parsed.values.iter().map(LitStr::value).collect::<Vec<_>>();
    let mut seen = HashSet::new();
    match values.iter().find(|value| !seen.insert(*value)) {
        Some(value) => Err(ScanError::InvalidCustomType {
            message: format!(
                "duplicate string value {value:?} in interned_string_pool `{}`",
                parsed.name
            ),
        }),
        None => Ok(CapturedPool {
            name: parsed.name.to_string(),
            values,
        }),
    }
}

impl Parse for ParsedSpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Visibility>()?;
        let name = input.parse()?;
        let content;
        braced!(content in input);
        let mut values = Vec::new();
        while !content.is_empty() {
            content.parse::<Ident>()?;
            content.parse::<Token![=]>()?;
            values.push(content.parse()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self { name, values })
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::parse_tokens;
    use crate::ScanError;

    #[test]
    fn rejects_duplicate_string_values() {
        let error = match parse_tokens(quote! {
            pub BrowserName {
                CHROME = "Chrome",
                CHROMIUM = "Chrome",
            }
        }) {
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
