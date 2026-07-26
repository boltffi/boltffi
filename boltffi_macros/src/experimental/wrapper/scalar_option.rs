use boltffi_binding::Primitive;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::error::Error;

pub struct WasmScalar {
    primitive: Primitive,
    value: Ident,
}

impl WasmScalar {
    pub fn new(primitive: Primitive, value: Ident) -> Self {
        Self { primitive, value }
    }

    pub fn incoming(self) -> Result<TokenStream, Error> {
        let value = self.value;
        Ok(match self.primitive {
            Primitive::Bool => quote! { #value != 0.0 },
            Primitive::F64 => quote! { f64::from_bits(#value) },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32 => quote! { #value as _ },
            _ => return Err(Error::UnsupportedExpansion("scalar option primitive")),
        })
    }

    pub fn outgoing(self) -> Result<TokenStream, Error> {
        let value = self.value;
        Ok(match self.primitive {
            Primitive::Bool => quote! {
                if #value { 1.0 } else { 0.0 }
            },
            Primitive::F64 => quote! {
                if #value.is_nan() {
                    f64::NAN.to_bits()
                } else {
                    #value.to_bits()
                }
            },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32 => quote! { #value as f64 },
            _ => return Err(Error::UnsupportedExpansion("scalar option primitive")),
        })
    }

    pub fn carrier_type(&self) -> TokenStream {
        match self.primitive {
            Primitive::F64 => quote! { u64 },
            _ => quote! { f64 },
        }
    }

    pub fn none(&self) -> TokenStream {
        match self.primitive {
            Primitive::F64 => quote! { u64::MAX },
            _ => quote! { f64::NAN },
        }
    }

    pub fn is_none(&self) -> TokenStream {
        let value = &self.value;
        match self.primitive {
            Primitive::F64 => quote! { #value == u64::MAX },
            _ => quote! { #value.is_nan() },
        }
    }
}
