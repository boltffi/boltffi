//! Scans one macro invocation's item without whole-crate knowledge.
//!
//! Ids come out as `$self::Name` placeholders and every named type reference becomes a
//! `$slot:N` leaf paired with the written path in [`CapturedItem::slots`], for the macro
//! to project through the compiler at the invocation site.

use boltffi_ast::{EnumDef, FunctionDef, RecordDef};

use crate::declared_types::DeclaredTypes;
use crate::path::{ModulePath, ModuleScope};
use crate::{ScanError, items};

/// One item scanned in deferred-resolution mode.
pub struct CapturedItem<Def> {
    /// The declaration with placeholder ids and slot leaves.
    pub def: Def,
    /// Written paths of the deferred references, in slot-index order.
    pub slots: Vec<syn::Path>,
}

/// Captures a `#[data]` struct as a record definition.
pub fn capture_struct(item: &syn::ItemStruct) -> Result<CapturedItem<RecordDef>, ScanError> {
    captured(|scope, declared_types| items::record::scan_item(item, scope, declared_types))
}

/// Captures a `#[data]` enum as an enum definition.
pub fn capture_enum(item: &syn::ItemEnum) -> Result<CapturedItem<EnumDef>, ScanError> {
    captured(|scope, declared_types| items::enumeration::scan_item(item, scope, declared_types))
}

/// Captures an `#[export]` free function as a function definition.
pub fn capture_function(item: &syn::ItemFn) -> Result<CapturedItem<FunctionDef>, ScanError> {
    captured(|scope, declared_types| items::function::scan_item(item, scope, declared_types))
}

fn captured<Def>(
    scan: impl FnOnce(&ModuleScope, &DeclaredTypes) -> Result<Def, ScanError>,
) -> Result<CapturedItem<Def>, ScanError> {
    let scope = ModuleScope::with_spans(ModulePath::root("$self"), &[], None);
    let mut declared_types = DeclaredTypes::deferred();
    let def = scan(&scope, &declared_types)?;
    Ok(CapturedItem {
        def,
        slots: declared_types.take_slots(),
    })
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{ReturnDef, TypeExpr};
    use quote::ToTokens;

    use super::*;

    #[test]
    fn captures_a_struct_with_deferred_references() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Route {
                pub start: Point,
                pub stops: Vec<geometry::Point>,
                pub label: String,
            }
        };

        let captured = capture_struct(&item).expect("struct captures");

        assert_eq!(captured.def.id.as_str(), "$self::Route");
        assert_eq!(
            captured.slots.len(),
            2,
            "each distinct written path gets one slot"
        );
        assert_eq!(
            captured.slots[0].to_token_stream().to_string(),
            "Point",
            "slots keep the written spelling"
        );
        assert_eq!(
            captured.slots[1].to_token_stream().to_string(),
            "geometry :: Point"
        );
        assert!(
            matches!(
                &captured.def.fields[0].type_expr,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "named references defer to their slot"
        );
        assert!(
            matches!(
                &captured.def.fields[1].type_expr,
                TypeExpr::Vec(inner)
                    if matches!(inner.as_ref(), TypeExpr::Record { id, .. } if id.as_str() == "$slot:1")
            ),
            "container spellings stay structural around the slot"
        );
        assert!(
            matches!(&captured.def.fields[2].type_expr, TypeExpr::String),
            "standard leaves never take a slot"
        );
    }

    #[test]
    fn captures_repeated_references_into_one_slot() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Segment {
                pub from: Point,
                pub to: Point,
            }
        };

        let captured = capture_struct(&item).expect("struct captures");

        assert_eq!(captured.slots.len(), 1, "identical spellings share a slot");
        assert!(
            matches!(
                &captured.def.fields[1].type_expr,
                TypeExpr::Record { id, .. } if id.as_str() == "$slot:0"
            ),
            "the second reference reuses the first slot"
        );
    }

    #[test]
    fn captures_an_enum_with_payload_references() {
        let item: syn::ItemEnum = syn::parse_quote! {
            pub enum Shape {
                Empty,
                At(Point),
            }
        };

        let captured = capture_enum(&item).expect("enum captures");

        assert_eq!(captured.def.id.as_str(), "$self::Shape");
        assert_eq!(captured.slots.len(), 1);
    }

    #[test]
    fn captures_a_function_signature() {
        let item: syn::ItemFn = syn::parse_quote! {
            pub fn nearest(route: Route, count: u32) -> Option<Point> {
                unimplemented!()
            }
        };

        let captured = capture_function(&item).expect("function captures");

        assert_eq!(captured.def.id.as_str(), "$self::nearest");
        assert_eq!(captured.slots.len(), 2, "Route and Point defer");
        assert!(
            matches!(
                &captured.def.returns,
                ReturnDef::Value(TypeExpr::Option(inner))
                    if matches!(inner.as_ref(), TypeExpr::Record { id, .. } if id.as_str() == "$slot:1")
            ),
            "the return type defers through the option"
        );
    }

    #[test]
    fn rejects_generics_loudly() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Wrapper<T> {
                pub inner: T,
            }
        };

        assert!(
            capture_struct(&item).is_err(),
            "generic items stay unsupported"
        );
    }
}
