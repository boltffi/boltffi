use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parse;

pub(crate) struct CustomTypeSpec {
    pub(crate) vis: syn::Visibility,
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

impl CustomTypeSpec {
    /// Whether the declared name is the remote type's own name, which needs no alias.
    pub(crate) fn names_remote(&self) -> bool {
        names_remote(&self.name, &self.remote)
    }
}

fn names_remote(name: &syn::Ident, remote: &syn::Type) -> bool {
    matches!(
        remote,
        syn::Type::Path(path) if path.qself.is_none()
            && path.path.segments.last().is_some_and(|segment| {
                segment.ident == *name && segment.arguments.is_empty()
            })
    )
}

pub(crate) fn parse_spec(item: TokenStream) -> syn::Result<CustomTypeSpec> {
    syn::parse(item)
}

impl Parse for CustomTypeSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let vis: syn::Visibility = input.parse()?;
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
            vis,
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
            vis,
            name,
            remote,
            repr,
            error,
            into_ffi,
            try_from_ffi,
        } = self.spec;

        let snake = boltffi_ast::CanonicalName::from(name.to_string().as_str()).to_string();
        let alias = (!names_remote(&name, &remote)).then(|| quote! { #vis type #name = #remote; });
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

            #alias

            impl ::boltffi::__private::CustomType<crate::__BoltffiTag> for #remote {
                type Repr = #repr;
                type Error = #error;

                fn into_ffi(value: &Self) -> Self::Repr {
                    #into_fn_name(value)
                }

                fn try_from_ffi(repr: Self::Repr) -> ::core::result::Result<Self, Self::Error> {
                    #try_from_fn_name(repr)
                }
            }
        }
    }
}
