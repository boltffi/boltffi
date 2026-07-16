use proc_macro2::{Literal, TokenStream};
use quote::quote;
use syn::ItemStruct;

use crate::parse::Record;
use crate::payload;

/// The type declares its own id, and the record's slots ask the compiler for everyone else's.
pub fn item(item: &ItemStruct, record: &Record) -> syn::Result<TokenStream> {
    let json = payload::json(record).map_err(|error| {
        syn::Error::new_spanned(item, format!("pim: cannot serialize the record: {error}"))
    })?;

    let json = Literal::byte_string(&json);
    let name = &item.ident;
    let name_literal = record.name.as_str();
    let slots = record.slots.types();
    let mach_o_section = pim_runtime::MACH_O_SECTION;
    let object_section = pim_runtime::OBJECT_SECTION;

    Ok(quote! {
        #item

        impl<Tag> ::pim_runtime::TypeInfo<Tag> for #name {
            const MODULE: &'static str = ::core::module_path!();
            const NAME: &'static str = #name_literal;
        }

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
    })
}
