use boltffi_ast::{FieldDef, MethodDef, Path as SourcePath, RecordDef, TypeExpr};
use boltffi_binding::{
    CanonicalName, CodecNode, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExecutionDecl,
    FieldKey, Primitive, Receive, RecordDecl, SurfaceLower, WritePlan,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse_quote};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::{DirectRecordCrossing, RenderSurface},
    wrapper::{self, Render, associated_fn, encoded, export, names},
};

/// A record declaration renderer for one target surface.
///
/// The renderer emits the runtime trait implementations that make a scanned Rust
/// record usable by generated wrappers. The record shape comes from the lowered
/// `RecordDecl`, so the generated code cannot reclassify the source struct.
pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Direct<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered RecordDef,
    binding: &'lowered DirectRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Encoded<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered RecordDef,
    binding: &'lowered EncodedRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedField<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered FieldDef,
    binding: &'lowered EncodedFieldDecl,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedFieldTokens {
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

impl<'expansion, 'lowered, S: RenderSurface> Renderer<'expansion, 'lowered, S> {
    /// Creates a renderer for one paired record declaration.
    pub fn new(
        pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { pair, expansion }
    }

    /// Renders the runtime trait implementations for the record.
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
        wrapper::param::direct::Record:
            Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        match self.pair.binding() {
            RecordDecl::Direct(binding) => Direct {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .render(),
            RecordDecl::Encoded(binding) => Encoded {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .render(),
            _ => Err(Error::UnsupportedExpansion("unknown record declaration")),
        }
    }

    pub fn render_exports(self, rust_type: Type) -> Result<TokenStream, Error>
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
        wrapper::param::direct::Record:
            Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
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

impl<'expansion, 'lowered, S> Direct<'expansion, 'lowered, S>
where
    'lowered: 'expansion,
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
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
{
    fn render(self) -> Result<TokenStream, Error> {
        let record = names::SourceSpelling::new(&self.source.name)
            .ident("source record name is not a Rust identifier")?;
        let layout = LayoutCheck::new(
            self.binding.layout().size().get(),
            self.binding.layout().alignment().get(),
        )?;
        let size = layout.size();
        let alignment = layout.alignment();
        let exports = associated_fn::Renderer::new(
            RecordOwner {
                source: self.source,
                record: quote! { #record },
                rust_type: parse_quote! { #record },
                receiver: ReceiverKind::Direct,
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()?;
        Ok(quote! {
            const _: [(); #size] = [(); ::core::mem::size_of::<#record>()];
            const _: [(); #alignment] = [(); ::core::mem::align_of::<#record>()];

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

            #exports
        })
    }

    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        associated_fn::Renderer::new(
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

impl<'expansion, 'lowered, S> Encoded<'expansion, 'lowered, S>
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
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
{
    fn render(self) -> Result<TokenStream, Error> {
        if self.binding.is_native_opaque() {
            return self.render_native_opaque();
        }
        let record = names::SourceSpelling::new(&self.source.name)
            .ident("source record name is not a Rust identifier")?;
        let fields = self.fields()?;
        let wire_sizes = fields
            .iter()
            .map(|field| &field.wire_size)
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
        let exports = associated_fn::Renderer::new(
            RecordOwner {
                source: self.source,
                record: quote! { #record },
                rust_type: parse_quote! { #record },
                receiver: ReceiverKind::Encoded {
                    codec: self.binding.write(),
                },
            },
            self.binding.initializers(),
            self.binding.methods(),
            self.expansion,
        )
        .render()?;

        Ok(quote! {
            unsafe impl ::boltffi::__private::WirePassable for #record {}

            impl ::boltffi::__private::wire::WireEncode for #record {
                fn wire_size(&self) -> usize {
                    0 #(+ #wire_sizes)*
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

            #exports
        })
    }

    fn exports(self, rust_type: Type) -> Result<TokenStream, Error> {
        associated_fn::Renderer::new(
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

    fn render_native_opaque(&self) -> Result<TokenStream, Error> {
        if !self.binding.initializers().is_empty() || !self.binding.methods().is_empty() {
            return Err(Error::UnsupportedExpansion(
                "native opaque record initializers and methods",
            ));
        }
        let record_ident = names::SourceSpelling::new(&self.source.name)
            .ident("source record name is not a Rust identifier")?;

        let exports = self
            .binding
            .native_opaque_exports()
            .ok_or(Error::UnsupportedExpansion(
                "native opaque record without helper exports",
            ))?;
        let drop_sym = quote::format_ident!("{}", exports.drop().name().as_str());
        let dsize_sym = quote::format_ident!("{}", exports.dsize().name().as_str());
        let dsize_terms = self
            .source
            .fields
            .iter()
            .zip(self.binding.fields().iter())
            .map(|(source_field, binding_field)| {
                let field_ident = names::SourceSpelling::new(&source_field.name)
                    .ident("source field name is not a Rust identifier")?;
                let (ty, optional) = match binding_field.ty() {
                    boltffi_binding::TypeRef::Optional(inner) => (inner.as_ref(), true),
                    ty => (ty, false),
                };
                let term = match (ty, optional) {
                    (boltffi_binding::TypeRef::String, false)
                    | (boltffi_binding::TypeRef::Bytes, false) => {
                        quote! { record.#field_ident.capacity() }
                    }
                    (boltffi_binding::TypeRef::String, true)
                    | (boltffi_binding::TypeRef::Bytes, true) => {
                        quote! { record.#field_ident.as_ref().map_or(0usize, |value| value.capacity()) }
                    }
                    (boltffi_binding::TypeRef::InternedString { .. }, false) => {
                        quote! {
                            match record.#field_ident.repr() {
                                ::boltffi::InternedStringRepr::Dynamic(value) => value.capacity(),
                                ::boltffi::InternedStringRepr::Interned(_) => 0usize,
                            }
                        }
                    }
                    (boltffi_binding::TypeRef::InternedString { .. }, true) => {
                        quote! {
                            record.#field_ident.as_ref().map_or(0usize, |value| match value.repr() {
                                ::boltffi::InternedStringRepr::Dynamic(value) => value.capacity(),
                                ::boltffi::InternedStringRepr::Interned(_) => 0usize,
                            })
                        }
                    }
                    _ => quote! { 0usize },
                };
                Ok(term)
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let mut accessors: Vec<TokenStream> = Vec::new();

        // Generate drop and dsize
        accessors.push(quote! {
            #[allow(clippy::missing_safety_doc)]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #drop_sym(handle: *mut ::core::ffi::c_void) {
                if handle.is_null() { return; }
                // Catch panics to prevent unwinding through C boundary.
                if ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                    drop(unsafe { ::std::boxed::Box::from_raw(handle as *mut #record_ident) });
                })).is_err() {
                    ::std::process::abort();
                }
            }

            #[allow(clippy::missing_safety_doc)]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #dsize_sym(handle: *const ::core::ffi::c_void) -> usize {
                if handle.is_null() { return 0; }
                let record = unsafe { &*(handle as *const #record_ident) };
                ::std::mem::size_of::<#record_ident>() #( + #dsize_terms )*
            }
        });

        // Generate per-field accessors
        for ((source_field, binding_field), field_exports) in self
            .source
            .fields
            .iter()
            .zip(self.binding.fields().iter())
            .zip(exports.fields().iter())
        {
            let field_ident = names::SourceSpelling::new(&source_field.name)
                .ident("source field name is not a Rust identifier")?;
            if field_exports.key() != binding_field.key() {
                return Err(Error::SourceSyntaxMismatch(
                    "native opaque field exports do not match record fields",
                ));
            }

            let (ty, optional) = match binding_field.ty() {
                boltffi_binding::TypeRef::Optional(inner) => (inner.as_ref(), true),
                ty => (ty, false),
            };

            if optional {
                let has_sym = quote::format_ident!(
                    "{}",
                    field_exports
                        .has()
                        .ok_or(Error::UnsupportedExpansion(
                            "optional native opaque field without presence export",
                        ))?
                        .name()
                        .as_str()
                );
                accessors.push(quote! {
                    #[allow(clippy::missing_safety_doc)]
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #has_sym(handle: *const ::core::ffi::c_void) -> ::core::ffi::c_int {
                        if handle.is_null() { return 0; }
                        let record = unsafe { &*(handle as *const #record_ident) };
                        record.#field_ident.is_some() as ::core::ffi::c_int
                    }
                });
            }

            match ty {
                boltffi_binding::TypeRef::Primitive(primitive) => {
                    let prim_type = primitive_rust_type(*primitive);
                    let get_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .get()
                            .ok_or(Error::UnsupportedExpansion(
                                "primitive native opaque field without getter export",
                            ))?
                            .name()
                            .as_str()
                    );
                    if optional {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #get_sym(handle: *const ::core::ffi::c_void) -> #prim_type {
                                if handle.is_null() { return Default::default(); }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                record.#field_ident.unwrap_or_default()
                            }
                        });
                    } else {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #get_sym(handle: *const ::core::ffi::c_void) -> #prim_type {
                                if handle.is_null() { return Default::default(); }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                record.#field_ident
                            }
                        });
                    }
                }
                boltffi_binding::TypeRef::String => {
                    let borrow_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .borrow()
                            .ok_or(Error::UnsupportedExpansion(
                                "string native opaque field without borrow export",
                            ))?
                            .name()
                            .as_str()
                    );
                    if optional {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match &record.#field_ident {
                                    Some(s) => {
                                        unsafe {
                                            *ptr_out = s.as_ptr();
                                            *len_out = s.len();
                                        }
                                        1
                                    }
                                    None => 0,
                                }
                            }
                        });
                    } else {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                unsafe {
                                    *ptr_out = record.#field_ident.as_ptr();
                                    *len_out = record.#field_ident.len();
                                }
                                1
                            }
                        });
                    }
                }
                boltffi_binding::TypeRef::Bytes => {
                    let borrow_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .borrow()
                            .ok_or(Error::UnsupportedExpansion(
                                "bytes native opaque field without borrow export",
                            ))?
                            .name()
                            .as_str()
                    );
                    if optional {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match &record.#field_ident {
                                    Some(b) => {
                                        unsafe {
                                            *ptr_out = b.as_ptr();
                                            *len_out = b.len();
                                        }
                                        1
                                    }
                                    None => 0,
                                }
                            }
                        });
                    } else {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                unsafe {
                                    *ptr_out = record.#field_ident.as_ptr();
                                    *len_out = record.#field_ident.len();
                                }
                                1
                            }
                        });
                    }
                }
                boltffi_binding::TypeRef::InternedString { .. } => {
                    let tag_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .interned_tag()
                            .ok_or(Error::UnsupportedExpansion(
                                "interned native opaque field without tag export",
                            ))?
                            .name()
                            .as_str()
                    );
                    let id_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .interned_id()
                            .ok_or(Error::UnsupportedExpansion(
                                "interned native opaque field without id export",
                            ))?
                            .name()
                            .as_str()
                    );
                    let borrow_dyn_sym = quote::format_ident!(
                        "{}",
                        field_exports
                            .interned_borrow_dynamic()
                            .ok_or(Error::UnsupportedExpansion(
                                "interned native opaque field without dynamic borrow export",
                            ))?
                            .name()
                            .as_str()
                    );
                    if optional {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #tag_sym(handle: *const ::core::ffi::c_void) -> u8 {
                                if handle.is_null() { return 0xff; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match &record.#field_ident {
                                    Some(s) => match s.repr() {
                                        ::boltffi::InternedStringRepr::Interned(_) => 0u8,
                                        ::boltffi::InternedStringRepr::Dynamic(_) => 1u8,
                                    },
                                    None => 0xffu8,
                                }
                            }

                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #id_sym(handle: *const ::core::ffi::c_void) -> u32 {
                                if handle.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match &record.#field_ident {
                                    Some(s) => match s.repr() {
                                        ::boltffi::InternedStringRepr::Interned(id) => *id,
                                        _ => 0,
                                    },
                                    None => 0,
                                }
                            }

                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_dyn_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match &record.#field_ident {
                                    Some(s) => match s.repr() {
                                        ::boltffi::InternedStringRepr::Dynamic(d) => {
                                            unsafe {
                                                *ptr_out = d.as_ptr();
                                                *len_out = d.len();
                                            }
                                            1
                                        }
                                        _ => 0,
                                    },
                                    None => 0,
                                }
                            }
                        });
                    } else {
                        accessors.push(quote! {
                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #tag_sym(handle: *const ::core::ffi::c_void) -> u8 {
                                if handle.is_null() { return 0xff; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match record.#field_ident.repr() {
                                    ::boltffi::InternedStringRepr::Interned(_) => 0u8,
                                    ::boltffi::InternedStringRepr::Dynamic(_) => 1u8,
                                }
                            }

                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #id_sym(handle: *const ::core::ffi::c_void) -> u32 {
                                if handle.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match record.#field_ident.repr() {
                                    ::boltffi::InternedStringRepr::Interned(id) => *id,
                                    _ => 0,
                                }
                            }

                            #[allow(clippy::missing_safety_doc)]
                            #[unsafe(no_mangle)]
                            pub unsafe extern "C" fn #borrow_dyn_sym(
                                handle: *const ::core::ffi::c_void,
                                ptr_out: *mut *const u8,
                                len_out: *mut usize,
                            ) -> ::core::ffi::c_int {
                                if handle.is_null() || ptr_out.is_null() || len_out.is_null() { return 0; }
                                let record = unsafe { &*(handle as *const #record_ident) };
                                match record.#field_ident.repr() {
                                    ::boltffi::InternedStringRepr::Dynamic(d) => {
                                        unsafe {
                                            *ptr_out = d.as_ptr();
                                            *len_out = d.len();
                                        }
                                        1
                                    }
                                    _ => 0,
                                }
                            }
                        });
                    }
                }
                _ => {
                    return Err(Error::UnsupportedExpansion(
                        "unsupported native opaque record field type",
                    ));
                }
            }
        }

        Ok(quote! {
            #(#accessors)*
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

impl<'expansion, 'lowered, S: RenderSurface> EncodedField<'expansion, 'lowered, S> {
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
        let wire_size = self.wire_size(&field, &wire, codec)?;
        let encode_to = self.encode_to(&field, &wire, codec)?;
        let decode_from = self.decode_from(&field, &decoded, &used, &rust_type, codec)?;
        Ok(EncodedFieldTokens {
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

impl<'expansion, 'lowered, S> associated_fn::Owner<'expansion, 'lowered, S>
    for RecordOwner<'lowered>
where
    'lowered: 'expansion,
    S: RenderSurface,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::record(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::record_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        self.receiver
            .render(self.source, &self.record, &self.rust_type, export)
    }
}

impl<'receiver> ReceiverKind<'receiver> {
    fn render<'expansion, S>(
        self,
        source: &'receiver RecordDef,
        record: &TokenStream,
        rust_type: &Type,
        export: associated_fn::ReceiverExport<'expansion, 'receiver, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: RenderSurface,
        wrapper::param::direct::Record:
            Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'receiver, S>,
                Output = wrapper::param::Tokens,
            >,
        wrapper::returns::Failure: Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'receiver, S>,
                Output = TokenStream,
            >,
    {
        let receive = export.callable().receiver();
        let execution = export.callable().execution();
        let method = export.method().clone();
        let failure = export.failure();
        let expansion = export.expansion();
        match (self, receive) {
            (Self::Direct, Some(receive)) => {
                let receiver = names::Locals::new(method.span()).receiver();
                let requires_failure_return =
                    matches!(S::DIRECT_RECORD_PARAMS, DirectRecordCrossing::Pointer);
                let failure_token = if requires_failure_return {
                    failure.render()?
                } else {
                    TokenStream::new()
                };
                let tokens = <wrapper::param::direct::Record as Render<S, _>>::render(
                    wrapper::param::direct::Record,
                    wrapper::param::direct::RecordInput::new(
                        receive,
                        rust_type.clone(),
                        receiver.clone(),
                        failure_token,
                    ),
                )?;
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
                    export::ReceiverTokens::new(
                        ffi_parameters,
                        conversions,
                        writebacks,
                        requires_failure_return || direct_writeback.requires_failure_return,
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
                let tokens = <wrapper::param::encoded::Renderer as Render<S, _>>::render(
                    wrapper::param::encoded::Renderer,
                    wrapper::param::encoded::Input::new(
                        codec,
                        <S as SurfaceLower>::encoded_param_shape(),
                        decode_target,
                        receiver.clone(),
                        failure.render()?,
                        expansion,
                    ),
                )?;
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

    fn direct_writeback<'expansion, S>(
        self,
        receive: Receive,
        receiver: &Ident,
        rust_type: &Type,
        needs_writeback: bool,
        failure: associated_fn::ReceiverFailure<'expansion, 'receiver, S>,
    ) -> Result<ReceiverWriteback, Error>
    where
        S: RenderSurface,
        wrapper::returns::Failure: Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'receiver, S>,
                Output = TokenStream,
            >,
    {
        if receive != Receive::ByMutRef || !needs_writeback {
            return Ok(ReceiverWriteback::none());
        }
        let out = names::Parameter::new(receiver).writeback();
        let failure = failure.render()?;
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

    fn encoded_writeback<'expansion, S>(
        self,
        receive: Receive,
        codec: &'receiver WritePlan,
        receiver: &Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'receiver, S>,
        expansion: &'expansion Expansion<'receiver, S>,
    ) -> Result<ReceiverWriteback, Error>
    where
        S: RenderSurface,
        wrapper::returns::Failure: Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'receiver, S>,
                Output = TokenStream,
            >,
    {
        if receive != Receive::ByMutRef {
            return Ok(ReceiverWriteback::none());
        }
        let out = names::Parameter::new(receiver).writeback();
        let storage = names::Parameter::new(receiver).storage();
        let failure = failure.render()?;
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

fn primitive_rust_type(primitive: Primitive) -> TokenStream {
    match primitive {
        Primitive::Bool => quote! { bool },
        Primitive::I8 => quote! { i8 },
        Primitive::U8 => quote! { u8 },
        Primitive::I16 => quote! { i16 },
        Primitive::U16 => quote! { u16 },
        Primitive::I32 => quote! { i32 },
        Primitive::U32 => quote! { u32 },
        Primitive::I64 => quote! { i64 },
        Primitive::U64 => quote! { u64 },
        Primitive::ISize => quote! { isize },
        Primitive::USize => quote! { usize },
        Primitive::F32 => quote! { f32 },
        Primitive::F64 => quote! { f64 },
        _ => quote! { u8 },
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
