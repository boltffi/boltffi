use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::{ItemStruct, Type};

use crate::parse::Record;
use crate::payload;

/// The type declares its own id, and the record's slots ask the compiler for everyone else's.
pub fn item(item: &ItemStruct, record: &Record) -> syn::Result<TokenStream> {
    let json = payload::json(record).map_err(|error| {
        syn::Error::new_spanned(item, format!("pim: cannot serialize the record: {error}"))
    })?;

    let name = &item.ident;
    let name_literal = record.name.as_str();
    let metadata = metadata_block(&json, record.slots.types());
    let codecs = codecs(item, record);

    Ok(quote! {
        #item

        impl<Tag> ::pim_runtime::TypeInfo<Tag> for #name {
            const MODULE: &'static str = ::core::module_path!();
            const NAME: &'static str = #name_literal;
        }

        #codecs
        #metadata
    })
}

/// The status-carrying wrapper body: report success, run under `catch_unwind`, and turn a
/// panic into a status code plus the poison value.
pub fn trapped(body: &TokenStream, poison: &TokenStream) -> TokenStream {
    quote! {
        unsafe { *__pim_status = ::pim_runtime::CallStatus::success() };
        match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(move || {
            #body
        })) {
            Ok(__pim_out) => __pim_out,
            Err(__pim_panic) => {
                unsafe { (*__pim_status).fail(::pim_runtime::panic_message(__pim_panic)) };
                #poison
            }
        }
    }
}

/// One `#[used]` static per invocation; the module path is whatever `module_path!` says here.
pub fn metadata_block(json: &[u8], slots: &[Type]) -> TokenStream {
    let json = Literal::byte_string(json);
    let mach_o_section = pim_runtime::MACH_O_SECTION;
    let object_section = pim_runtime::OBJECT_SECTION;

    quote! {
        const _: () = {
            const MODULE: &str = ::core::module_path!();
            const SLOTS: &[&str] = &[
                #(
                    <#slots as ::pim_runtime::TypeInfo<crate::PimTag>>::MODULE,
                    <#slots as ::pim_runtime::TypeInfo<crate::PimTag>>::NAME,
                )*
            ];
            const JSON: &[u8] = #json;
            const LEN: usize = ::pim_runtime::record_len(MODULE, SLOTS, JSON);

            #[cfg_attr(any(target_os = "macos", target_os = "ios"), unsafe(link_section = #mach_o_section))]
            #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), unsafe(link_section = #object_section))]
            #[used]
            static METADATA: [u8; LEN] = ::pim_runtime::record::<LEN>(MODULE, SLOTS, JSON);
        };
    }
}

/// Bounds are `where` clauses, so a field without an encoding only errors where a value crosses.
fn codecs(item: &ItemStruct, record: &Record) -> TokenStream {
    let name = &item.ident;
    let field_idents = item
        .fields
        .iter()
        .map(|field| {
            field
                .ident
                .as_ref()
                .expect("named fields carry an identifier")
        })
        .collect::<Vec<_>>();
    let field_types = item
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let codec = if record.direct {
        quote! {
            impl<Tag> ::pim_runtime::Codec<Tag> for #name {
                type FfiType = Self;
                fn lower(self) -> Self {
                    self
                }
                fn lift(ffi: Self) -> Self {
                    ffi
                }
                // Sound because `direct` requires `repr(C)` with all-primitive fields.
                fn poisoned() -> Self {
                    unsafe { ::core::mem::zeroed() }
                }
            }
        }
    } else {
        quote! {
            impl<Tag> ::pim_runtime::Codec<Tag> for #name
            where
                #name: ::pim_runtime::Encode<Tag>,
            {
                type FfiType = ::pim_runtime::RawBuffer;

                fn lower(self) -> ::pim_runtime::RawBuffer {
                    let mut bytes = ::std::vec::Vec::new();
                    ::pim_runtime::Encode::<Tag>::write(&self, &mut bytes);
                    ::pim_runtime::RawBuffer::from_vec(bytes)
                }

                fn lift(ffi: ::pim_runtime::RawBuffer) -> Self {
                    let bytes = ffi.into_vec();
                    <Self as ::pim_runtime::Encode<Tag>>::read(&mut bytes.as_slice())
                }

                fn poisoned() -> ::pim_runtime::RawBuffer {
                    ::pim_runtime::RawBuffer::null()
                }
            }
        }
    };

    quote! {
        impl<Tag> ::pim_runtime::Encode<Tag> for #name
        where
            #(#field_types: ::pim_runtime::Encode<Tag>,)*
        {
            fn write(&self, out: &mut ::std::vec::Vec<u8>) {
                #(::pim_runtime::Encode::<Tag>::write(&self.#field_idents, out);)*
            }

            fn read(input: &mut &[u8]) -> Self {
                Self {
                    #(#field_idents: <#field_types as ::pim_runtime::Encode<Tag>>::read(input),)*
                }
            }
        }

        #codec
    }
}
