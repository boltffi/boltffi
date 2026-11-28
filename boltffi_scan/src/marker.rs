use boltffi_ast::ClassThreadSafety;
use syn::parse::Parser;

use crate::{ScanError, attributes};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Marker {
    Data,
    DataImpl,
    CustomFfi,
    Error,
    Export,
    Skip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Skip,
    Reject(Marker),
    Unmarked,
}

pub fn disposition(attrs: &[syn::Attribute]) -> Result<Disposition, ScanError> {
    Ok(match Marker::detect(attrs)? {
        Some(Marker::Skip) => Disposition::Skip,
        Some(marker) => Disposition::Reject(marker),
        None => Disposition::Unmarked,
    })
}

impl Marker {
    pub fn detect(attrs: &[syn::Attribute]) -> Result<Option<Self>, ScanError> {
        attrs.iter().try_fold(None, |detected: Option<Self>, attr| {
            let marker = Self::from_attribute(attr)?;
            match (detected, marker) {
                (Some(first), Some(second)) => Err(ScanError::ConflictingMarkers {
                    first: first.as_str().to_owned(),
                    second: second.as_str().to_owned(),
                }),
                (None, Some(marker)) => Ok(Some(marker)),
                (detected, None) => Ok(detected),
            }
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::DataImpl => "data(impl)",
            Self::CustomFfi => "custom_ffi",
            Self::Error => "error",
            Self::Export => "export",
            Self::Skip => "skip",
        }
    }

    pub fn invalid_placement(self, item: impl Into<String>) -> ScanError {
        ScanError::InvalidMarkerPlacement {
            marker: self.as_str().to_owned(),
            item: item.into(),
        }
    }

    fn from_attribute(attr: &syn::Attribute) -> Result<Option<Self>, ScanError> {
        match attributes::owned_name(attr) {
            Some("data") => Self::from_data(attr).map(Some),
            Some("custom_ffi") => Self::empty(attr, Self::CustomFfi).map(Some),
            Some("error") => Self::empty(attr, Self::Error).map(Some),
            Some("export") => Self::from_export(attr).map(Some),
            Some("skip") => Self::empty(attr, Self::Skip).map(Some),
            _ => Ok(None),
        }
    }

    fn empty(attr: &syn::Attribute, marker: Self) -> Result<Self, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(marker),
            _ => Err(invalid(attr)),
        }
    }

    fn from_data(attr: &syn::Attribute) -> Result<Self, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(Self::Data),
            syn::Meta::List(list) => parse_data_impl
                .parse2(list.tokens.clone())
                .map(|_| Self::DataImpl)
                .map_err(|_| invalid(attr)),
            _ => Err(invalid(attr)),
        }
    }

    fn from_export(attr: &syn::Attribute) -> Result<Self, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(Self::Export),
            syn::Meta::List(list) => parse_export_args
                .parse2(list.tokens.clone())
                .map(|_| Self::Export)
                .map_err(|_| invalid(attr)),
            _ => Err(invalid(attr)),
        }
    }
}

/// The arguments `#[export]` takes on a struct or enum that declares a class.
pub const CLASS_EXPORT_ARGUMENTS: [&str; 2] = ["single_threaded", "thread_unsafe"];

fn parse_data_impl(input: syn::parse::ParseStream<'_>) -> syn::Result<()> {
    input.parse::<syn::Token![impl]>()?;
    Ok(())
}

fn parse_export_args(input: syn::parse::ParseStream<'_>) -> syn::Result<ClassThreadSafety> {
    let args = syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated(input)?;
    if !args.is_empty()
        && args
            .iter()
            .all(|ident| CLASS_EXPORT_ARGUMENTS.iter().any(|known| ident == known))
    {
        Ok(ClassThreadSafety::UnsafeSingleThreaded)
    } else {
        Err(input.error("unsupported export marker arguments"))
    }
}

/// The thread safety an `#[export]` struct or enum asks for through its marker arguments.
pub(crate) fn class_thread_safety(
    tokens: proc_macro2::TokenStream,
) -> Result<ClassThreadSafety, ScanError> {
    if tokens.is_empty() {
        return Ok(ClassThreadSafety::default());
    }
    syn::parse::Parser::parse2(parse_export_args, tokens.clone()).map_err(|_| {
        ScanError::InvalidMarker {
            attribute: format!("export({tokens})"),
        }
        .at(&tokens)
    })
}

fn invalid(attr: &syn::Attribute) -> ScanError {
    ScanError::InvalidMarker {
        attribute: crate::spelling::attr(attr),
    }
    .at(attr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn struct_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemStruct>(source)
            .expect("valid struct")
            .attrs
    }

    fn impl_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemImpl>(source)
            .expect("valid impl")
            .attrs
    }

    fn fn_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemFn>(source)
            .expect("valid fn")
            .attrs
    }

    fn enum_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemEnum>(source)
            .expect("valid enum")
            .attrs
    }

    fn const_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemConst>(source)
            .expect("valid const")
            .attrs
    }

    #[test]
    fn detects_data_on_value_types() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[data] struct S { x: i32 }")),
            Ok(Some(Marker::Data))
        );
        assert_eq!(
            Marker::detect(&struct_attrs("#[boltffi::data] struct S { x: i32 }")),
            Ok(Some(Marker::Data))
        );
        assert_eq!(
            Marker::detect(&struct_attrs("struct S { x: i32 }")),
            Ok(None)
        );
        assert_eq!(
            Marker::detect(&struct_attrs("#[derive(Clone)] struct S { x: i32 }")),
            Ok(None)
        );
    }

    #[test]
    fn detects_custom_ffi_on_trait_impls() {
        assert_eq!(
            Marker::detect(&impl_attrs(
                "#[custom_ffi] impl CustomFfiConvertible for Email {}"
            )),
            Ok(Some(Marker::CustomFfi))
        );
        assert_eq!(
            Marker::detect(&impl_attrs(
                "#[boltffi::custom_ffi] impl boltffi::CustomFfiConvertible for Email {}"
            )),
            Ok(Some(Marker::CustomFfi))
        );
    }

    #[test]
    fn detects_data_impl_distinctly_from_data() {
        assert_eq!(
            Marker::detect(&impl_attrs("#[data(impl)] impl S {}")),
            Ok(Some(Marker::DataImpl))
        );
        assert_eq!(
            Marker::detect(&impl_attrs("#[boltffi::data(impl)] impl S {}")),
            Ok(Some(Marker::DataImpl))
        );
        assert_eq!(Marker::detect(&impl_attrs("impl S {}")), Ok(None));
    }

    #[test]
    fn rejects_unknown_marker_arguments() {
        assert_eq!(
            crate::error::unlocated(Marker::detect(&struct_attrs(
                "#[data(foo)] struct S { x: i32 }"
            ))),
            Err(ScanError::InvalidMarker {
                attribute: "data(foo)".to_owned()
            })
        );
        assert_eq!(
            crate::error::unlocated(Marker::detect(&fn_attrs("#[export(foo)] fn f() {}"))),
            Err(ScanError::InvalidMarker {
                attribute: "export(foo)".to_owned()
            })
        );
    }

    #[test]
    fn rejects_conflicting_markers() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[data] #[error] struct S { x: i32 }")),
            Err(ScanError::ConflictingMarkers {
                first: "data".to_owned(),
                second: "error".to_owned()
            })
        );
    }

    #[test]
    fn ignores_unowned_qualified_attributes() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[other::data] struct S { x: i32 }")),
            Ok(None)
        );
    }

    #[test]
    fn detects_error_on_value_types() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[error] struct E { code: i32 }")),
            Ok(Some(Marker::Error))
        );
        assert_eq!(
            Marker::detect(&enum_attrs("#[boltffi::error] enum E { Io, Parse }")),
            Ok(Some(Marker::Error))
        );
    }

    #[test]
    fn ignores_unqualified_error_helpers() {
        [
            "#[error(\"request failed\")]",
            "#[error(transparent)]",
            "#[error()]",
        ]
        .into_iter()
        .for_each(|helper| {
            assert_eq!(
                Marker::detect(&struct_attrs(&format!(
                    "{helper} struct ServiceError {{ message: String }}"
                ))),
                Ok(None),
            );
        });
    }

    #[test]
    fn detects_error_markers_alongside_thiserror_helpers() {
        ["error", "boltffi::error"].into_iter().for_each(|marker| {
            assert_eq!(
                Marker::detect(&struct_attrs(&format!(
                    "#[{marker}] #[derive(Debug, thiserror::Error)] \
                         #[error(\"request failed: {{message}}\")] \
                         struct ServiceError {{ message: String }}"
                ))),
                Ok(Some(Marker::Error)),
            );
        });
    }

    #[test]
    fn rejects_argument_bearing_qualified_error_markers() {
        ["foo", "\"request failed\"", ""]
            .into_iter()
            .for_each(|arguments| {
                assert_eq!(
                    crate::error::unlocated(Marker::detect(&struct_attrs(&format!(
                        "#[boltffi::error({arguments})] struct ServiceError {{ message: String }}"
                    )))),
                    Err(ScanError::InvalidMarker {
                        attribute: format!("boltffi::error({arguments})"),
                    }),
                );
            });
    }

    #[test]
    fn detects_export_on_exported_items() {
        assert_eq!(
            Marker::detect(&fn_attrs("#[export] fn f() {}")),
            Ok(Some(Marker::Export))
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[boltffi::export] fn f() {}")),
            Ok(Some(Marker::Export))
        );
        assert_eq!(Marker::detect(&fn_attrs("fn f() {}")), Ok(None));
        assert_eq!(
            Marker::detect(&const_attrs("#[export] const ANSWER: u32 = 42;")),
            Ok(Some(Marker::Export))
        );
    }

    #[test]
    fn detects_export_with_class_threading_arguments() {
        assert_eq!(
            Marker::detect(&impl_attrs("#[export(single_threaded)] impl S {}")),
            Ok(Some(Marker::Export))
        );
        assert_eq!(
            Marker::detect(&impl_attrs("#[boltffi::export(thread_unsafe)] impl S {}")),
            Ok(Some(Marker::Export))
        );
    }

    #[test]
    fn detects_skip_through_the_marker_set() {
        assert_eq!(
            Marker::detect(&fn_attrs("#[skip] fn f() {}")),
            Ok(Some(Marker::Skip))
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[boltffi::skip] fn f() {}")),
            Ok(Some(Marker::Skip))
        );
        assert_eq!(
            crate::error::unlocated(Marker::detect(&fn_attrs("#[skip(reason)] fn f() {}"))),
            Err(ScanError::InvalidMarker {
                attribute: "skip(reason)".to_owned()
            })
        );
    }
}
