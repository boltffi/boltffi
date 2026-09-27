use boltffi_ast::ReprItem;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Item, ItemEnum, ItemStruct};

pub(crate) fn materialize(
    item: proc_macro::TokenStream,
) -> Result<proc_macro::TokenStream, proc_macro::TokenStream> {
    materialize_tokens(item.into())
        .map(Into::into)
        .map_err(|error| error.to_compile_error().into())
}

fn materialize_tokens(item: TokenStream) -> syn::Result<TokenStream> {
    let Ok(item) = syn::parse2::<Item>(item.clone()) else {
        return Ok(item);
    };
    match item {
        Item::Struct(item) => struct_repr(item),
        Item::Enum(item) => Ok(enum_repr(item)),
        item => Ok(quote!(#item)),
    }
}

fn struct_repr(mut item: ItemStruct) -> syn::Result<TokenStream> {
    reject_layout_changing_packing(&item)?;
    if lacks_repr(&item.attrs) {
        item.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
    }
    Ok(quote!(#item))
}

/// Packing that changes the layout sends a record down the wire encoder,
/// which takes references to fields: an error (E0793) on packed structs.
fn reject_layout_changing_packing(item: &ItemStruct) -> syn::Result<()> {
    let Some(packed) = item.attrs.iter().find(|attribute| {
        attribute.path().is_ident("repr")
            && boltffi_scan::scan_repr(std::slice::from_ref(attribute))
                .items
                .iter()
                .any(|item| matches!(item, ReprItem::Packed(_)))
    }) else {
        return Ok(());
    };
    let field_primitives = item.fields.iter().map(|field| match &field.ty {
        syn::Type::Path(type_path) if type_path.qself.is_none() => {
            boltffi_scan::primitive_path(&type_path.path)
        }
        _ => None,
    });
    if boltffi_binding::direct_record_fields(
        &boltffi_scan::scan_repr(&item.attrs),
        field_primitives,
    ) {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        packed,
        "#[data] supports repr(packed) only when packing leaves the repr(C) layout unchanged; remove the packed modifier",
    ))
}

fn enum_repr(mut item: ItemEnum) -> TokenStream {
    if lacks_repr(&item.attrs)
        && item
            .variants
            .iter()
            .all(|variant| variant.fields.is_empty())
    {
        item.attrs.insert(0, syn::parse_quote!(#[repr(i32)]));
    }
    quote!(#item)
}

fn lacks_repr(attrs: &[Attribute]) -> bool {
    !attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("repr"))
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::materialize_tokens;

    fn materialized(item: proc_macro2::TokenStream) -> String {
        materialize_tokens(item)
            .expect("item must materialize")
            .to_string()
    }

    #[test]
    fn bare_struct_gains_repr_c() {
        let tokens = materialized(quote! {
            #[derive(Clone, Copy)]
            pub struct Point {
                pub x: f64,
            }
        });

        assert!(tokens.starts_with("# [repr (C)]"));
    }

    #[test]
    fn struct_with_repr_keeps_written_repr() {
        let tokens = materialized(quote! {
            #[repr(transparent)]
            pub struct Meters {
                pub raw: f64,
            }
        });

        assert!(tokens.contains("# [repr (transparent)]"));
        assert!(!tokens.contains("repr (C)"));
    }

    #[test]
    fn unit_enum_gains_repr_i32() {
        let tokens = materialized(quote! {
            pub enum Direction {
                North,
                South,
            }
        });

        assert!(tokens.starts_with("# [repr (i32)]"));
    }

    #[test]
    fn payload_enum_keeps_written_layout() {
        let tokens = materialized(quote! {
            pub enum Shape {
                Dot,
                Line(f64),
            }
        });

        assert!(!tokens.contains("repr"));
    }

    #[test]
    fn layout_changing_packed_struct_is_rejected() {
        let error = materialize_tokens(quote! {
            #[repr(C, packed)]
            pub struct Packet { pub tag: u8, pub count: u32 }
        })
        .expect_err("packing that moves count must be rejected");

        assert!(error.to_string().contains("repr(packed)"));
    }

    #[test]
    fn layout_changing_packed_alignment_across_attributes_is_rejected() {
        materialize_tokens(quote! {
            #[repr(C)]
            #[repr(packed(2))]
            pub struct Packet { pub tag: u8, pub count: u32 }
        })
        .expect_err("packed(2) below the u32 alignment must be rejected");
    }

    #[test]
    fn packed_struct_with_non_primitive_field_is_rejected() {
        materialize_tokens(quote! {
            #[repr(C, packed)]
            pub struct Packet { pub tag: u8, pub name: String }
        })
        .expect_err("packing over a non-primitive field must be rejected");
    }

    #[test]
    fn packing_that_changes_nothing_is_accepted() {
        materialize_tokens(quote! {
            #[repr(C, packed(4))]
            pub struct Pair { pub left: u32, pub right: u16 }
        })
        .expect("packed(4) over u32 and u16 leaves the layout unchanged");
    }

    #[test]
    fn plain_repr_c_struct_is_accepted() {
        materialize_tokens(quote! {
            #[repr(C)]
            pub struct Point { pub x: f64, pub y: f64 }
        })
        .expect("plain repr(C) must expand");
    }

    #[test]
    fn unit_enum_with_repr_keeps_written_repr() {
        let tokens = materialized(quote! {
            #[repr(u8)]
            pub enum Flag {
                Off,
                On,
            }
        });

        assert!(tokens.contains("# [repr (u8)]"));
        assert!(!tokens.contains("repr (i32)"));
    }
}
