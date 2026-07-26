use boltffi_ast::TypeExpr;
use boltffi_binding::CodecNode;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error, expansion::Expansion, rust_api, surface::RenderSurface, wrapper::names,
};

pub struct Value<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct Input {
    target: rust_api::DecodeTarget,
    binding: Ident,
    pointer: Ident,
    length: Ident,
    failure: TokenStream,
}

pub struct Bytes {
    rust_type: Type,
    source: TypeExpr,
    bytes: TokenStream,
    failure: TokenStream,
}

impl<'expansion, 'lowered, S: RenderSurface> Value<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn decode(&self, input: Input) -> Result<TokenStream, Error> {
        super::require_runtime_wire(self.codec)?;
        input.target.incoming_encoded_type().require_supported()?;
        if matches!(self.codec, CodecNode::Utf8String) {
            return input.utf8_string();
        }
        match input.target.borrow() {
            rust_api::DecodeBorrow::Owned => input.owned(
                self.codec,
                input.target.owned(),
                &input.binding,
                false,
                self.expansion,
            ),
            borrow => input.reference(self.codec, borrow, self.expansion),
        }
    }

    pub fn expression(&self, bytes: Bytes) -> Result<TokenStream, Error> {
        super::require_runtime_wire(self.codec)?;
        rust_api::IncomingEncodedType::new(&bytes.source).require_supported()?;
        let incoming = super::custom::Incoming::new(self.codec, self.expansion);
        let converted = incoming.convert(quote! { __boltffi_decoded })?;
        let rust_type = &bytes.rust_type;
        let decode_type = incoming
            .decoded_type()?
            .unwrap_or_else(|| quote! { #rust_type });
        let bytes_expr = bytes.bytes;
        let failure = bytes.failure;
        let decoded_value = if converted.changed() {
            let converted_value = converted.tokens();
            match converted.fallible() {
                true => quote! {
                    match #converted_value {
                        Ok(value) => value,
                        Err(error) => {
                            #failure
                        }
                    }
                },
                false => quote! { #converted_value },
            }
        } else {
            quote! { __boltffi_decoded }
        };
        Ok(quote! {
            {
                let __boltffi_decoded =
                    match ::boltffi::__private::wire::decode::<#decode_type>(#bytes_expr) {
                        Ok(value) => value,
                        Err(error) => {
                            #failure
                        }
                    };
                #decoded_value
            }
        })
    }

    pub fn packed_expression(
        &self,
        rust_type: &Type,
        source: &TypeExpr,
        packed: TokenStream,
        failure: TokenStream,
    ) -> Result<TokenStream, Error> {
        let value = self.expression(Bytes::new(
            rust_type,
            source,
            quote! { __boltffi_packed_bytes.as_slice() },
            failure,
        ))?;
        Ok(quote! {
            {
                let __boltffi_packed_bytes = unsafe {
                    ::boltffi::__private::take_packed_bytes(#packed)
                };
                #value
            }
        })
    }
}

impl Bytes {
    pub fn new(
        rust_type: &Type,
        source: &TypeExpr,
        bytes: TokenStream,
        failure: TokenStream,
    ) -> Self {
        Self {
            rust_type: rust_type.clone(),
            source: source.clone(),
            bytes,
            failure,
        }
    }
}

impl Input {
    pub fn new(
        target: &rust_api::DecodeTarget,
        binding: &Ident,
        pointer: &Ident,
        length: &Ident,
        failure: &TokenStream,
    ) -> Self {
        Self {
            target: target.clone(),
            binding: binding.clone(),
            pointer: pointer.clone(),
            length: length.clone(),
            failure: failure.clone(),
        }
    }

    fn utf8_string(&self) -> Result<TokenStream, Error> {
        let rust_type = self.target.owned();
        match self.target.borrow() {
            rust_api::DecodeBorrow::Owned => {
                self.owned_utf8_string(rust_type, &self.binding, false)
            }
            borrow => {
                let storage = names::Parameter::new(&self.binding).storage();
                let owned = self.owned_utf8_string(rust_type, &storage, borrow.mutable())?;
                let binding = &self.binding;
                let borrowed = self.borrow(&storage, borrow)?;
                Ok(quote! {
                    #owned
                    let #binding = #borrowed;
                })
            }
        }
    }

    fn owned_utf8_string(
        &self,
        rust_type: &Type,
        binding: &Ident,
        mutable: bool,
    ) -> Result<TokenStream, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let failure = &self.failure;
        let mutability = mutable.then(|| quote! { mut });
        Ok(quote! {
            let #mutability #binding: #rust_type = {
                if #pointer.is_null() && #length > 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null pointer with non-zero length (buf_len={})",
                        stringify!(#binding),
                        #length
                    ));
                    #failure
                }
                let __boltffi_packed =
                    (#pointer as u32 as u64) | ((#length as u32 as u64) << 32);
                unsafe { ::boltffi::__private::take_packed_utf8_string(__boltffi_packed) }
            };
        })
    }

    fn reference<'lowered, S: RenderSurface>(
        &self,
        codec: &CodecNode,
        borrow: rust_api::DecodeBorrow,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        let storage = names::Parameter::new(&self.binding).storage();
        let owned = self.owned(
            codec,
            self.target.owned(),
            &storage,
            borrow.mutable(),
            expansion,
        )?;
        let binding = &self.binding;
        let borrow = self.borrow(&storage, borrow)?;
        Ok(quote! {
            #owned
            let #binding = #borrow;
        })
    }

    fn owned<'lowered, S: RenderSurface>(
        &self,
        codec: &CodecNode,
        rust_type: &Type,
        binding: &Ident,
        mutable: bool,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let failure = &self.failure;
        let mutability = mutable.then(|| quote! { mut });
        let incoming = super::custom::Incoming::new(codec, expansion);
        let converted = incoming.convert(quote! { __boltffi_decoded })?;
        if !converted.changed() {
            return Ok(quote! {
                let #mutability #binding: #rust_type = {
                    if #pointer.is_null() && #length > 0 {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: null pointer with non-zero length (buf_len={})",
                            stringify!(#binding),
                            #length
                        ));
                        #failure
                    }
                    let __boltffi_bytes: &[u8] = if #length == 0 {
                        &[]
                    } else {
                        unsafe { ::core::slice::from_raw_parts(#pointer, #length) }
                    };
                    match ::boltffi::__private::wire::decode::<#rust_type>(__boltffi_bytes) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: wire decode failed: {} (buf_len={})",
                                stringify!(#binding),
                                error,
                                #length
                            ));
                            #failure
                        }
                    }
                };
            });
        }
        let decoded_value = if converted.fallible() {
            let converted_value = converted.tokens();
            quote! {
                match #converted_value {
                    Ok(value) => value,
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: custom conversion failed: {:?} (buf_len={})",
                            stringify!(#binding),
                            error,
                            #length
                        ));
                        #failure
                    }
                }
            }
        } else {
            let converted_value = converted.tokens();
            quote! { #converted_value }
        };
        let decode_type = incoming.decoded_type()?.ok_or(Error::UnsupportedExpansion(
            "custom codec representation type",
        ))?;
        let type_annotation = (!converted.changed()).then(|| quote! { : #rust_type });
        Ok(quote! {
            let #mutability #binding #type_annotation = {
                if #pointer.is_null() && #length > 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null pointer with non-zero length (buf_len={})",
                        stringify!(#binding),
                        #length
                    ));
                    #failure
                }
                let __boltffi_bytes: &[u8] = if #length == 0 {
                    &[]
                } else {
                    unsafe { ::core::slice::from_raw_parts(#pointer, #length) }
                };
                let __boltffi_decoded = match ::boltffi::__private::wire::decode::<#decode_type>(__boltffi_bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: wire decode failed: {} (buf_len={})",
                            stringify!(#binding),
                            error,
                            #length
                        ));
                        #failure
                    }
                };
                #decoded_value
            };
        })
    }

    fn borrow(
        &self,
        storage: &Ident,
        borrow: rust_api::DecodeBorrow,
    ) -> Result<TokenStream, Error> {
        match borrow {
            rust_api::DecodeBorrow::Owned => Err(Error::SourceSyntaxMismatch(
                "owned encoded parameter does not borrow decoded storage",
            )),
            rust_api::DecodeBorrow::Value { mutable: false } => Ok(quote! { &#storage }),
            rust_api::DecodeBorrow::Value { mutable: true } => Ok(quote! { &mut #storage }),
            rust_api::DecodeBorrow::Slice { mutable: false } => Ok(quote! { #storage.as_slice() }),
            rust_api::DecodeBorrow::Slice { mutable: true } => {
                Ok(quote! { #storage.as_mut_slice() })
            }
            rust_api::DecodeBorrow::Str { mutable: false } => Ok(quote! { #storage.as_str() }),
            rust_api::DecodeBorrow::Str { mutable: true } => Ok(quote! { #storage.as_mut_str() }),
        }
    }
}
