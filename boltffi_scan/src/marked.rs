use crate::items;
use crate::marker::Marker;
use crate::source_tree::SourceTree;
use crate::{ModulePath, ModuleScope, ScanError};

pub(super) struct MarkedItems<'source> {
    records: Vec<Marked<'source, syn::ItemStruct>>,
    enums: Vec<Marked<'source, syn::ItemEnum>>,
    functions: Vec<Marked<'source, syn::ItemFn>>,
    traits: Vec<Marked<'source, syn::ItemTrait>>,
    classes: Vec<Marked<'source, syn::ItemImpl>>,
    constants: Vec<Marked<'source, syn::ItemConst>>,
    customs: Vec<MarkedCustom<'source>>,
    interned_string_pools: Vec<MarkedInternedStringPool<'source>>,
    impls: Vec<Marked<'source, syn::ItemImpl>>,
}

impl<'source> MarkedItems<'source> {
    pub(super) fn collect(tree: &'source SourceTree) -> Result<Self, ScanError> {
        tree.modules()
            .iter()
            .flat_map(|module| {
                module
                    .items()
                    .iter()
                    .map(move |item| (module.scope(), item))
            })
            .try_fold(Self::empty(), |mut marked, (scope, item)| {
                marked.push(scope, item)?;
                Ok(marked)
            })
    }

    fn empty() -> Self {
        Self {
            records: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            traits: Vec::new(),
            classes: Vec::new(),
            constants: Vec::new(),
            customs: Vec::new(),
            interned_string_pools: Vec::new(),
            impls: Vec::new(),
        }
    }

    pub(super) fn records(&self) -> &[Marked<'source, syn::ItemStruct>] {
        &self.records
    }

    pub(super) fn enums(&self) -> &[Marked<'source, syn::ItemEnum>] {
        &self.enums
    }

    pub(super) fn functions(&self) -> &[Marked<'source, syn::ItemFn>] {
        &self.functions
    }

    pub(super) fn traits(&self) -> &[Marked<'source, syn::ItemTrait>] {
        &self.traits
    }

    pub(super) fn classes(&self) -> &[Marked<'source, syn::ItemImpl>] {
        &self.classes
    }

    pub(super) fn constants(&self) -> &[Marked<'source, syn::ItemConst>] {
        &self.constants
    }

    pub(super) fn customs(&self) -> &[MarkedCustom<'source>] {
        &self.customs
    }

    pub(super) fn interned_string_pools(&self) -> &[MarkedInternedStringPool<'source>] {
        &self.interned_string_pools
    }

    pub(super) fn impls(&self) -> &[Marked<'source, syn::ItemImpl>] {
        &self.impls
    }

    fn push(
        &mut self,
        scope: &'source ModuleScope,
        item: &'source syn::Item,
    ) -> Result<(), ScanError> {
        if let Some(error) = items::misplaced_stream_marker(attrs(item), item_kind(item))? {
            return Err(error);
        }

        if let Some(item_macro) = custom_type_macro(item) {
            if let Some(marker) = Marker::detect(attrs(item))? {
                return Err(marker.invalid_placement(item_kind(item)));
            }
            self.customs.push(MarkedCustom::new(scope, item_macro));
            return Ok(());
        }

        if let Some(item_macro) = interned_string_pool_macro(item) {
            if let Some(marker) = Marker::detect(attrs(item))? {
                return Err(marker.invalid_placement(item_kind(item)));
            }
            self.interned_string_pools
                .push(MarkedInternedStringPool::new(scope, item_macro));
            return Ok(());
        }

        let Some(marker) = Marker::detect(attrs(item))? else {
            return Ok(());
        };
        match (marker, item) {
            (Marker::Data | Marker::Error, syn::Item::Struct(item)) => {
                self.records.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::Data | Marker::Error, syn::Item::Enum(item)) => {
                self.enums.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::DataImpl, syn::Item::Impl(item)) => {
                self.impls.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::CustomFfi, syn::Item::Impl(item)) => {
                self.customs.push(MarkedCustom::trait_impl(scope, item));
                Ok(())
            }
            (Marker::Export(export), syn::Item::Fn(item)) if !export.requires_class_impl() => {
                self.functions.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::Export(export), syn::Item::Trait(item)) if !export.requires_class_impl() => {
                self.traits.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::Export(_), syn::Item::Impl(item)) => {
                self.classes.push(Marked::new(scope, marker, item));
                Ok(())
            }
            (Marker::Export(export), syn::Item::Const(item)) if !export.requires_class_impl() => {
                self.constants.push(Marked::new(scope, marker, item));
                Ok(())
            }
            _ => Err(marker.invalid_placement(item_kind(item))),
        }
    }
}

pub(super) struct MarkedCustom<'source> {
    scope: &'source ModuleScope,
    item: MarkedCustomItem<'source>,
}

pub enum MarkedCustomItem<'source> {
    Macro(&'source syn::ItemMacro),
    TraitImpl(&'source syn::ItemImpl),
}

impl<'source> MarkedCustom<'source> {
    pub fn new(scope: &'source ModuleScope, item: &'source syn::ItemMacro) -> Self {
        Self {
            scope,
            item: MarkedCustomItem::Macro(item),
        }
    }

    pub fn trait_impl(scope: &'source ModuleScope, item: &'source syn::ItemImpl) -> Self {
        Self {
            scope,
            item: MarkedCustomItem::TraitImpl(item),
        }
    }

    pub(super) fn module(&self) -> &'source ModulePath {
        self.scope.path()
    }

    pub(super) fn scope(&self) -> &'source ModuleScope {
        self.scope
    }

    pub fn item(&self) -> &MarkedCustomItem<'source> {
        &self.item
    }

    pub fn attrs(&self) -> &'source [syn::Attribute] {
        match self.item {
            MarkedCustomItem::Macro(item) => &item.attrs,
            MarkedCustomItem::TraitImpl(item) => &item.attrs,
        }
    }

    pub fn span(&self) -> proc_macro2::Span {
        match self.item {
            MarkedCustomItem::Macro(item) => syn::spanned::Spanned::span(item),
            MarkedCustomItem::TraitImpl(item) => syn::spanned::Spanned::span(item),
        }
    }
}

pub(super) struct MarkedInternedStringPool<'source> {
    scope: &'source ModuleScope,
    item: &'source syn::ItemMacro,
}

impl<'source> MarkedInternedStringPool<'source> {
    fn new(scope: &'source ModuleScope, item: &'source syn::ItemMacro) -> Self {
        Self { scope, item }
    }

    pub(super) fn module(&self) -> &'source ModulePath {
        self.scope.path()
    }

    pub(super) fn item(&self) -> &'source syn::ItemMacro {
        self.item
    }
}

pub(super) struct Marked<'source, T> {
    scope: &'source ModuleScope,
    marker: Marker,
    item: &'source T,
}

impl<'source, T> Marked<'source, T> {
    fn new(scope: &'source ModuleScope, marker: Marker, item: &'source T) -> Self {
        Self {
            scope,
            marker,
            item,
        }
    }

    pub(super) fn module(&self) -> &'source ModulePath {
        self.scope.path()
    }

    pub(super) fn scope(&self) -> &'source ModuleScope {
        self.scope
    }

    pub(super) fn marker(&self) -> Marker {
        self.marker
    }

    pub(super) fn item(&self) -> &'source T {
        self.item
    }
}

fn custom_type_macro(item: &syn::Item) -> Option<&syn::ItemMacro> {
    let syn::Item::Macro(item_macro) = item else {
        return None;
    };
    custom_type_path(&item_macro.mac.path).then_some(item_macro)
}

fn interned_string_pool_macro(item: &syn::Item) -> Option<&syn::ItemMacro> {
    let syn::Item::Macro(item_macro) = item else {
        return None;
    };
    interned_string_pool_path(&item_macro.mac.path).then_some(item_macro)
}

fn custom_type_path(path: &syn::Path) -> bool {
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        [name] => path.leading_colon.is_none() && name == "custom_type",
        [namespace, name] => namespace == "boltffi" && name == "custom_type",
        _ => false,
    }
}

fn interned_string_pool_path(path: &syn::Path) -> bool {
    let segments = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        [name] => path.leading_colon.is_none() && name == "interned_string_pool",
        [namespace, name] => namespace == "boltffi" && name == "interned_string_pool",
        _ => false,
    }
}

fn attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn item_kind(item: &syn::Item) -> &'static str {
    match item {
        syn::Item::Const(_) => "const",
        syn::Item::Enum(_) => "enum",
        syn::Item::ExternCrate(_) => "extern crate",
        syn::Item::Fn(_) => "function",
        syn::Item::ForeignMod(_) => "foreign mod",
        syn::Item::Impl(_) => "impl",
        syn::Item::Macro(_) => "macro",
        syn::Item::Mod(_) => "module",
        syn::Item::Static(_) => "static",
        syn::Item::Struct(_) => "struct",
        syn::Item::Trait(_) => "trait",
        syn::Item::TraitAlias(_) => "trait alias",
        syn::Item::Type(_) => "type alias",
        syn::Item::Union(_) => "union",
        syn::Item::Use(_) => "use",
        _ => "item",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_tree::SourceTree;

    fn tree(source: &str) -> SourceTree {
        let file = syn::parse_str::<syn::File>(source).expect("valid source");
        SourceTree::in_memory("demo", file.items).expect("source tree")
    }

    #[test]
    fn collects_marked_items_by_domain_shape() {
        let tree = tree(
            "#[data] struct Point { x: i32 } \
             #[error] enum ParseError { Eof } \
             #[export] fn origin() {} \
             #[export] trait Listener { fn call(&self); } \
             #[export] impl Engine {} \
             #[export] const ANSWER: u32 = 42; \
             custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             #[custom_ffi] impl CustomFfiConvertible for Email { type FfiRepr = String; type Error = String; } \
             #[data(impl)] impl Point {}",
        );

        let marked = MarkedItems::collect(&tree).expect("marked items");

        assert_eq!(marked.records().len(), 1);
        assert_eq!(marked.records()[0].marker(), Marker::Data);
        assert_eq!(marked.enums().len(), 1);
        assert_eq!(marked.enums()[0].marker(), Marker::Error);
        assert_eq!(marked.functions().len(), 1);
        assert_eq!(marked.traits().len(), 1);
        assert_eq!(marked.classes().len(), 1);
        assert_eq!(marked.constants().len(), 1);
        assert_eq!(marked.customs().len(), 2);
        assert_eq!(marked.impls().len(), 1);
    }

    #[test]
    fn rejects_marker_on_wrong_item_kind() {
        let tree = tree("#[export] struct Point { x: i32 }");

        let error = match MarkedItems::collect(&tree) {
            Ok(_) => panic!("wrong marker placement must reject"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "struct".to_owned()
            }
        );
    }

    #[test]
    fn rejects_class_export_options_on_non_class_items() {
        let tree = tree("#[export(single_threaded)] fn answer() -> u32 { 42 }");

        let error = match MarkedItems::collect(&tree) {
            Ok(_) => panic!("class-only export option must reject on functions"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "function".to_owned()
            }
        );
    }

    #[test]
    fn rejects_marker_on_custom_type_macro() {
        let tree = tree(
            "#[export] custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);",
        );

        let error = match MarkedItems::collect(&tree) {
            Ok(_) => panic!("marked custom type macro must reject"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "macro".to_owned()
            }
        );
    }

    #[test]
    fn rejects_stream_marker_on_top_level_items() {
        let tree = tree("#[ffi_stream(item = i32)] fn values() {}");

        let error = match MarkedItems::collect(&tree) {
            Ok(_) => panic!("top-level stream marker must reject"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "ffi_stream".to_owned(),
                item: "function".to_owned()
            }
        );
    }

    #[test]
    fn collects_owned_qualified_custom_type_macro() {
        let tree = tree(
            "boltffi::custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);",
        );

        let marked = MarkedItems::collect(&tree).expect("marked items");

        assert_eq!(marked.customs().len(), 1);
    }

    #[test]
    fn ignores_unowned_qualified_custom_type_macro() {
        let tree = tree(
            "other::custom_type!(UtcDateTime, remote = DateTime<Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis);",
        );

        let marked = MarkedItems::collect(&tree).expect("marked items");

        assert_eq!(marked.customs().len(), 0);
    }
}
