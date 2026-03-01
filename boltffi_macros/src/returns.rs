use crate::custom_types;
pub use boltffi_ffi_rules::transport::EncodedReturnStrategy;
use proc_macro2::Span;
use quote::quote;
use syn::{ReturnType, Type};

pub enum OptionReturnAbi {
    OutValue { inner: syn::Type },
    OutFfiString,
    Vec { inner: syn::Type },
}

#[allow(clippy::large_enum_variant)]
pub enum ReturnKind {
    Unit,
    Primitive(syn::Type),
    String,
    ResultPrimitive { ok: syn::Type, err: syn::Type },
    ResultString { err: syn::Type },
    ResultUnit { err: syn::Type },
    Vec(syn::Type),
    Option(OptionReturnAbi),
    WireEncoded(syn::Type),
}

pub enum ReturnAbi {
    Unit,
    Scalar { rust_type: syn::Type },
    Encoded { rust_type: syn::Type, strategy: EncodedReturnStrategy },
}

pub fn extract_vec_inner(ty: &Type) -> Option<syn::Type> {
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && segment.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }
    None
}

fn is_string_like_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "String"),
        Type::Reference(reference) => match reference.elem.as_ref() {
            Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "str"),
            _ => false,
        },
        _ => false,
    }
}

pub fn is_primitive_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "usize" | "isize" | "()"
    )
}

pub fn classify_return(output: &ReturnType) -> ReturnKind {
    match output {
        ReturnType::Default => ReturnKind::Unit,
        ReturnType::Type(_, ty) => {
            let type_str = quote!(#ty).to_string().replace(' ', "");

            if type_str == "String" || type_str == "std::string::String" {
                return ReturnKind::String;
            }

            if let Some(inner) = extract_vec_inner(ty) {
                return ReturnKind::Vec(inner);
            }

            if let Type::Path(path) = ty.as_ref()
                && let Some(segment) = path.path.segments.last()
            {
                if segment.ident == "Result"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && args.args.len() >= 2
                    && let Some(syn::GenericArgument::Type(ok_ty)) = args.args.first()
                    && let Some(syn::GenericArgument::Type(err_ty)) = args.args.iter().nth(1)
                {
                    let ok_str = quote!(#ok_ty).to_string().replace(' ', "");
                    if ok_str == "String" || ok_str == "std::string::String" {
                        return ReturnKind::ResultString { err: err_ty.clone() };
                    } else if ok_str == "()" {
                        return ReturnKind::ResultUnit { err: err_ty.clone() };
                    } else {
                        return ReturnKind::ResultPrimitive {
                            ok: ok_ty.clone(),
                            err: err_ty.clone(),
                        };
                    }
                }
                if segment.ident == "Option"
                    && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
                    && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
                {
                    if let Some(vec_inner) = extract_vec_inner(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::Vec { inner: vec_inner });
                    }

                    if is_string_like_type(inner_ty) {
                        return ReturnKind::Option(OptionReturnAbi::OutFfiString);
                    }

                    return ReturnKind::Option(OptionReturnAbi::OutValue { inner: inner_ty.clone() });
                }
            }

            if is_primitive_type(&type_str) {
                ReturnKind::Primitive(ty.as_ref().clone())
            } else {
                ReturnKind::WireEncoded(ty.as_ref().clone())
            }
        }
    }
}

fn option_rust_type(abi: OptionReturnAbi) -> syn::Type {
    match abi {
        OptionReturnAbi::OutValue { inner } => syn::parse_quote!(Option<#inner>),
        OptionReturnAbi::OutFfiString => syn::parse_quote!(Option<String>),
        OptionReturnAbi::Vec { inner } => syn::parse_quote!(Option<Vec<#inner>>),
    }
}

fn result_rust_type(ok: syn::Type, err: syn::Type) -> syn::Type {
    syn::parse_quote!(Result<#ok, #err>)
}

fn type_is_primitive(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    is_primitive_type(&type_str)
}

pub fn lower_return_abi(kind: ReturnKind) -> ReturnAbi {
    match kind {
        ReturnKind::Unit => ReturnAbi::Unit,
        ReturnKind::Primitive(rust_type) => ReturnAbi::Scalar { rust_type },
        ReturnKind::String => ReturnAbi::Encoded {
            rust_type: syn::parse_quote!(String),
            strategy: EncodedReturnStrategy::Utf8String,
        },
        ReturnKind::Vec(inner) => ReturnAbi::Encoded {
            rust_type: syn::parse_quote!(Vec<#inner>),
            strategy: if type_is_primitive(&inner) {
                EncodedReturnStrategy::PrimitiveVec
            } else {
                EncodedReturnStrategy::WireEncoded
            },
        },
        ReturnKind::Option(abi) => match abi {
            OptionReturnAbi::OutValue { inner } if type_is_primitive(&inner) => ReturnAbi::Encoded {
                rust_type: syn::parse_quote!(Option<#inner>),
                strategy: EncodedReturnStrategy::OptionScalar,
            },
            other => ReturnAbi::Encoded {
                rust_type: option_rust_type(other),
                strategy: EncodedReturnStrategy::WireEncoded,
            },
        },
        ReturnKind::ResultString { err } => ReturnAbi::Encoded {
            rust_type: result_rust_type(syn::parse_quote!(String), err),
            strategy: EncodedReturnStrategy::WireEncoded,
        },
        ReturnKind::ResultPrimitive { ok, err } => {
            if type_is_primitive(&ok) && type_is_primitive(&err) {
                ReturnAbi::Encoded {
                    rust_type: result_rust_type(ok.clone(), err.clone()),
                    strategy: EncodedReturnStrategy::ResultScalar,
                }
            } else {
                ReturnAbi::Encoded {
                    rust_type: result_rust_type(ok, err),
                    strategy: EncodedReturnStrategy::WireEncoded,
                }
            }
        }
        ReturnKind::ResultUnit { err } => ReturnAbi::Encoded {
            rust_type: result_rust_type(syn::parse_quote!(()), err),
            strategy: EncodedReturnStrategy::WireEncoded,
        },
        ReturnKind::WireEncoded(rust_type) => ReturnAbi::Encoded {
            rust_type,
            strategy: EncodedReturnStrategy::WireEncoded,
        },
    }
}

impl ReturnAbi {
    pub fn from_output(output: &ReturnType) -> Self {
        lower_return_abi(classify_return(output))
    }

    pub fn async_ffi_return_type(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Unit => quote! { () },
            Self::Scalar { rust_type } => quote! { #rust_type },
            Self::Encoded { .. } => quote! { ::boltffi::__private::FfiBuf<u8> },
        }
    }

    pub fn async_rust_return_type(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Unit => quote! { () },
            Self::Scalar { rust_type } | Self::Encoded { rust_type, .. } => {
                quote! { #rust_type }
            }
        }
    }

    pub fn async_complete_conversion(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Unit => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                ()
            },
            Self::Scalar { .. } => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                result
            },
            Self::Encoded { rust_type, strategy } => {
                let registry = custom_types::registry_for_current_crate().ok();
                let result_ident = syn::Ident::new("result", Span::call_site());
                let encode_expression = encoded_return_buffer_expression(rust_type, *strategy, &result_ident, registry.as_ref());
                quote! {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                    #encode_expression
                }
            }
        }
    }

    pub fn async_default_ffi_value(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Unit => quote! { () },
            Self::Scalar { .. } => quote! { Default::default() },
            Self::Encoded { .. } => quote! { ::boltffi::__private::FfiBuf::default() },
        }
    }
}

pub fn encoded_return_body(
    rust_type: &syn::Type,
    strategy: EncodedReturnStrategy,
    result_ident: &syn::Ident,
    evaluate_result_expression: proc_macro2::TokenStream,
    conversions: &[proc_macro2::TokenStream],
    custom_type_registry: &custom_types::CustomTypeRegistry,
) -> proc_macro2::TokenStream {
    let encode_expression = encoded_return_buffer_expression(rust_type, strategy, result_ident, Some(custom_type_registry));

    quote! {
        #(#conversions)*
        let #result_ident: #rust_type = #evaluate_result_expression;
        #encode_expression
    }
}

fn encoded_return_buffer_expression(
    rust_type: &syn::Type,
    strategy: EncodedReturnStrategy,
    result_ident: &syn::Ident,
    custom_type_registry: Option<&custom_types::CustomTypeRegistry>,
) -> proc_macro2::TokenStream {
    match strategy {
        EncodedReturnStrategy::PrimitiveVec => quote! {
            ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
        },
        EncodedReturnStrategy::Utf8String => quote! {
            #[cfg(target_arch = "wasm32")]
            {
                ::boltffi::__private::FfiBuf::from_vec(#result_ident.into_bytes())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            }
        },
        EncodedReturnStrategy::OptionScalar | EncodedReturnStrategy::ResultScalar => quote! {
            ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
        },
        EncodedReturnStrategy::WireEncoded => wire_encode_expression(rust_type, result_ident, custom_type_registry),
    }
}

fn wire_encode_expression(rust_type: &syn::Type, result_ident: &syn::Ident, custom_type_registry: Option<&custom_types::CustomTypeRegistry>) -> proc_macro2::TokenStream {
    match custom_type_registry {
        Some(registry) if custom_types::contains_custom_types(rust_type, registry) => {
            let wire_ty = custom_types::wire_type_for(rust_type, registry);
            let wire_value_ident = syn::Ident::new("__boltffi_wire_value", result_ident.span());
            let to_wire = custom_types::to_wire_expr_owned(rust_type, registry, result_ident);
            quote! {
                let #wire_value_ident: #wire_ty = { #to_wire };
                ::boltffi::__private::FfiBuf::wire_encode(&#wire_value_ident)
            }
        }
        _ => quote! {
            ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
        },
    }
}
