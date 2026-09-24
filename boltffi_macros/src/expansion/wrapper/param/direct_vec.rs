use boltffi_binding::{DirectVectorElementType, Primitive, Receive};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    error::Error,
    wrapper::{self, names},
};

use super::Tokens;

pub struct Input {
    element: DirectVectorElementType,
    receive: Receive,
    rust_element: Type,
    ident: Ident,
    failure: TokenStream,
}

impl Input {
    pub fn new(
        element: &DirectVectorElementType,
        receive: Receive,
        rust_element: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            element: element.clone(),
            receive,
            rust_element,
            ident,
            failure,
        }
    }

    pub fn render(self) -> Result<Tokens, Error> {
        match &self.element {
            DirectVectorElementType::Primitive(primitive) => {
                PrimitiveVec::new(primitive.primitive(), self.receive, self.ident).tokens()
            }
            DirectVectorElementType::Record(_) => {
                PassableVec::new(self.receive, self.rust_element, self.ident, self.failure).tokens()
            }
            _ => Err(Error::UnsupportedExpansion("direct-vector element")),
        }
    }
}

struct PrimitiveVec {
    primitive: Primitive,
    receive: Receive,
    ident: Ident,
}

impl PrimitiveVec {
    fn new(primitive: Primitive, receive: Receive, ident: Ident) -> Self {
        Self {
            primitive,
            receive,
            ident,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let element_type = wrapper::type_ref::primitive(self.primitive)?;
        match self.receive {
            Receive::ByValue => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: vec![
                    quote! { #pointer: *const #element_type },
                    quote! { #length: usize },
                ],
                ffi_parameter_types: vec![quote! { *const #element_type }, quote! { usize }],
                owned_values: Vec::new(),
                conversions: vec![quote! {
                    let #ident: Vec<#element_type> = if #pointer.is_null() {
                        Vec::new()
                    } else {
                        unsafe { ::core::slice::from_raw_parts(#pointer, #length) }.to_vec()
                    };
                }],
                writebacks: Vec::new(),
                argument: quote! { #ident },
            }),
            Receive::ByRef => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: vec![
                    quote! { #pointer: *const #element_type },
                    quote! { #length: usize },
                ],
                ffi_parameter_types: vec![quote! { *const #element_type }, quote! { usize }],
                owned_values: Vec::new(),
                conversions: vec![quote! {
                    let #ident: &[#element_type] = if #pointer.is_null() {
                        &[]
                    } else {
                        unsafe { ::core::slice::from_raw_parts(#pointer, #length) }
                    };
                }],
                writebacks: Vec::new(),
                argument: quote! { #ident },
            }),
            Receive::ByMutRef => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: vec![
                    quote! { #pointer: *mut #element_type },
                    quote! { #length: usize },
                ],
                ffi_parameter_types: vec![quote! { *mut #element_type }, quote! { usize }],
                owned_values: Vec::new(),
                conversions: vec![quote! {
                    let #ident: &mut [#element_type] = if #pointer.is_null() {
                        &mut []
                    } else {
                        unsafe { ::core::slice::from_raw_parts_mut(#pointer, #length) }
                    };
                }],
                writebacks: Vec::new(),
                argument: quote! { #ident },
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct-vector receive mode",
            )),
        }
    }
}

struct PassableVec {
    receive: Receive,
    element: Type,
    ident: Ident,
    failure: TokenStream,
}

impl PassableVec {
    fn new(receive: Receive, element: Type, ident: Ident, failure: TokenStream) -> Self {
        Self {
            receive,
            element,
            ident,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        if self.receive != Receive::ByValue {
            return Err(Error::UnsupportedExpansion(
                "borrowed direct-record vector parameter",
            ));
        }
        let element = &self.element;
        let ident = &self.ident;
        let failure = self.failure;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            owned_values: Vec::new(),
            conversions: vec![quote! {
                let #ident: Vec<#element> = if #pointer.is_null() {
                    Vec::new()
                } else {
                    let raw_byte_len = #length;
                    let element_size = ::core::mem::size_of::<<#element as ::boltffi::__private::Passable>::In>();
                    if raw_byte_len % element_size == 0 {
                        unsafe {
                            <#element as ::boltffi::__private::VecTransport>::unpack_vec(
                                #pointer,
                                raw_byte_len
                            )
                        }
                    } else {
                        ::boltffi::__private::set_last_error(format!(
                            "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                            raw_byte_len,
                            ::core::any::type_name::<#element>(),
                            element_size
                        ));
                        #failure
                    }
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}
