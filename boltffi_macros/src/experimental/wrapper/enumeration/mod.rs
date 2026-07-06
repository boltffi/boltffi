use boltffi_ast::{EnumDef, FieldDef, TypeExpr, VariantDef, VariantPayload};
use boltffi_binding::{
    CStyleEnumDecl, CanonicalName, CodecNode, DataEnumDecl, DataVariantDecl, DataVariantPayload,
    DirectValueType, EncodedFieldDecl, EnumDecl, FieldKey, IntegerRepr,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, names},
};

mod exports;

pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, EnumDef, EnumDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct CStyle<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered EnumDef,
    binding: &'lowered CStyleEnumDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Data<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered EnumDef,
    binding: &'lowered DataEnumDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Renderer<'expansion, 'lowered, S> {
    pub fn new(
        pair: DeclarationPair<'lowered, EnumDef, EnumDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { pair, expansion }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::param::direct::Renderer:
            Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        match self.pair.binding() {
            EnumDecl::CStyle(binding) => CStyle {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .render(),
            EnumDecl::Data(binding) => Data {
                source: self.pair.source(),
                binding: binding.as_ref(),
                expansion: self.expansion,
            }
            .render(),
            _ => Err(Error::UnsupportedExpansion("unknown enum declaration")),
        }
    }
}

impl<'expansion, 'lowered, S> CStyle<'expansion, 'lowered, S>
where
    S: RenderSurface,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Renderer:
        Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let enumeration = names::SourceSpelling::new(&self.source.name)
            .ident("source enum name is not a Rust identifier")?;
        let repr = self.repr_type()?;
        let variants = self.variants()?;
        let discriminant_arms = variants
            .iter()
            .map(|variant| variant.discriminant_arm(&enumeration, &repr))
            .collect::<Result<Vec<_>, _>>()?;
        let decode_arms = variants
            .iter()
            .map(|variant| variant.decode_arm(&enumeration, &repr))
            .collect::<Result<Vec<_>, _>>()?;
        let repr_arms = variants
            .iter()
            .map(|variant| variant.repr_arm(&enumeration, &repr))
            .collect::<Result<Vec<_>, _>>()?;
        let exports = exports::Renderer::new(
            self.source,
            enumeration.clone(),
            exports::Receiver::Direct {
                ty: DirectValueType::enumeration(self.binding.id()),
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()?;

        Ok(quote! {
            unsafe impl ::boltffi::__private::Passable for #enumeration {
                type In = #repr;
                type Out = #repr;

                unsafe fn unpack(input: #repr) -> Self {
                    match input {
                        #(#discriminant_arms,)*
                        _ => panic!("invalid enum discriminant for {}", stringify!(#enumeration)),
                    }
                }

                fn pack(self) -> #repr {
                    self as #repr
                }
            }

            impl ::boltffi::__private::wire::WireEncode for #enumeration {
                fn is_fixed_size() -> bool {
                    <#repr as ::boltffi::__private::wire::WireEncode>::is_fixed_size()
                }

                fn fixed_size() -> Option<usize> {
                    <#repr as ::boltffi::__private::wire::WireEncode>::fixed_size()
                }

                fn wire_size(&self) -> usize {
                    let value: #repr = match self {
                        #(#repr_arms,)*
                    };
                    <#repr as ::boltffi::__private::wire::WireEncode>::wire_size(&value)
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    let value: #repr = match self {
                        #(#repr_arms,)*
                    };
                    <#repr as ::boltffi::__private::wire::WireEncode>::encode_to(
                        &value,
                        buffer
                    )
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #enumeration {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let (value, used) =
                        <#repr as ::boltffi::__private::wire::WireDecode>::decode_from(buffer)?;
                    match value {
                        #(#decode_arms,)*
                        _ => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                            ::boltffi::__private::wire::InvalidWireValue::EnumTag
                        )),
                    }
                    .map(|value| (value, used))
                }
            }

            impl ::boltffi::__private::VecTransport for #enumeration {
                fn pack_vec(values: Vec<#enumeration>) -> ::boltffi::__private::FfiBuf {
                    let values = values
                        .into_iter()
                        .map(|value| value as #repr)
                        .collect::<Vec<#repr>>();
                    ::boltffi::__private::FfiBuf::from_vec(values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#enumeration> {
                    if byte_len == 0 {
                        return Vec::new();
                    }
                    let element_count = byte_len / ::core::mem::size_of::<#repr>();
                    unsafe {
                        ::core::slice::from_raw_parts(pointer as *const #repr, element_count)
                    }
                    .iter()
                    .copied()
                    .map(|value| unsafe {
                        <#enumeration as ::boltffi::__private::Passable>::unpack(value)
                    })
                    .collect::<Vec<#enumeration>>()
                }
            }

            #exports
        })
    }

    fn repr_type(&self) -> Result<Type, Error> {
        match self.binding.repr() {
            IntegerRepr::I8 => parse_str("i8"),
            IntegerRepr::U8 => parse_str("u8"),
            IntegerRepr::I16 => parse_str("i16"),
            IntegerRepr::U16 => parse_str("u16"),
            IntegerRepr::I32 => parse_str("i32"),
            IntegerRepr::U32 => parse_str("u32"),
            IntegerRepr::I64 => parse_str("i64"),
            IntegerRepr::U64 => parse_str("u64"),
            IntegerRepr::ISize => parse_str("isize"),
            IntegerRepr::USize => parse_str("usize"),
            _ => return Err(Error::UnsupportedExpansion("unknown enum repr")),
        }
        .map_err(|_| Error::SourceSyntaxMismatch("enum repr is not Rust syntax"))
    }

    fn variants(&self) -> Result<Vec<CStyleVariant>, Error> {
        if self.source.variants.len() != self.binding.variants().len() {
            return Err(Error::SourceSyntaxMismatch(
                "source and binding enum variant counts differ",
            ));
        }
        self.source
            .variants
            .iter()
            .zip(self.binding.variants())
            .map(|(source, binding)| {
                let expected = CanonicalName::from(&source.name);
                if binding.name() != &expected {
                    return Err(Error::SourceSyntaxMismatch(
                        "source and binding enum variant names differ",
                    ));
                }
                Ok(CStyleVariant {
                    name: names::SourceSpelling::new(&source.name)
                        .ident("source enum variant name is not a Rust identifier")?,
                })
            })
            .collect()
    }
}

impl<'expansion, 'lowered, S> Data<'expansion, 'lowered, S>
where
    S: RenderSurface,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Renderer:
        Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let enumeration = names::SourceSpelling::new(&self.source.name)
            .ident("source enum name is not a Rust identifier")?;
        let variants = self.variants()?;
        let wire_size_arms = variants
            .iter()
            .map(|variant| variant.wire_size_arm(&enumeration))
            .collect::<Result<Vec<_>, _>>()?;
        let encode_arms = variants
            .iter()
            .map(|variant| variant.encode_arm(&enumeration))
            .collect::<Result<Vec<_>, _>>()?;
        let decode_arms = variants
            .iter()
            .map(|variant| variant.decode_arm(&enumeration))
            .collect::<Result<Vec<_>, _>>()?;
        let exports = exports::Renderer::new(
            self.source,
            enumeration.clone(),
            exports::Receiver::Encoded {
                codec: self.binding.write(),
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()?;

        Ok(quote! {
            unsafe impl ::boltffi::__private::WirePassable for #enumeration {}

            impl ::boltffi::__private::wire::WireEncode for #enumeration {
                fn is_fixed_size() -> bool {
                    false
                }

                fn fixed_size() -> Option<usize> {
                    None
                }

                fn wire_size(&self) -> usize {
                    match self {
                        #(#wire_size_arms,)*
                    }
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    match self {
                        #(#encode_arms,)*
                    }
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #enumeration {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let tag_bytes: [u8; 4] = buffer
                        .get(0..4)
                        .ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?
                        .try_into()
                        .map_err(|_| ::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                    let tag = i32::from_le_bytes(tag_bytes);
                    match tag {
                        #(#decode_arms,)*
                        _ => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                            ::boltffi::__private::wire::InvalidWireValue::EnumTag
                        )),
                    }
                }
            }

            impl ::boltffi::__private::VecTransport for #enumeration {
                fn pack_vec(values: Vec<#enumeration>) -> ::boltffi::__private::FfiBuf {
                    ::boltffi::__private::FfiBuf::wire_encode(&values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#enumeration> {
                    let bytes = if byte_len == 0 {
                        &[]
                    } else {
                        unsafe { ::core::slice::from_raw_parts(pointer, byte_len) }
                    };
                    ::boltffi::__private::wire::decode::<Vec<#enumeration>>(bytes)
                        .expect("wire decode failed in VecTransport::unpack_vec")
                }
            }

            #exports
        })
    }

    fn variants(&self) -> Result<Vec<DataVariant<'expansion, 'lowered, S>>, Error> {
        if self.source.variants.len() != self.binding.variants().len() {
            return Err(Error::SourceSyntaxMismatch(
                "source and binding enum variant counts differ",
            ));
        }
        self.source
            .variants
            .iter()
            .zip(self.binding.variants())
            .map(|(source, binding)| {
                let expected = CanonicalName::from(&source.name);
                if binding.name() != &expected {
                    return Err(Error::SourceSyntaxMismatch(
                        "source and binding enum variant names differ",
                    ));
                }
                Ok(DataVariant {
                    source,
                    binding,
                    fields: DataVariant::fields(source, binding, self.expansion)?,
                })
            })
            .collect()
    }
}

struct DataVariant<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered VariantDef,
    binding: &'lowered DataVariantDecl,
    fields: Vec<DataField<'expansion, 'lowered, S>>,
}

struct DataField<'expansion, 'lowered, S: RenderSurface> {
    source: FieldSource<'lowered>,
    binding: &'lowered EncodedFieldDecl,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct DataFieldTokens {
    binding: Ident,
    wire_size: TokenStream,
    encode_to: TokenStream,
    decode_from: TokenStream,
}

#[derive(Clone, Copy)]
enum FieldSource<'lowered> {
    Tuple {
        index: usize,
        type_expr: &'lowered TypeExpr,
    },
    Struct(&'lowered FieldDef),
}

impl<'expansion, 'lowered, S: RenderSurface> DataVariant<'expansion, 'lowered, S> {
    fn wire_size_arm(&self, enumeration: &Ident) -> Result<TokenStream, Error> {
        let variant = names::SourceSpelling::new(&self.source.name)
            .ident("source enum variant name is not a Rust identifier")?;
        let fields = self
            .fields
            .iter()
            .map(DataField::tokens)
            .collect::<Result<Vec<_>, _>>()?;
        let bindings = fields
            .iter()
            .map(|field| &field.binding)
            .collect::<Vec<_>>();
        let wire_sizes = fields
            .iter()
            .map(|field| &field.wire_size)
            .collect::<Vec<_>>();
        Ok(match &self.source.payload {
            VariantPayload::Unit => quote! { #enumeration::#variant => 4usize },
            VariantPayload::Tuple(_) => quote! {
                #enumeration::#variant(#(#bindings),*) => {
                    4usize #(+ #wire_sizes)*
                }
            },
            VariantPayload::Struct(_) => quote! {
                #enumeration::#variant { #(#bindings),* } => {
                    4usize #(+ #wire_sizes)*
                }
            },
        })
    }

    fn encode_arm(&self, enumeration: &Ident) -> Result<TokenStream, Error> {
        let variant = names::SourceSpelling::new(&self.source.name)
            .ident("source enum variant name is not a Rust identifier")?;
        let tag = self.tag()?;
        let fields = self
            .fields
            .iter()
            .map(DataField::tokens)
            .collect::<Result<Vec<_>, _>>()?;
        let bindings = fields
            .iter()
            .map(|field| &field.binding)
            .collect::<Vec<_>>();
        let encoders = fields
            .iter()
            .map(|field| &field.encode_to)
            .collect::<Vec<_>>();
        Ok(match &self.source.payload {
            VariantPayload::Unit => quote! {
                #enumeration::#variant => {
                    buffer[0..4].copy_from_slice(&(#tag as i32).to_le_bytes());
                    4usize
                }
            },
            VariantPayload::Tuple(_) => quote! {
                #enumeration::#variant(#(#bindings),*) => {
                    buffer[0..4].copy_from_slice(&(#tag as i32).to_le_bytes());
                    let mut __boltffi_offset = 4usize;
                    #(#encoders)*
                    __boltffi_offset
                }
            },
            VariantPayload::Struct(_) => quote! {
                #enumeration::#variant { #(#bindings),* } => {
                    buffer[0..4].copy_from_slice(&(#tag as i32).to_le_bytes());
                    let mut __boltffi_offset = 4usize;
                    #(#encoders)*
                    __boltffi_offset
                }
            },
        })
    }

    fn decode_arm(&self, enumeration: &Ident) -> Result<TokenStream, Error> {
        let variant = names::SourceSpelling::new(&self.source.name)
            .ident("source enum variant name is not a Rust identifier")?;
        let tag = self.tag()?;
        let fields = self
            .fields
            .iter()
            .map(DataField::tokens)
            .collect::<Result<Vec<_>, _>>()?;
        let bindings = fields
            .iter()
            .map(|field| &field.binding)
            .collect::<Vec<_>>();
        let decoders = fields
            .iter()
            .map(|field| &field.decode_from)
            .collect::<Vec<_>>();
        Ok(match &self.source.payload {
            VariantPayload::Unit => quote! {
                #tag => Ok((#enumeration::#variant, 4usize))
            },
            VariantPayload::Tuple(_) => quote! {
                #tag => {
                    let mut __boltffi_offset = 4usize;
                    #(#decoders)*
                    Ok((#enumeration::#variant(#(#bindings),*), __boltffi_offset))
                }
            },
            VariantPayload::Struct(_) => quote! {
                #tag => {
                    let mut __boltffi_offset = 4usize;
                    #(#decoders)*
                    Ok((#enumeration::#variant { #(#bindings),* }, __boltffi_offset))
                }
            },
        })
    }

    fn tag(&self) -> Result<i32, Error> {
        i32::try_from(self.binding.tag().get())
            .map_err(|_| Error::UnsupportedExpansion("data enum tag overflow"))
    }

    fn fields(
        source: &'lowered VariantDef,
        binding: &'lowered DataVariantDecl,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Vec<DataField<'expansion, 'lowered, S>>, Error> {
        match (&source.payload, binding.payload()) {
            (VariantPayload::Unit, DataVariantPayload::Unit) => Ok(Vec::new()),
            (VariantPayload::Tuple(source_fields), DataVariantPayload::Tuple(binding_fields)) => {
                if source_fields.len() != binding_fields.len() {
                    return Err(Error::SourceSyntaxMismatch(
                        "source and binding enum tuple payload counts differ",
                    ));
                }
                Ok(source_fields
                    .iter()
                    .enumerate()
                    .zip(binding_fields)
                    .map(|((index, type_expr), binding)| DataField {
                        source: FieldSource::Tuple { index, type_expr },
                        binding,
                        expansion,
                    })
                    .collect())
            }
            (VariantPayload::Struct(source_fields), DataVariantPayload::Struct(binding_fields)) => {
                if source_fields.len() != binding_fields.len() {
                    return Err(Error::SourceSyntaxMismatch(
                        "source and binding enum struct payload counts differ",
                    ));
                }
                Ok(source_fields
                    .iter()
                    .zip(binding_fields)
                    .map(|(field, binding)| DataField {
                        source: FieldSource::Struct(field),
                        binding,
                        expansion,
                    })
                    .collect())
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source and binding enum payload shapes differ",
            )),
        }
    }
}

impl<'expansion, 'lowered, S: RenderSurface> DataField<'expansion, 'lowered, S> {
    fn tokens(&self) -> Result<DataFieldTokens, Error> {
        self.source.validate_key(self.binding.key())?;
        let binding = self.source.binding()?;
        let names = self.source.names(&binding);
        let decoded = names.decoded();
        let used = names.used();
        let wire = names.wire();
        let rust_type = rust_api::TypeTokens::new(self.source.type_expr()?)?.into_type();
        let source_type = self.source.type_expr()?;
        let codec = self.binding.codec().write().root();
        wrapper::encoded::require_runtime_wire(codec)?;
        rust_api::IncomingEncodedType::new(source_type).require_supported()?;
        Ok(DataFieldTokens {
            binding: binding.clone(),
            wire_size: self.wire_size(&binding, &wire, codec)?,
            encode_to: self.encode_to(&binding, &wire, codec)?,
            decode_from: self.decode_from(&binding, &decoded, &used, &rust_type, codec)?,
        })
    }

    fn wire_size(
        &self,
        binding: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = wrapper::encoded::BorrowedOutgoing::new(codec, self.expansion);
        if !conversion.has_custom_conversion() {
            return Ok(quote! {
                ::boltffi::__private::wire::WireEncode::wire_size(#binding)
            });
        }
        let converted = conversion.convert(quote! { #binding })?;
        Ok(quote! {
            {
                let #wire = #converted;
                ::boltffi::__private::wire::WireEncode::wire_size(&#wire)
            }
        })
    }

    fn encode_to(
        &self,
        binding: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = wrapper::encoded::BorrowedOutgoing::new(codec, self.expansion);
        let value = match conversion.has_custom_conversion() {
            true => {
                let converted = conversion.convert(quote! { #binding })?;
                quote! {
                    let #wire = #converted;
                    let __boltffi_written =
                        ::boltffi::__private::wire::WireEncode::encode_to(
                            &#wire,
                            &mut buffer[__boltffi_offset..]
                        );
                }
            }
            false => quote! {
                let __boltffi_written =
                    ::boltffi::__private::wire::WireEncode::encode_to(
                        #binding,
                        &mut buffer[__boltffi_offset..]
                    );
            },
        };
        Ok(quote! {
            {
                #value
                __boltffi_offset += __boltffi_written;
            }
        })
    }

    fn decode_from(
        &self,
        binding: &Ident,
        decoded: &Ident,
        used: &Ident,
        rust_type: &Type,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let incoming = wrapper::encoded::Incoming::new(codec, self.expansion);
        let decoded_type = incoming
            .decoded_type()?
            .unwrap_or_else(|| quote! { #rust_type });
        let converted = incoming.convert(quote! { #decoded })?;
        let value = match converted.changed() {
            true if converted.fallible() => {
                let converted_value = converted.tokens();
                quote! {
                    match #converted_value {
                        Ok(value) => value,
                        Err(_) => {
                            return Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                                ::boltffi::__private::wire::InvalidWireValue::CustomConversion
                            ));
                        }
                    }
                }
            }
            true => {
                let converted_value = converted.tokens();
                quote! { #converted_value }
            }
            false => quote! { #decoded },
        };
        let type_annotation = (!converted.changed()).then(|| quote! { : #rust_type });
        Ok(quote! {
            let (#decoded, #used) =
                <#decoded_type as ::boltffi::__private::wire::WireDecode>::decode_from(
                    &buffer[__boltffi_offset..]
                )?;
            __boltffi_offset += #used;
            let #binding #type_annotation = #value;
        })
    }
}

impl<'lowered> FieldSource<'lowered> {
    fn validate_key(self, key: &FieldKey) -> Result<(), Error> {
        match (self, key) {
            (Self::Tuple { index, .. }, FieldKey::Position(position))
                if index == *position as usize =>
            {
                Ok(())
            }
            (Self::Struct(field), FieldKey::Named(name))
                if name == &CanonicalName::from(&field.name) =>
            {
                Ok(())
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source and binding enum payload field keys differ",
            )),
        }
    }

    fn binding(self) -> Result<Ident, Error> {
        match self {
            Self::Tuple { index, .. } => Ok(names::PayloadField::new(index).value()),
            Self::Struct(field) => names::SourceSpelling::new(&field.name)
                .ident("source enum payload field name is not a Rust identifier"),
        }
    }

    fn names(self, binding: &Ident) -> FieldNames {
        match self {
            Self::Tuple { index, .. } => FieldNames::Payload(names::PayloadField::new(index)),
            Self::Struct(_) => FieldNames::Record(names::RecordField::new(binding)),
        }
    }

    fn type_expr(self) -> Result<&'lowered TypeExpr, Error> {
        match self {
            Self::Tuple { type_expr, .. } => Ok(type_expr),
            Self::Struct(field) => Ok(&field.type_expr),
        }
    }
}

enum FieldNames {
    Payload(names::PayloadField),
    Record(names::RecordField),
}

impl FieldNames {
    fn decoded(&self) -> Ident {
        match self {
            Self::Payload(names) => names.decoded(),
            Self::Record(names) => names.decoded(),
        }
    }

    fn used(&self) -> Ident {
        match self {
            Self::Payload(names) => names.used(),
            Self::Record(names) => names.used(),
        }
    }

    fn wire(&self) -> Ident {
        match self {
            Self::Payload(names) => names.wire(),
            Self::Record(names) => names.wire(),
        }
    }
}

struct CStyleVariant {
    name: Ident,
}

impl CStyleVariant {
    fn discriminant_arm(&self, enumeration: &Ident, repr: &Type) -> Result<TokenStream, Error> {
        let variant = &self.name;
        Ok(quote! {
            value if value == #enumeration::#variant as #repr => #enumeration::#variant
        })
    }

    fn decode_arm(&self, enumeration: &Ident, repr: &Type) -> Result<TokenStream, Error> {
        let variant = &self.name;
        Ok(quote! {
            value if value == #enumeration::#variant as #repr => Ok(#enumeration::#variant)
        })
    }

    fn repr_arm(&self, enumeration: &Ident, repr: &Type) -> Result<TokenStream, Error> {
        let variant = &self.name;
        Ok(quote! {
            #enumeration::#variant => #enumeration::#variant as #repr
        })
    }
}
