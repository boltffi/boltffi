use boltffi_binding::Primitive;
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    error::Error,
    wrapper::{names, scalar_option::WasmScalar},
};

use super::Tokens;

pub struct Input {
    primitive: Primitive,
    rust_type: Type,
    ident: Ident,
    failure: proc_macro2::TokenStream,
}

impl Input {
    pub fn new(
        primitive: Primitive,
        rust_type: Type,
        ident: Ident,
        failure: proc_macro2::TokenStream,
    ) -> Self {
        Self {
            primitive,
            rust_type,
            ident,
            failure,
        }
    }

    pub fn native(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let rust_type = &self.rust_type;
        let failure = self.failure;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            owned_values: Vec::new(),
            conversions: vec![quote! {
                let #ident: #rust_type = if #pointer.is_null() {
                    None
                } else {
                    match ::boltffi::__private::wire::decode::<#rust_type>(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error_display(stringify!(#ident), "invalid optional scalar payload", &error, #length as usize);
                            #failure
                        }
                    }
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    pub fn wasm32(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        let scalar = WasmScalar::new(self.primitive, ident.clone());
        let ffi_type = scalar.carrier_type();
        let is_none = scalar.is_none();
        let value = scalar.incoming()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            owned_values: Vec::new(),
            conversions: vec![quote! {
                let #ident: #rust_type = if #is_none {
                    None
                } else {
                    Some(#value)
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}
