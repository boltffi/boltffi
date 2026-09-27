use boltffi_binding::TypeRef;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

pub fn classify_callback_error_payload(
    error_type: &Type,
    bytes: TokenStream,
    declared_error: TokenStream,
) -> TokenStream {
    quote! {
        match ::boltffi::__private::UnexpectedFfiCallbackError::classify_payload(#bytes) {
            ::boltffi::__private::UnexpectedFfiCallbackPayload::NotUnexpected => {
                #declared_error
            }
            ::boltffi::__private::UnexpectedFfiCallbackPayload::Unexpected(error)
            | ::boltffi::__private::UnexpectedFfiCallbackPayload::Malformed(error) => {
                <#error_type as ::core::convert::From<
                    ::boltffi::__private::UnexpectedFfiCallbackError
                >>::from(error)
            }
        }
    }
}

pub fn classified_callback_error_value(
    error_ty: &TypeRef,
    error_type: &Type,
    bytes: TokenStream,
    declared_error: TokenStream,
) -> TokenStream {
    match error_ty {
        TypeRef::Record(_) | TypeRef::Enum(_) | TypeRef::String => {
            classify_callback_error_payload(error_type, bytes, declared_error)
        }
        _ => declared_error,
    }
}
