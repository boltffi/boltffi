use syn::{Token, punctuated::Punctuated};

use boltffi_ast::{Primitive, ReprAttr, ReprItem};

pub fn scan(attrs: &[syn::Attribute]) -> ReprAttr {
    ReprAttr::new(
        attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("repr"))
            .flat_map(items)
            .collect(),
    )
}

fn items(attribute: &syn::Attribute) -> Vec<ReprItem> {
    attribute
        .parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        .map(|items| items.into_iter().map(item).collect())
        .unwrap_or_default()
}

fn item(meta: syn::Meta) -> ReprItem {
    match meta {
        syn::Meta::Path(path) => {
            path_item(&path).unwrap_or_else(|| ReprItem::Other(crate::spelling::path(&path)))
        }
        syn::Meta::List(list) => {
            let name = crate::spelling::path(&list.path);
            match name.as_str() {
                "packed" => ReprItem::Packed(int_arg(&list)),
                "align" => int_arg(&list)
                    .map(ReprItem::Align)
                    .unwrap_or_else(|| ReprItem::Other(format!("align({})", list.tokens))),
                _ => ReprItem::Other(format!("{}({})", name, list.tokens)),
            }
        }
        syn::Meta::NameValue(value) => ReprItem::Other(crate::spelling::path(&value.path)),
    }
}

fn path_item(path: &syn::Path) -> Option<ReprItem> {
    let ident = path.get_ident()?.to_string();
    Some(match ident.as_str() {
        "C" => ReprItem::C,
        "transparent" => ReprItem::Transparent,
        "packed" => ReprItem::Packed(None),
        "i8" => ReprItem::Primitive(Primitive::I8),
        "u8" => ReprItem::Primitive(Primitive::U8),
        "i16" => ReprItem::Primitive(Primitive::I16),
        "u16" => ReprItem::Primitive(Primitive::U16),
        "i32" => ReprItem::Primitive(Primitive::I32),
        "u32" => ReprItem::Primitive(Primitive::U32),
        "i64" => ReprItem::Primitive(Primitive::I64),
        "u64" => ReprItem::Primitive(Primitive::U64),
        "isize" => ReprItem::Primitive(Primitive::ISize),
        "usize" => ReprItem::Primitive(Primitive::USize),
        _ => return None,
    })
}

fn int_arg(list: &syn::MetaList) -> Option<u16> {
    syn::parse2::<syn::LitInt>(list.tokens.clone())
        .ok()
        .and_then(|literal| literal.base10_parse().ok())
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{Primitive, ReprItem};

    fn scan_struct(source: &str) -> Vec<ReprItem> {
        let item = syn::parse_str::<syn::ItemStruct>(source).expect("valid struct source");
        super::scan(&item.attrs).items
    }

    fn scan_enum(source: &str) -> Vec<ReprItem> {
        let item = syn::parse_str::<syn::ItemEnum>(source).expect("valid enum source");
        super::scan(&item.attrs).items
    }

    #[test]
    fn scans_c_and_alignment() {
        assert_eq!(
            scan_struct("#[repr(C, align(8))] struct Point { x: f64 }"),
            vec![ReprItem::C, ReprItem::Align(8)]
        );
    }

    #[test]
    fn scans_primitive_and_packing() {
        assert_eq!(
            scan_struct("#[repr(u8, packed(2))] struct Header { tag: u8 }"),
            vec![
                ReprItem::Primitive(Primitive::U8),
                ReprItem::Packed(Some(2))
            ]
        );
    }

    #[test]
    fn preserves_source_order_across_multiple_repr_attributes() {
        assert_eq!(
            scan_struct("#[repr(C)] #[repr(packed, align(16))] struct Header { tag: u8 }"),
            vec![ReprItem::C, ReprItem::Packed(None), ReprItem::Align(16)]
        );
    }

    #[test]
    fn scans_enum_repr_through_the_same_boundary() {
        assert_eq!(
            scan_enum("#[repr(i16)] enum Status { Ready, Busy }"),
            vec![ReprItem::Primitive(Primitive::I16)]
        );
    }

    #[test]
    fn preserves_unknown_repr_items_without_dropping_neighbors() {
        assert_eq!(
            scan_struct("#[repr(C, simd, align = 8, custom(foo))] struct Header { tag: u8 }"),
            vec![
                ReprItem::C,
                ReprItem::Other("simd".to_owned()),
                ReprItem::Other("align".to_owned()),
                ReprItem::Other("custom(foo)".to_owned())
            ]
        );
    }
}
