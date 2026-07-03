use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
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
    let spec = syn::parse_macro_input!(item as PoolSpec);
    render(spec).into()
}

fn render(spec: PoolSpec) -> proc_macro2::TokenStream {
    let PoolSpec {
        visibility,
        name,
        entries,
    } = spec;
    let values = entries.iter().map(|entry| &entry.value).collect::<Vec<_>>();
    let constants = entries.iter().enumerate().map(|(index, entry)| {
        let constant = format_ident!(
            "{}",
            naming::to_snake_case(&entry.name.to_string()).to_uppercase()
        );
        let id = index as u32;
        quote! {
            pub const #constant: ::boltffi::InternedString<#name> = unsafe {
                ::boltffi::InternedString::from_id_unchecked(#id)
            };
        }
    });
    quote! {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        #visibility struct #name;

        impl ::boltffi::InternedStringPool for #name {
            const VALUES: &'static [&'static str] = &[#(#values),*];
        }

        impl #name {
            #(#constants)*
        }
    }
}
