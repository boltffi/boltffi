use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Visibility, braced};

struct PoolSpec {
    visibility: Visibility,
    name: Ident,
    entries: Vec<PoolEntry>,
}

struct PoolEntry {
    name: Ident,
    value: LitStr,
}

impl Parse for PoolSpec {
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
            entries.push(PoolEntry {
                name: entry_name,
                value,
            });
        }
        Ok(Self {
            visibility,
            name,
            entries,
        })
    }
}

pub fn interned_string_pool_impl(item: TokenStream) -> TokenStream {
    expand(item.into()).into()
}

fn expand(item: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let spec = match syn::parse2::<PoolSpec>(item) {
        Ok(spec) => spec,
        Err(error) => return error.into_compile_error(),
    };
    if let Err(error) = validate_unique_values(&spec) {
        return error.into_compile_error();
    }
    render(spec)
}

fn validate_unique_values(spec: &PoolSpec) -> syn::Result<()> {
    let mut seen = std::collections::HashMap::<String, &LitStr>::new();
    for entry in &spec.entries {
        let value = entry.value.value();
        if let Some(first) = seen.get(&value) {
            let mut error = syn::Error::new(
                entry.value.span(),
                format!("duplicate interned string pool value: {value:?}"),
            );
            error.combine(syn::Error::new(first.span(), "first defined here"));
            return Err(error);
        }
        seen.insert(value, &entry.value);
    }
    Ok(())
}

fn render(spec: PoolSpec) -> proc_macro2::TokenStream {
    let PoolSpec {
        visibility,
        name,
        entries,
    } = spec;
    let runtime = runtime_crate();
    let values = entries.iter().map(|entry| &entry.value).collect::<Vec<_>>();
    let constants = entries.iter().enumerate().map(|(index, entry)| {
        let constant = format_ident!(
            "{}",
            naming::to_snake_case(&entry.name.to_string()).to_uppercase()
        );
        let id = index as u32;
        quote! {
            pub const #constant: #runtime::InternedString<#name> = unsafe {
                #runtime::InternedString::from_id_unchecked(#id)
            };
        }
    });
    quote! {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        #visibility struct #name;

        impl #runtime::InternedStringPool for #name {
            const VALUES: &'static [&'static str] = &[#(#values),*];
        }

        impl #name {
            #(#constants)*
        }
    }
}

fn runtime_crate() -> proc_macro2::TokenStream {
    runtime_crate_named("boltffi")
        .or_else(|| runtime_crate_named("boltffi_core"))
        .unwrap_or_else(|| quote! { ::boltffi })
}

fn runtime_crate_named(name: &str) -> Option<proc_macro2::TokenStream> {
    match crate_name(name).ok()? {
        // `crate` resolves to rustdoc's generated doctest crate, not the
        // runtime crate. Both runtimes publish a canonical self alias so this
        // absolute path works internally, from doctests, and for consumers.
        FoundCrate::Itself => {
            let name = format_ident!("{name}");
            Some(quote! { ::#name })
        }
        FoundCrate::Name(name) => {
            let name = format_ident!("{name}");
            Some(quote! { ::#name })
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::expand;

    #[test]
    fn duplicate_string_values_expand_to_compile_error() {
        let expanded = expand(quote! {
            pub BrowserName {
                CHROME = "Chrome",
                CHROMIUM = "Chrome",
            }
        })
        .to_string();

        assert!(expanded.contains("compile_error"));
        assert!(expanded.contains("duplicate interned string pool value"));
        assert!(expanded.contains("Chrome"));
    }
}
