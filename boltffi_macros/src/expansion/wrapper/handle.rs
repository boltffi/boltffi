use boltffi_binding::{native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::expansion::error::Error;

pub struct CarrierTokens {
    ty: TokenStream,
    zero: TokenStream,
}

impl CarrierTokens {
    pub fn native(carrier: native::HandleCarrier) -> Result<Self, Error> {
        match carrier {
            native::HandleCarrier::U64 => Ok(Self {
                ty: quote! { u64 },
                zero: quote! { 0 },
            }),
            native::HandleCarrier::USize => Ok(Self {
                ty: quote! { usize },
                zero: quote! { 0 },
            }),
            native::HandleCarrier::CallbackHandle => Ok(Self {
                ty: quote! { ::boltffi::__private::CallbackHandle },
                zero: quote! { ::boltffi::__private::CallbackHandle::NULL },
            }),
            _ => Err(Error::UnsupportedExpansion("unknown native handle carrier")),
        }
    }

    pub fn wasm32(carrier: wasm32::HandleCarrier) -> Result<Self, Error> {
        match carrier {
            wasm32::HandleCarrier::U32 => Ok(Self {
                ty: quote! { u32 },
                zero: quote! { 0 },
            }),
            _ => Err(Error::UnsupportedExpansion("unknown wasm handle carrier")),
        }
    }

    pub fn ty(&self) -> &TokenStream {
        &self.ty
    }

    pub fn zero(&self) -> &TokenStream {
        &self.zero
    }
}
