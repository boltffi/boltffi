use boltffi_ast::{FieldDef, MethodDef, Path as SourcePath, RecordDef, TypeExpr};
use boltffi_binding::{
    CanonicalName, CodecNode, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExecutionDecl,
    FieldKey, Native, Receive, RecordDecl, Wasm32, WritePlan,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    contract::{DeclarationPair, Expansion},
    error::Error,
    rust_api,
    wrapper::{self, associated_fn, encoded, export, names, native_opaque},
};

pub struct Record<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
struct Direct<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    source: &'lowered RecordDef,
    binding: &'lowered DirectRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
struct Encoded<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    source: &'lowered RecordDef,
    binding: &'lowered EncodedRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedField<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    source: &'lowered FieldDef,
    binding: &'lowered EncodedFieldDecl,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedFieldTokens {
    fixed_size_check: TokenStream,
    fixed_size: TokenStream,
    wire_size: TokenStream,
    encode_to: TokenStream,
    decode_from: TokenStream,
    initializer: Ident,
}

struct RecordOwner<'lowered> {
    source: &'lowered RecordDef,
    record: TokenStream,
    rust_type: Type,
    receiver: ReceiverKind<'lowered>,
}

#[derive(Clone, Copy)]
enum ReceiverKind<'lowered> {
    Direct,
    Encoded { codec: &'lowered WritePlan },
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Record<'expansion, 'lowered, S> {
    pub fn new(
        pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { pair, expansion }
    }

    pub fn render_runtime(self) -> Result<TokenStream, Error> {
        match self.pair.binding() {
            RecordDecl::Direct(binding) => Direct {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .runtime(),
            RecordDecl::Encoded(binding) => Encoded {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .runtime(),
            _ => Err(Error::UnsupportedExpansion("unknown record declaration")),
        }
    }
}

impl<'expansion, 'lowered> Record<'expansion, 'lowered, Native> {
    pub fn render_exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        match self.pair.binding() {
            RecordDecl::Direct(binding) => Direct {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .exports(rust_type),
            RecordDecl::Encoded(binding) => Encoded {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .exports(rust_type),
            _ => Err(Error::UnsupportedExpansion("unknown record declaration")),
        }
    }
}

impl<'expansion, 'lowered> Record<'expansion, 'lowered, Wasm32> {
    pub fn render_exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        match self.pair.binding() {
            RecordDecl::Direct(binding) => Direct {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .exports(rust_type),
            RecordDecl::Encoded(binding) => Encoded {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .exports(rust_type),
            _ => Err(Error::UnsupportedExpansion("unknown record declaration")),
        }
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Direct<'expansion, 'lowered, S> {
    fn runtime(self) -> Result<TokenStream, Error> {
        let record = names::SourceSpelling::new(&self.source.name)
            .ident("source record name is not a Rust identifier")?;
        let layout = LayoutCheck::new(
            self.binding.layout().size().get(),
            self.binding.layout().alignment().get(),
        )?;
        let size = layout.size();
        let alignment = layout.alignment();
        let field_offsets = self
            .source
            .fields
            .iter()
            .zip(self.binding.layout().fields())
            .map(|(source, field)| {
                let ident = names::SourceSpelling::new(&source.name)
                    .ident("source record field name is not a Rust identifier")?;
                let offset = LayoutCheck::bytes(field.offset().get())?;
                Ok(quote! {
                    const _: [(); #offset] = [(); ::core::mem::offset_of!(#record, #ident)];
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(quote! {
            const _: [(); #size] = [(); ::core::mem::size_of::<#record>()];
            const _: [(); 0] = [(); #alignment % ::core::mem::align_of::<#record>()];
            #(#field_offsets)*

            unsafe impl ::boltffi::__private::Passable for #record {
                type In = #record;
                type Out = #record;

                unsafe fn unpack(input: #record) -> Self {
                    input
                }

                fn pack(self) -> #record {
                    self
                }
            }

            unsafe impl ::boltffi::__private::wire::Blittable for #record {}

            impl ::boltffi::__private::wire::WireEncode for #record {
                const ENCODING_KIND: ::boltffi::__private::wire::WireEncodingKind =
                    ::boltffi::__private::wire::WireEncodingKind::Blittable;

                fn is_fixed_size() -> bool {
                    true
                }

                fn fixed_size() -> Option<usize> {
                    Some(::core::mem::size_of::<Self>())
                }

                fn wire_size(&self) -> usize {
                    ::core::mem::size_of::<Self>()
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    <Self as ::boltffi::__private::wire::Blittable>::encode_value(self, buffer)
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #record {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    match <Self as ::boltffi::__private::wire::Blittable>::decode_value(buffer) {
                        Some(value) => Ok((value, ::core::mem::size_of::<Self>())),
                        None => Err(::boltffi::__private::wire::DecodeError::BufferTooSmall),
                    }
                }
            }

            impl ::boltffi::__private::VecTransport for #record {
                fn pack_vec(values: Vec<#record>) -> ::boltffi::__private::FfiBuf {
                    ::boltffi::__private::FfiBuf::from_vec(values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#record> {
                    if byte_len == 0 {
                        return Vec::new();
                    }
                    let element_count = byte_len / ::core::mem::size_of::<#record>();
                    unsafe {
                        ::core::slice::from_raw_parts(pointer as *const #record, element_count)
                    }
                    .to_vec()
                }
            }
        })
    }
}

impl<'expansion, 'lowered> Direct<'expansion, 'lowered, Native> {
    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        associated_fn::AssociatedFunctions::new(
            RecordOwner {
                source: self.source,
                record: quote! { #rust_type },
                rust_type,
                receiver: ReceiverKind::Direct,
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered> Direct<'expansion, 'lowered, Wasm32> {
    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        associated_fn::AssociatedFunctions::new(
            RecordOwner {
                source: self.source,
                record: quote! { #rust_type },
                rust_type,
                receiver: ReceiverKind::Direct,
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Encoded<'expansion, 'lowered, S> {
    fn runtime(self) -> Result<TokenStream, Error> {
        let record = names::SourceSpelling::new(&self.source.name)
            .ident("source record name is not a Rust identifier")?;
        let fields = self.fields()?;
        let wire_sizes = fields
            .iter()
            .map(|field| &field.wire_size)
            .collect::<Vec<_>>();
        let fixed_size_checks = fields
            .iter()
            .map(|field| &field.fixed_size_check)
            .collect::<Vec<_>>();
        let fixed_sizes = fields
            .iter()
            .map(|field| &field.fixed_size)
            .collect::<Vec<_>>();
        let encoders = fields
            .iter()
            .map(|field| &field.encode_to)
            .collect::<Vec<_>>();
        let decoders = fields
            .iter()
            .map(|field| &field.decode_from)
            .collect::<Vec<_>>();
        let initializers = fields
            .iter()
            .map(|field| &field.initializer)
            .collect::<Vec<_>>();
        Ok(quote! {
            unsafe impl ::boltffi::__private::WirePassable for #record {}

            impl ::boltffi::__private::wire::WireEncode for #record {
                fn is_fixed_size() -> bool {
                    true #(&& #fixed_size_checks)*
                }

                fn fixed_size() -> Option<usize> {
                    <Self as ::boltffi::__private::wire::WireEncode>::is_fixed_size()
                        .then(|| 0 #(+ #fixed_sizes)*)
                }

                fn wire_size(&self) -> usize {
                    <Self as ::boltffi::__private::wire::WireEncode>::fixed_size()
                        .unwrap_or_else(|| 0 #(+ #wire_sizes)*)
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    let mut __boltffi_offset = 0usize;
                    #(#encoders)*
                    __boltffi_offset
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #record {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let mut __boltffi_offset = 0usize;
                    #(#decoders)*
                    Ok((Self { #(#initializers),* }, __boltffi_offset))
                }
            }

            impl ::boltffi::__private::VecTransport for #record {
                fn pack_vec(values: Vec<#record>) -> ::boltffi::__private::FfiBuf {
                    ::boltffi::__private::FfiBuf::wire_encode(&values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#record> {
                    let bytes = if byte_len == 0 {
                        &[]
                    } else {
                        unsafe { ::core::slice::from_raw_parts(pointer, byte_len) }
                    };
                    ::boltffi::__private::wire::decode::<Vec<#record>>(bytes)
                        .expect("wire decode failed in VecTransport::unpack_vec")
                }
            }
        })
    }

    fn fields(&self) -> Result<Vec<EncodedFieldTokens>, Error> {
        if self.source.fields.len() != self.binding.fields().len() {
            return Err(Error::SourceSyntaxMismatch(
                "source and binding record field counts differ",
            ));
        }
        self.source
            .fields
            .iter()
            .zip(self.binding.fields())
            .map(|(source, binding)| {
                EncodedField {
                    source,
                    binding,
                    expansion: self.expansion,
                }
                .tokens()
            })
            .collect()
    }
}

impl<'expansion, 'lowered> Encoded<'expansion, 'lowered, Native> {
    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        // An opaque record exposes no initializers or wire methods; its whole
        // host surface is the generated handle accessors.
        if self.binding.is_native_opaque() {
            return native_opaque::render(self.source, self.binding);
        }
        associated_fn::AssociatedFunctions::new(
            RecordOwner {
                source: self.source,
                record: quote! { #rust_type },
                rust_type,
                receiver: ReceiverKind::Encoded {
                    codec: self.binding.write(),
                },
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered> Encoded<'expansion, 'lowered, Wasm32> {
    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        associated_fn::AssociatedFunctions::new(
            RecordOwner {
                source: self.source,
                record: quote! { #rust_type },
                rust_type,
                receiver: ReceiverKind::Encoded {
                    codec: self.binding.write(),
                },
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> EncodedField<'expansion, 'lowered, S> {
    fn tokens(self) -> Result<EncodedFieldTokens, Error> {
        self.validate_key()?;
        let field = names::SourceSpelling::new(&self.source.name)
            .ident("source field name is not a Rust identifier")?;
        let generated = names::RecordField::new(&field);
        let decoded = generated.decoded();
        let used = generated.used();
        let wire = generated.wire();
        let rust_type = rust_api::TypeTokens::new(&self.source.type_expr)?.into_type();
        let codec = self.binding.codec().write().root();
        encoded::require_runtime_wire(codec)?;
        rust_api::IncomingEncodedType::new(&self.source.type_expr).require_supported()?;
        let conversion = encoded::BorrowedOutgoing::new(codec, self.expansion);
        let (fixed_size_check, fixed_size) = match conversion.has_custom_conversion() {
            true => (quote! { false }, quote! { 0 }),
            false => (
                quote! {
                    <#rust_type as ::boltffi::__private::wire::WireEncode>::is_fixed_size()
                },
                quote! {
                    <#rust_type as ::boltffi::__private::wire::WireEncode>::fixed_size()
                        .unwrap_or(0)
                },
            ),
        };
        let wire_size = self.wire_size(&field, &wire, codec)?;
        let encode_to = self.encode_to(&field, &wire, codec)?;
        let decode_from = self.decode_from(&field, &decoded, &used, &rust_type, codec)?;
        Ok(EncodedFieldTokens {
            fixed_size_check,
            fixed_size,
            wire_size,
            encode_to,
            decode_from,
            initializer: field,
        })
    }

    fn validate_key(&self) -> Result<(), Error> {
        let expected = FieldKey::Named(CanonicalName::from(&self.source.name));
        if self.binding.key() == &expected {
            return Ok(());
        }
        Err(Error::SourceSyntaxMismatch(
            "source and binding record field keys differ",
        ))
    }

    fn wire_size(
        &self,
        field: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = encoded::BorrowedOutgoing::new(codec, self.expansion);
        if !conversion.has_custom_conversion() {
            return Ok(quote! {
                ::boltffi::__private::wire::WireEncode::wire_size(&self.#field)
            });
        }
        let converted = conversion.convert(quote! { &self.#field })?;
        Ok(quote! {
            {
                let #wire = #converted;
                ::boltffi::__private::wire::WireEncode::wire_size(&#wire)
            }
        })
    }

    fn encode_to(
        &self,
        field: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = encoded::BorrowedOutgoing::new(codec, self.expansion);
        let value = match conversion.has_custom_conversion() {
            true => {
                let converted = conversion.convert(quote! { &self.#field })?;
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
                        &self.#field,
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
        field: &Ident,
        decoded: &Ident,
        used: &Ident,
        rust_type: &Type,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let incoming = encoded::Incoming::new(codec, self.expansion);
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
            let #field #type_annotation = #value;
        })
    }
}

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Native>
    for RecordOwner<'lowered>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::record(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::record_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        self.receiver
            .render_native(self.source, &self.record, &self.rust_type, export)
    }
}

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Wasm32>
    for RecordOwner<'lowered>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::record(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::record_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        self.receiver
            .render_wasm32(self.source, &self.record, &self.rust_type, export)
    }
}

impl<'receiver> ReceiverKind<'receiver> {
    fn render_native<'expansion>(
        self,
        source: &'receiver RecordDef,
        record: &TokenStream,
        rust_type: &Type,
        export: associated_fn::ReceiverExport<'expansion, 'receiver, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receive = export.callable().receiver();
        let execution = export.callable().execution();
        let method = export.method().clone();
        let failure = export.failure();
        let expansion = export.expansion();
        match (self, receive) {
            (Self::Direct, Some(receive)) => {
                let receiver = names::Locals::new(method.span()).receiver();
                let tokens = wrapper::param::direct::RecordInput::new(
                    receive,
                    rust_type.clone(),
                    receiver.clone(),
                    TokenStream::new(),
                )
                .native()?;
                let direct_writeback = self.direct_writeback(
                    receive,
                    &receiver,
                    rust_type,
                    tokens.writebacks().is_empty(),
                    failure.render()?,
                )?;
                let ffi_parameters = tokens
                    .ffi_parameters()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.ffi_parameters)
                    .collect();
                let conversions = tokens
                    .conversions()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.conversions)
                    .collect();
                let writebacks = tokens
                    .writebacks()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.writebacks)
                    .collect();
                Ok((
                    export::ReceiverTokens::new(
                        ffi_parameters,
                        conversions,
                        writebacks,
                        direct_writeback.requires_failure_return,
                    ),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Direct, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
            (Self::Encoded { codec }, Some(receive)) => {
                let source_type = TypeExpr::record(
                    source.id.clone(),
                    SourcePath::single(source.name.spelling()),
                );
                let receiver = names::Locals::new(method.span()).receiver();
                let async_shared_receiver = receive == Receive::ByRef
                    && matches!(execution, ExecutionDecl::Asynchronous(_));
                let decode_target = match async_shared_receiver {
                    true => rust_api::DecodeTarget::by_value(&source_type)?,
                    false => rust_api::DecodeTarget::received(receive, &source_type)?,
                };
                let failure = failure.render()?;
                let tokens = wrapper::param::encoded::Input::new(
                    codec,
                    <Native as boltffi_binding::SurfaceLower>::encoded_param_shape(),
                    decode_target,
                    receiver.clone(),
                    failure.clone(),
                    expansion,
                )
                .render()?;
                let encoded_writeback =
                    self.encoded_writeback(receive, codec, &receiver, failure, expansion)?;
                let ffi_parameters = tokens
                    .ffi_parameters()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.ffi_parameters)
                    .collect();
                let conversions = tokens
                    .conversions()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.conversions)
                    .collect();
                let writebacks = tokens
                    .writebacks()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.writebacks)
                    .collect();
                Ok((
                    export::ReceiverTokens::new(ffi_parameters, conversions, writebacks, true),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Encoded { .. }, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
        }
    }

    fn render_wasm32<'expansion>(
        self,
        source: &'receiver RecordDef,
        record: &TokenStream,
        rust_type: &Type,
        export: associated_fn::ReceiverExport<'expansion, 'receiver, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receive = export.callable().receiver();
        let execution = export.callable().execution();
        let method = export.method().clone();
        let failure = export.failure();
        let expansion = export.expansion();
        match (self, receive) {
            (Self::Direct, Some(receive)) => {
                let receiver = names::Locals::new(method.span()).receiver();
                let failure = failure.render()?;
                let tokens = wrapper::param::direct::RecordInput::new(
                    receive,
                    rust_type.clone(),
                    receiver.clone(),
                    failure.clone(),
                )
                .wasm32()?;
                let direct_writeback = self.direct_writeback(
                    receive,
                    &receiver,
                    rust_type,
                    tokens.writebacks().is_empty(),
                    failure,
                )?;
                let ffi_parameters = tokens
                    .ffi_parameters()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.ffi_parameters)
                    .collect();
                let conversions = tokens
                    .conversions()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.conversions)
                    .collect();
                let writebacks = tokens
                    .writebacks()
                    .iter()
                    .cloned()
                    .chain(direct_writeback.writebacks)
                    .collect();
                Ok((
                    export::ReceiverTokens::new(ffi_parameters, conversions, writebacks, true),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Direct, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
            (Self::Encoded { codec }, Some(receive)) => {
                let source_type = TypeExpr::record(
                    source.id.clone(),
                    SourcePath::single(source.name.spelling()),
                );
                let receiver = names::Locals::new(method.span()).receiver();
                let async_shared_receiver = receive == Receive::ByRef
                    && matches!(execution, ExecutionDecl::Asynchronous(_));
                let decode_target = match async_shared_receiver {
                    true => rust_api::DecodeTarget::by_value(&source_type)?,
                    false => rust_api::DecodeTarget::received(receive, &source_type)?,
                };
                let failure = failure.render()?;
                let tokens = wrapper::param::encoded::Input::new(
                    codec,
                    <Wasm32 as boltffi_binding::SurfaceLower>::encoded_param_shape(),
                    decode_target,
                    receiver.clone(),
                    failure.clone(),
                    expansion,
                )
                .render()?;
                let encoded_writeback =
                    self.encoded_writeback(receive, codec, &receiver, failure, expansion)?;
                let ffi_parameters = tokens
                    .ffi_parameters()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.ffi_parameters)
                    .collect();
                let conversions = tokens
                    .conversions()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.conversions)
                    .collect();
                let writebacks = tokens
                    .writebacks()
                    .iter()
                    .cloned()
                    .chain(encoded_writeback.writebacks)
                    .collect();
                Ok((
                    export::ReceiverTokens::new(ffi_parameters, conversions, writebacks, true),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Encoded { .. }, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
        }
    }

    fn direct_writeback(
        self,
        receive: Receive,
        receiver: &Ident,
        rust_type: &Type,
        needs_writeback: bool,
        failure: TokenStream,
    ) -> Result<ReceiverWriteback, Error> {
        if receive != Receive::ByMutRef || !needs_writeback {
            return Ok(ReceiverWriteback::none());
        }
        let out = names::Parameter::new(receiver).writeback();
        Ok(ReceiverWriteback {
            ffi_parameters: vec![quote! {
                #out: *mut <#rust_type as ::boltffi::__private::Passable>::In
            }],
            conversions: vec![quote! {
                if #out.is_null() {
                    ::boltffi::__private::set_last_error("receiver writeback pointer is null".to_string());
                    #failure
                }
            }],
            writebacks: vec![quote! {
                unsafe {
                    ::core::ptr::write_unaligned(
                        #out,
                        <#rust_type as ::boltffi::__private::Passable>::pack(#receiver)
                    );
                }
            }],
            requires_failure_return: true,
        })
    }

    fn encoded_writeback<'expansion, S: boltffi_binding::SurfaceLower>(
        self,
        receive: Receive,
        codec: &'receiver WritePlan,
        receiver: &Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'receiver, S>,
    ) -> Result<ReceiverWriteback, Error> {
        if receive != Receive::ByMutRef {
            return Ok(ReceiverWriteback::none());
        }
        let out = names::Parameter::new(receiver).writeback();
        let storage = names::Parameter::new(receiver).storage();
        let buffer =
            encoded::outgoing::Value::new(codec.root(), expansion).buffer(quote! { #storage })?;
        Ok(ReceiverWriteback {
            ffi_parameters: vec![quote! { #out: *mut ::boltffi::__private::FfiBuf }],
            conversions: vec![quote! {
                if #out.is_null() {
                    ::boltffi::__private::set_last_error("receiver writeback pointer is null".to_string());
                    #failure
                }
            }],
            writebacks: vec![quote! {
                unsafe {
                    ::core::ptr::write(#out, #buffer);
                }
            }],
            requires_failure_return: true,
        })
    }
}

struct ReceiverWriteback {
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    requires_failure_return: bool,
}

struct LayoutCheck {
    size: usize,
    alignment: usize,
}

impl LayoutCheck {
    fn new(size: u64, alignment: u64) -> Result<Self, Error> {
        Ok(Self {
            size: Self::bytes(size)?,
            alignment: Self::bytes(alignment)?,
        })
    }

    const fn size(&self) -> usize {
        self.size
    }

    const fn alignment(&self) -> usize {
        self.alignment
    }

    fn bytes(bytes: u64) -> Result<usize, Error> {
        usize::try_from(bytes)
            .map_err(|_| Error::SourceSyntaxMismatch("record layout is too large"))
    }
}

impl ReceiverWriteback {
    fn none() -> Self {
        Self {
            ffi_parameters: Vec::new(),
            conversions: Vec::new(),
            writebacks: Vec::new(),
            requires_failure_return: false,
        }
    }
}
