use boltffi_ffi_rules::classification::PassableCategory;
use boltffi_scan::is_boltffi_data_marker;
use std::collections::HashMap;
use syn::{Item, ItemEnum, ItemStruct, Type};

use crate::data::analysis::{EnumDataShape, StructDataShape};
use crate::index::reexports::ReExport;
use crate::index::type_paths::TypePathKey;
use crate::index::{CrateIndex, IndexedCrateSource, PathResolver, SourceModule};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataTypeCategory {
    /// C-style enum, carrying the integer repr its discriminants cross as.
    Scalar(ScalarEnumRepr),
    Blittable,
    WireEncoded,
    NativeOpaque,
}

impl DataTypeCategory {
    pub fn supports_direct_vec(self) -> bool {
        matches!(self, Self::Blittable)
    }
}

/// Integer repr backing a C-style enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarEnumRepr {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    ISize,
    USize,
    /// A repr outside the set above, including targets added to Rust later.
    Other,
}

impl ScalarEnumRepr {
    fn from_ident(ident: &syn::Ident) -> Self {
        match ident.to_string().as_str() {
            "i8" => Self::I8,
            "u8" => Self::U8,
            "i16" => Self::I16,
            "u16" => Self::U16,
            "i32" => Self::I32,
            "u32" => Self::U32,
            "i64" => Self::I64,
            "u64" => Self::U64,
            "isize" => Self::ISize,
            "usize" => Self::USize,
            // Anything unrecognised must not be assumed narrow.
            _ => Self::Other,
        }
    }

    /// Whether every discriminant of this repr survives a round-trip through
    /// `f64`, which is how wasm carries an optional scalar.
    ///
    /// `isize`/`usize` are excluded because their width is target-dependent.
    /// This set must stay in sync with `Wasm32::scalar_option_enum` in
    /// `boltffi_binding`, which decides the same thing for the bindings side.
    pub fn fits_in_f64(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::U8 | Self::I16 | Self::U16 | Self::I32 | Self::U32
        )
    }
}

#[derive(Default, Clone)]
pub struct DataTypeRegistry {
    categories_by_path: HashMap<Vec<String>, DataTypeCategory>,
    unique_name_categories: HashMap<String, DataTypeCategory>,
    path_resolver: PathResolver,
}

impl DataTypeRegistry {
    fn insert(&mut self, qualified_path: Vec<String>, category: DataTypeCategory) {
        self.categories_by_path.insert(qualified_path, category);
    }

    fn category_for_segments(&self, path_segments: &[String]) -> Option<DataTypeCategory> {
        if path_segments.len() == 1 {
            return path_segments
                .first()
                .and_then(|name| self.unique_name_categories.get(name).copied());
        }

        if let Some(category) = self.categories_by_path.get(path_segments).copied() {
            return Some(category);
        }

        let mut matches = self
            .categories_by_path
            .iter()
            .filter(|(registered_path, _)| registered_path.ends_with(path_segments))
            .map(|(_, category)| *category);
        let first = matches.next()?;
        matches.all(|next| next == first).then_some(first)
    }

    fn finalize_unique_names(&mut self) {
        let name_counts = self.categories_by_path.keys().fold(
            HashMap::<String, usize>::new(),
            |mut counts, qualified_path| {
                if let Some(name) = qualified_path.last() {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                counts
            },
        );

        self.unique_name_categories = self.categories_by_path.iter().fold(
            HashMap::<String, DataTypeCategory>::new(),
            |mut unique, (qualified_path, category)| {
                if let Some(name) = qualified_path.last()
                    && name_counts.get(name).copied() == Some(1)
                {
                    unique.insert(name.clone(), *category);
                }
                unique
            },
        );
    }

    pub fn category_for(&self, ty: &Type) -> Option<DataTypeCategory> {
        let type_path_key = TypePathKey::from_type(ty)?;
        self.category_for_key(&type_path_key).or_else(|| {
            let resolved_type_path_key = self.resolved_type_path_key(ty)?;
            (resolved_type_path_key != type_path_key)
                .then(|| self.category_for_key(&resolved_type_path_key))
                .flatten()
        })
    }

    fn category_for_key(&self, type_path_key: &TypePathKey) -> Option<DataTypeCategory> {
        if type_path_key.is_single_segment() {
            return type_path_key
                .first_segment()
                .and_then(|name| self.unique_name_categories.get(name).copied());
        }

        if let Some(category) = self
            .categories_by_path
            .get(type_path_key.segments())
            .copied()
        {
            return Some(category);
        }

        let mut matches = self
            .categories_by_path
            .iter()
            .filter(|(registered_path, _)| type_path_key.has_suffix(registered_path))
            .map(|(_, category)| *category);
        let first = matches.next()?;
        matches.all(|next| next == first).then_some(first)
    }

    fn resolved_type_path_key(&self, ty: &Type) -> Option<TypePathKey> {
        match ty {
            Type::Path(type_path) if type_path.qself.is_none() => {
                let resolved_path = self.path_resolver.resolve(&type_path.path).into_path();
                Some(TypePathKey::from_path(&resolved_path))
            }
            Type::Group(group) => self.resolved_type_path_key(group.elem.as_ref()),
            Type::Paren(paren) => self.resolved_type_path_key(paren.elem.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
impl DataTypeRegistry {
    pub(crate) fn with_entries(entries: &[(&str, DataTypeCategory)]) -> Self {
        let mut registry = DataTypeRegistry::default();
        entries
            .iter()
            .map(|(path, category)| {
                (
                    path.split("::").map(str::to_string).collect::<Vec<_>>(),
                    *category,
                )
            })
            .for_each(|(segments, category)| registry.insert(segments, category));
        registry.finalize_unique_names();
        registry
    }

    pub(crate) fn with_entries_and_use_aliases(
        entries: &[(&str, DataTypeCategory)],
        aliases: &[(&str, &str)],
    ) -> Self {
        let mut registry = Self::with_entries(entries);
        registry.path_resolver = PathResolver::with_use_aliases(aliases);
        registry
    }
}

pub fn registry_for_current_crate() -> syn::Result<DataTypeRegistry> {
    Ok(CrateIndex::for_current_crate()?.data_types().clone())
}

pub(super) fn build_data_type_registry(
    sources: &[IndexedCrateSource],
    path_resolver: PathResolver,
) -> syn::Result<DataTypeRegistry> {
    let mut registry = DataTypeRegistry {
        path_resolver,
        ..DataTypeRegistry::default()
    };
    sources.iter().try_for_each(|source| {
        collect_root_types(source.root_path(), source.modules(), &mut registry)
    })?;

    registry.finalize_unique_names();
    sources.iter().try_for_each(|source| {
        collect_root_reexports(source.root_path(), source.modules(), &mut registry)
    })?;
    registry.finalize_unique_names();
    Ok(registry)
}

fn collect_root_types(
    root_path: &[String],
    source_modules: &[SourceModule],
    registry: &mut DataTypeRegistry,
) -> syn::Result<()> {
    source_modules.iter().try_for_each(|source_module| {
        let module_path = root_path
            .iter()
            .cloned()
            .chain(source_module.module_path().clone().into_strings())
            .collect::<Vec<_>>();
        let mut collector = DataTypeCollector {
            module_path,
            registry,
        };
        source_module
            .syntax()
            .items
            .iter()
            .try_for_each(|item| collector.collect_item(item))
    })
}

fn collect_root_reexports(
    root_path: &[String],
    source_modules: &[SourceModule],
    registry: &mut DataTypeRegistry,
) -> syn::Result<()> {
    source_modules.iter().try_for_each(|source_module| {
        let module_path = root_path
            .iter()
            .cloned()
            .chain(source_module.module_path().clone().into_strings())
            .collect::<Vec<_>>();
        let mut collector = DataTypeCollector {
            module_path,
            registry,
        };
        source_module
            .syntax()
            .items
            .iter()
            .try_for_each(|item| collector.collect_reexport_item(item))
    })
}

struct DataTypeCollector<'a> {
    module_path: Vec<String>,
    registry: &'a mut DataTypeRegistry,
}

impl<'a> DataTypeCollector<'a> {
    fn collect_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Struct(item_struct) => {
                self.collect_struct(item_struct);
                Ok(())
            }
            Item::Enum(item_enum) => {
                self.collect_enum(item_enum);
                Ok(())
            }
            Item::Mod(item_mod) => {
                let Some((_, items)) = &item_mod.content else {
                    return Ok(());
                };
                self.module_path.push(item_mod.ident.to_string());
                let collect_result = items
                    .iter()
                    .try_for_each(|nested| self.collect_item(nested));
                self.module_path.pop();
                collect_result
            }
            _ => Ok(()),
        }
    }

    fn collect_reexport_item(&mut self, item: &Item) -> syn::Result<()> {
        match item {
            Item::Use(_) => {
                ReExport::from_item(item)
                    .into_iter()
                    .for_each(|reexport| self.collect_reexport(reexport));
                Ok(())
            }
            Item::Mod(item_mod) => {
                let Some((_, items)) = &item_mod.content else {
                    return Ok(());
                };
                self.module_path.push(item_mod.ident.to_string());
                let collect_result = items
                    .iter()
                    .try_for_each(|nested| self.collect_reexport_item(nested));
                self.module_path.pop();
                collect_result
            }
            _ => Ok(()),
        }
    }

    fn collect_struct(&mut self, item_struct: &ItemStruct) {
        if !StructDataShape::new(item_struct).is_boltffi_data() {
            return;
        }
        let category = classify_struct_category(item_struct);
        let mut qualified_path = self.module_path.clone();
        qualified_path.push(item_struct.ident.to_string());
        self.registry.insert(qualified_path, category);
    }

    fn collect_enum(&mut self, item_enum: &ItemEnum) {
        if !EnumDataShape::new(item_enum).is_boltffi_data() {
            return;
        }
        let category = classify_enum_category(item_enum);
        let mut qualified_path = self.module_path.clone();
        qualified_path.push(item_enum.ident.to_string());
        self.registry.insert(qualified_path, category);
    }

    fn collect_reexport(&mut self, reexport: ReExport) {
        let Some(category) = self.registry.category_for_segments(reexport.target()) else {
            return;
        };
        self.registry.insert(
            self.module_path
                .iter()
                .cloned()
                .chain(std::iter::once(reexport.alias().to_string()))
                .collect(),
            category,
        );
    }
}

fn classify_struct_category(item_struct: &ItemStruct) -> DataTypeCategory {
    if item_struct.attrs.iter().any(|attr| {
        is_boltffi_data_marker(attr)
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|argument| argument == "opaque")
    }) {
        return DataTypeCategory::NativeOpaque;
    }

    match StructDataShape::new(item_struct).passable_category() {
        PassableCategory::Blittable => DataTypeCategory::Blittable,
        PassableCategory::Scalar | PassableCategory::WireEncoded => DataTypeCategory::WireEncoded,
    }
}

fn classify_enum_category(item_enum: &ItemEnum) -> DataTypeCategory {
    let enum_shape = EnumDataShape::new(item_enum);
    match enum_shape.passable_category() {
        PassableCategory::Scalar => DataTypeCategory::Scalar(ScalarEnumRepr::from_ident(
            &enum_shape.effective_integer_repr(),
        )),
        PassableCategory::Blittable | PassableCategory::WireEncoded => {
            DataTypeCategory::WireEncoded
        }
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::{
        DataTypeCategory, DataTypeCollector, DataTypeRegistry, ScalarEnumRepr,
        classify_struct_category,
    };

    fn registry_for(source: &str) -> DataTypeRegistry {
        let syntax = syn::parse_file(source).expect("valid source");
        let mut registry = DataTypeRegistry::default();
        let mut collector = DataTypeCollector {
            module_path: Vec::new(),
            registry: &mut registry,
        };
        syntax
            .items
            .iter()
            .try_for_each(|item| collector.collect_item(item))
            .expect("collect source");
        registry.finalize_unique_names();
        registry
    }

    #[test]
    fn opaque_data_struct_uses_native_opaque_category_for_owned_marker_paths_only() {
        for item in [
            parse_quote!(
                #[data(opaque)]
                struct Snapshot {
                    name: String,
                }
            ),
            parse_quote!(
                #[boltffi::data(opaque)]
                struct Snapshot {
                    name: String,
                }
            ),
        ] {
            assert_eq!(
                classify_struct_category(&item),
                DataTypeCategory::NativeOpaque
            );
        }

        let foreign: syn::ItemStruct = parse_quote!(
            #[other::data(opaque)]
            struct Snapshot {
                name: String,
            }
        );
        assert_ne!(
            classify_struct_category(&foreign),
            DataTypeCategory::NativeOpaque
        );
    }

    #[test]
    fn collector_recognizes_only_exact_data_and_error_marker_paths() {
        let registry = registry_for(
            r#"
            #[data(opaque)]
            struct ShortData { value: String }
            #[boltffi::data(opaque)]
            struct QualifiedData { value: String }
            #[error]
            struct ShortError { value: String }
            #[boltffi::error]
            struct QualifiedError { value: String }

            mod nested {
                #[other::data(opaque)]
                struct ForeignData { value: String }
                #[vendor::boltffi::data(opaque)]
                struct DeepData { value: String }
                #[other::error]
                struct ForeignError { value: String }
                #[vendor::boltffi::error]
                struct DeepError { value: String }
            }
            "#,
        );

        assert_eq!(
            registry.category_for(&parse_quote!(ShortData)),
            Some(DataTypeCategory::NativeOpaque)
        );
        assert_eq!(
            registry.category_for(&parse_quote!(QualifiedData)),
            Some(DataTypeCategory::NativeOpaque)
        );
        assert_eq!(
            registry.category_for(&parse_quote!(ShortError)),
            Some(DataTypeCategory::WireEncoded)
        );
        assert_eq!(
            registry.category_for(&parse_quote!(QualifiedError)),
            Some(DataTypeCategory::WireEncoded)
        );
        for ty in [
            parse_quote!(nested::ForeignData),
            parse_quote!(nested::DeepData),
            parse_quote!(nested::ForeignError),
            parse_quote!(nested::DeepError),
        ] {
            assert_eq!(registry.category_for(&ty), None);
        }
    }

    #[test]
    fn single_segment_unique_name_wins_over_unrelated_alias() {
        let registry = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "records::with_options::UserProfile",
                DataTypeCategory::WireEncoded,
            )],
            &[("UserProfile", "crate::records::UserProfile")],
        );

        assert_eq!(
            registry.category_for(&parse_quote!(UserProfile)),
            Some(DataTypeCategory::WireEncoded)
        );
    }

    #[test]
    fn short_module_path_still_resolves_through_alias() {
        let registry = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "core::parser::RouteProvider",
                DataTypeCategory::Scalar(ScalarEnumRepr::I32),
            )],
            &[("parser", "crate::core::parser")],
        );

        assert_eq!(
            registry.category_for(&parse_quote!(parser::RouteProvider)),
            Some(DataTypeCategory::Scalar(ScalarEnumRepr::I32))
        );
    }
}
