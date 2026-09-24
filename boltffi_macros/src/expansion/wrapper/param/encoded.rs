use boltffi_binding::{Native, Wasm32, WritePlan, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{encoded, names},
};

use super::Tokens;

pub struct Input<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    codec: &'lowered WritePlan,
    shape: S::BufferShape,
    target: rust_api::DecodeTarget,
    ident: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    writeback: bool,
    mutable_bytes: bool,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Input<'expansion, 'lowered, S> {
    pub fn new(
        codec: &'lowered WritePlan,
        shape: S::BufferShape,
        target: rust_api::DecodeTarget,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            codec,
            shape,
            target,
            ident,
            failure,
            expansion,
            writeback: false,
            mutable_bytes: false,
        }
    }

    pub fn with_writeback(mut self) -> Self {
        self.writeback = true;
        self
    }

    pub fn into_mutable_bytes(mut self) -> Self {
        self.mutable_bytes = true;
        self
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<Tokens, Error> {
        match self.shape {
            native::BufferShape::Slice => Slice::new(self).tokens(),
            native::BufferShape::Buffer | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native encoded parameter shape"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native encoded parameter shape",
            )),
        }
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<Tokens, Error> {
        match self.shape {
            wasm32::BufferShape::Slice => {
                let mut input = self;
                input.writeback = false;
                input.mutable_bytes = false;
                Slice::new(input).tokens()
            }
            wasm32::BufferShape::Packed => {
                Err(Error::UnsupportedExpansion("wasm encoded parameter shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm encoded parameter shape",
            )),
        }
    }
}

struct Slice<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    codec: &'lowered WritePlan,
    target: rust_api::DecodeTarget,
    ident: Ident,
    pointer: Ident,
    length: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    writeback: bool,
    mutable_bytes: bool,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> From<Input<'expansion, 'lowered, S>>
    for Slice<'expansion, 'lowered, S>
{
    fn from(input: Input<'expansion, 'lowered, S>) -> Self {
        Self::new(input)
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Slice<'expansion, 'lowered, S> {
    fn new(input: Input<'expansion, 'lowered, S>) -> Self {
        let ident = input.ident;
        let locals = names::Parameter::new(&ident);
        let pointer = locals.pointer();
        let length = locals.length();
        Self {
            codec: input.codec,
            target: input.target,
            ident,
            pointer,
            length,
            failure: input.failure,
            expansion: input.expansion,
            writeback: input.writeback,
            mutable_bytes: input.mutable_bytes,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        if self.mutable_bytes {
            return if self.target.is_slice_source() {
                self.mutable_byte_slice_tokens()
            } else {
                Err(Error::UnsupportedExpansion(
                    "`&mut Vec<u8>` parameters are not supported; \
                     use `&mut [u8]` for in-place mutation or return `Vec<u8>`",
                ))
            };
        }

        let pointer = &self.pointer;
        let length = &self.length;
        let ident = &self.ident;
        let pointer_type = self.pointer_type();
        let conversion = encoded::incoming::Value::new(self.codec.root(), self.expansion).decode(
            encoded::incoming::Input::new(&self.target, ident, pointer, length, &self.failure),
        )?;
        let tokens = Tokens {
            items: Vec::new(),
            ffi_parameters: [
                quote! { #pointer: #pointer_type },
                quote! { #length: usize },
            ]
            .into_iter()
            .collect(),
            ffi_parameter_types: [pointer_type, quote! { usize }].into_iter().collect(),
            owned_values: Vec::new(),
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        };
        if self.writeback {
            tokens.with_encoded_writeback(self.codec, ident, &self.failure, self.expansion)
        } else {
            Ok(tokens)
        }
    }

    fn mutable_byte_slice_tokens(self) -> Result<Tokens, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let ident = &self.ident;
        let failure = &self.failure;
        let conversion = quote! {
            let #ident: &mut [u8] = if #pointer.is_null() && #length > 0 {
                ::boltffi::__private::set_last_error_len(stringify!(#ident), "null pointer with non-zero length", #length as usize);
                #failure
            } else if #length == 0 {
                &mut []
            } else {
                unsafe { ::core::slice::from_raw_parts_mut(#pointer, #length) }
            };
        };
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *mut u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *mut u8 }, quote! { usize }],
            owned_values: Vec::new(),
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn pointer_type(&self) -> TokenStream {
        if self.mutable_bytes {
            quote! { *mut u8 }
        } else {
            quote! { *const u8 }
        }
    }
}

impl Tokens {
    pub fn with_encoded_writeback<S: boltffi_binding::SurfaceLower>(
        mut self,
        codec: &WritePlan,
        ident: &Ident,
        failure: &TokenStream,
        expansion: &Expansion<'_, S>,
    ) -> Result<Self, Error> {
        let out = names::Parameter::new(ident).writeback();
        let storage = names::Parameter::new(ident).storage();
        let buffer =
            encoded::outgoing::Value::new(codec.root(), expansion).buffer(quote! { #storage })?;
        self.ffi_parameters
            .push(quote! { #out: *mut ::boltffi::__private::FfiBuf });
        self.ffi_parameter_types
            .push(quote! { *mut ::boltffi::__private::FfiBuf });
        self.conversions.push(quote! {
            if #out.is_null() {
                ::boltffi::__private::set_last_error(concat!(stringify!(#out), ": writeback pointer is null"));
                #failure
            }
        });
        self.writebacks.push(quote! {
            unsafe {
                ::core::ptr::write(#out, #buffer);
            }
        });
        Ok(self)
    }
}
