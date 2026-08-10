//! Rust-side emission for `#[data(opaque)]` native opaque records.
//!
//! An opaque record never crosses the wire as a serialized layout. The host
//! receives an owned `*mut c_void` handle, reads fields through the per-field
//! accessor exports generated here, and releases the box through the generated
//! destructor. Keeping the emission in one module isolates the `unsafe extern
//! "C"` surface from the ordinary encoded-record path.

use boltffi_ast::RecordDef;
use boltffi_binding::{EncodedRecordDecl, Native, Primitive};
use proc_macro2::TokenStream;
use quote::quote;

use crate::expansion::{error::Error, wrapper::names};

/// Emits the drop, dsize and per-field accessor exports for one opaque record.
pub(crate) fn render(
    source: &RecordDef,
    binding: &EncodedRecordDecl<Native>,
) -> Result<TokenStream, Error> {
    if !binding.initializers().is_empty() || !binding.methods().is_empty() {
        return Err(Error::UnsupportedExpansion(
            "native opaque record initializers and methods",
        ));
    }
    let record_ident = names::SourceSpelling::new(&source.name)
        .ident("source record name is not a Rust identifier")?;

    let exports = binding
        .native_opaque_exports()
        .ok_or(Error::UnsupportedExpansion(
            "native opaque record without helper exports",
        ))?;
    let drop_sym = quote::format_ident!("{}", exports.drop().name().as_str());
    let dsize_sym = quote::format_ident!("{}", exports.dsize().name().as_str());
    let dsize_terms = source
        .fields
        .iter()
        .zip(binding.fields().iter())
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
    for ((source_field, binding_field), field_exports) in source
        .fields
        .iter()
        .zip(binding.fields().iter())
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
                let prim_type = primitive_rust_type(primitive);
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

/// Maps a binding primitive to the Rust type used in the accessor signature.
fn primitive_rust_type(primitive: &Primitive) -> TokenStream {
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
