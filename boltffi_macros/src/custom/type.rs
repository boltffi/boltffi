use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parse;

pub(crate) struct CustomTypeSpec {
    pub(crate) name: syn::Ident,
    pub(crate) remote: syn::Type,
    repr: syn::Type,
    error: syn::Type,
    into_ffi: syn::Expr,
    try_from_ffi: syn::Expr,
}

struct CustomTypeExpansion {
    spec: CustomTypeSpec,
}

pub(crate) fn parse_spec(item: TokenStream) -> syn::Result<CustomTypeSpec> {
    syn::parse(item)
}

impl Parse for CustomTypeSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: syn::Visibility = input.parse()?;
        let name: syn::Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;

        let mut remote: Option<syn::Type> = None;
        let mut repr: Option<syn::Type> = None;
        let mut error: Option<syn::Type> = None;
        let mut into_ffi: Option<syn::Expr> = None;
        let mut try_from_ffi: Option<syn::Expr> = None;

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            match key.to_string().as_str() {
                "remote" => {
                    remote = Some(input.parse()?);
                }
                "repr" => {
                    repr = Some(input.parse()?);
                }
                "error" => {
                    error = Some(input.parse()?);
                }
                "into_ffi" => {
                    into_ffi = Some(input.parse()?);
                }
                "try_from_ffi" => {
                    try_from_ffi = Some(input.parse()?);
                }
                _ => {
                    let _: syn::Expr = input.parse()?;
                }
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            }
        }

        let remote = remote.ok_or_else(|| input.error("custom_type!: missing `remote = ...`"))?;
        let repr = repr.ok_or_else(|| input.error("custom_type!: missing `repr = ...`"))?;
        let error =
            error.unwrap_or_else(|| syn::parse_quote!(::boltffi::CustomTypeConversionError));
        let into_ffi =
            into_ffi.ok_or_else(|| input.error("custom_type!: missing `into_ffi = ...`"))?;
        let try_from_ffi = try_from_ffi
            .ok_or_else(|| input.error("custom_type!: missing `try_from_ffi = ...`"))?;

        Ok(Self {
            name,
            remote,
            repr,
            error,
            into_ffi,
            try_from_ffi,
        })
    }
}

pub fn custom_type_impl(item: TokenStream) -> TokenStream {
    let spec = syn::parse_macro_input!(item as CustomTypeSpec);
    CustomTypeExpansion::new(spec).render().into()
}

impl CustomTypeExpansion {
    fn new(spec: CustomTypeSpec) -> Self {
        Self { spec }
    }

    fn render(self) -> proc_macro2::TokenStream {
        let CustomTypeSpec {
            name,
            remote,
            repr,
            error,
            into_ffi,
            try_from_ffi,
        } = self.spec;

        let snake = naming::to_snake_case(&name.to_string());
        let into_fn_name = format_ident!("__boltffi_custom_type_{}_into_ffi", snake);
        let try_from_fn_name = format_ident!("__boltffi_custom_type_{}_try_from_ffi", snake);

        quote! {
            #[doc(hidden)]
            pub(crate) fn #into_fn_name(value: &#remote) -> #repr {
                (#into_ffi)(value)
            }

            #[doc(hidden)]
            pub(crate) fn #try_from_fn_name(value: #repr) -> ::core::result::Result<#remote, #error> {
                (#try_from_ffi)(value)
            }
        }
    }
}
