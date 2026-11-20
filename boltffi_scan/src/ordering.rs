//! The method `#[data(ord)]` adds to a record, which carries the struct's `Ord` across.

/// The inherent `impl` block `#[data(ord)]` adds to `item`: `compare_to(&self, other)`,
/// negative, zero or positive as `self` sorts before, with or after `other` by `Ord`.
///
/// The block carries the struct's `#[cfg]` attributes and no BoltFFI marker; the macro
/// exports it with `#[data(impl)]`, and the source scan reads it as if it did.
pub fn compare_impl(item: &syn::ItemStruct) -> syn::ItemImpl {
    let ident = &item.ident;
    let method = quote::format_ident!("{}", boltffi_ast::RecordDef::COMPARE_METHOD);
    let cfgs = item.attrs.iter().filter(|attr| attr.path().is_ident("cfg"));
    syn::parse_quote! {
        #(#cfgs)*
        impl #ident {
            /// Compares with `other` by the Rust type's `Ord`: negative, zero or positive
            /// as this value sorts before, with or after `other`.
            pub fn #method(&self, other: #ident) -> i32 {
                ::core::cmp::Ord::cmp(self, &other) as i32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn compares_through_ord_and_keeps_the_struct_cfg() {
        let item: syn::ItemStruct = syn::parse_quote! {
            #[cfg(feature = "versions")]
            #[derive(PartialEq, Eq, PartialOrd, Ord)]
            pub struct Version { pub major: u32 }
        };
        let expected: syn::ItemImpl = syn::parse_quote! {
            #[cfg(feature = "versions")]
            impl Version {
                /// Compares with `other` by the Rust type's `Ord`: negative, zero or positive
                /// as this value sorts before, with or after `other`.
                pub fn compare_to(&self, other: Version) -> i32 {
                    ::core::cmp::Ord::cmp(self, &other) as i32
                }
            }
        };
        assert_eq!(quote!(#expected).to_string(), {
            let actual = compare_impl(&item);
            quote!(#actual).to_string()
        });
    }
}
