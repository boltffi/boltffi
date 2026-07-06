use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Item, ItemEnum, ItemStruct};

pub(crate) fn materialize(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    materialize_tokens(item.into()).into()
}

fn materialize_tokens(item: TokenStream) -> TokenStream {
    let Ok(item) = syn::parse2::<Item>(item.clone()) else {
        return item;
    };
    match item {
        Item::Struct(item) => struct_repr(item),
        Item::Enum(item) => enum_repr(item),
        item => quote!(#item),
    }
}

fn struct_repr(mut item: ItemStruct) -> TokenStream {
    if lacks_repr(&item.attrs) {
        item.attrs.insert(0, syn::parse_quote!(#[repr(C)]));
    }
    quote!(#item)
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
        materialize_tokens(item).to_string()
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
