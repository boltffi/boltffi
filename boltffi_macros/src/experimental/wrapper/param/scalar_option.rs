use boltffi_binding::{Native, Primitive, Wasm32};
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    wrapper::{Render, names, scalar_option::WasmScalar},
};

use super::Tokens;

pub struct Renderer;

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
}

impl Render<Native, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let ident = &input.ident;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let rust_type = &input.rust_type;
        let failure = input.failure;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversions: vec![quote! {
                let #ident: #rust_type = if #pointer.is_null() {
                    None
                } else {
                    match ::boltffi::__private::wire::decode::<#rust_type>(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: invalid optional scalar payload: {} (buf_len={})",
                                stringify!(#ident),
                                error,
                                #length
                            ));
                            #failure
                        }
                    }
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

impl Render<Wasm32, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let ident = &input.ident;
        let rust_type = &input.rust_type;
        let scalar = WasmScalar::new(input.primitive, ident.clone());
        let ffi_type = scalar.carrier_type();
        let is_none = scalar.is_none();
        let value = scalar.incoming()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
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
