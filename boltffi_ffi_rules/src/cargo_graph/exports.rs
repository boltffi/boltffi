use std::fs;

use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Attribute, Item, Meta, Token};
use walkdir::WalkDir;

use super::ExportedPackage;

pub trait PackageExports {
    fn has_exports(&self) -> bool;
}

impl PackageExports for ExportedPackage {
    fn has_exports(&self) -> bool {
        WalkDir::new(self.source_root())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "rs")
            })
            .map(|entry| entry.path().to_path_buf())
            .any(|path| {
                fs::read_to_string(path)
                    .ok()
                    .and_then(|source| syn::parse_file(&source).ok())
                    .is_some_and(|syntax| syntax.has_exports())
            })
    }
}

struct AttributeCandidates {
    attrs: Vec<Meta>,
}

impl AttributeCandidates {
    fn from_attrs(attrs: &[Attribute]) -> Self {
        Self {
            attrs: attrs
                .iter()
                .flat_map(|attr| Self::expand(attr.meta.clone()))
                .collect(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = &Meta> {
        self.attrs.iter()
    }

    fn expand(meta: Meta) -> Vec<Meta> {
        if !meta.path().is_ident("cfg_attr") {
            return vec![meta];
        }
        ConditionalAttributes::from_meta(&meta)
            .map(|conditional| {
                conditional
                    .attrs
                    .into_iter()
                    .flat_map(Self::expand)
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct ConditionalAttributes {
    attrs: Vec<Meta>,
}

impl ConditionalAttributes {
    fn from_meta(meta: &Meta) -> Option<Self> {
        let Meta::List(list) = meta else {
            return None;
        };
        syn::parse2(list.tokens.clone()).ok()
    }
}

impl Parse for ConditionalAttributes {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Meta>()?;
        input.parse::<Token![,]>()?;
        let attrs = Punctuated::<Meta, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { attrs })
    }
}

trait SyntaxExports {
    fn has_exports(&self) -> bool;
}

impl SyntaxExports for syn::File {
    fn has_exports(&self) -> bool {
        self.items.iter().any(ItemExports::has_exports)
    }
}

trait ItemExports {
    fn has_exports(&self) -> bool;
}

impl ItemExports for Item {
    fn has_exports(&self) -> bool {
        match self {
            Self::Struct(item) => {
                item.attrs.has_any_marker(&["ffi_record", "data", "error"])
                    || item.attrs.derives_ffi_type()
            }
            Self::Enum(item) => item.attrs.has_any_marker(&["data", "error"]),
            Self::Impl(item) => {
                item.attrs
                    .has_any_marker(&["ffi_class", "export", "data", "custom_ffi"])
            }
            Self::Trait(item) => {
                item.attrs
                    .has_any_marker(&["ffi_trait", "ffi_callback", "export"])
            }
            Self::Fn(item) => item
                .attrs
                .has_any_marker(&["ffi_export", "ffi_func", "export"]),
            Self::Const(item) => item.attrs.has_any_marker(&["export"]),
            Self::Macro(item) => item
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "custom_type"),
            Self::Mod(item) => item
                .content
                .as_ref()
                .is_some_and(|(_, items)| items.iter().any(ItemExports::has_exports)),
            _ => false,
        }
    }
}

trait AttributeExports {
    fn has_any_marker(&self, names: &[&str]) -> bool;

    fn derives_ffi_type(&self) -> bool;
}

impl AttributeExports for [Attribute] {
    fn has_any_marker(&self, names: &[&str]) -> bool {
        AttributeCandidates::from_attrs(self).iter().any(|meta| {
            meta.path()
                .segments
                .last()
                .is_some_and(|segment| names.iter().any(|name| segment.ident == *name))
        })
    }

    fn derives_ffi_type(&self) -> bool {
        AttributeCandidates::from_attrs(self).iter().any(|meta| {
            if !meta.path().is_ident("derive") {
                return false;
            }
            let Meta::List(list) = meta else {
                return false;
            };
            Punctuated::<syn::Path, Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .ok()
                .is_some_and(|paths| {
                    paths.iter().any(|path| {
                        path.segments
                            .last()
                            .is_some_and(|segment| segment.ident == "FfiType")
                    })
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxExports;

    fn has_exports(source: &str) -> bool {
        syn::parse_file(source).expect("valid source").has_exports()
    }

    #[test]
    fn discovers_markers_nested_in_conditional_attributes() {
        assert!(has_exports(
            "mod api { #[cfg_attr(feature = \"ffi\", cfg_attr(unix, boltffi::data))] pub struct Signal; }"
        ));
    }

    #[test]
    fn discovers_multiple_wrapped_attributes() {
        assert!(has_exports(
            "#[cfg_attr(feature = \"ffi\", allow(dead_code), boltffi::export)] pub fn signal() {}"
        ));
    }

    #[test]
    fn discovers_established_export_syntax() {
        assert!(has_exports(
            "#[cfg_attr(feature = \"ffi\", boltffi::ffi_trait)] pub trait Callback {}"
        ));
        assert!(has_exports(
            "#[cfg_attr(feature = \"ffi\", boltffi::ffi_export)] pub fn signal() {}"
        ));
        assert!(has_exports("boltffi::custom_type!();"));
    }
}
