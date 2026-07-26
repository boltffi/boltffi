use std::collections::{HashMap, HashSet};
use std::path::Path as FsPath;

use boltffi_ast::{PackageInfo, Path, PathRoot, PathSegment, SourceContract};

use crate::declared_types::DeclaredTypes;
use crate::input::ScanInput;
use crate::marked::MarkedItems;
use crate::package_graph::{ExportedPackage, LoadError, PackageGraph};
use crate::path::ImportLookup;
use crate::source_tree::SourceTree;
use crate::{ModuleScope, ScanError, items};

pub fn scan(input: &ScanInput) -> Result<SourceContract, ScanError> {
    let source_tree = SourceTree::load_with_cfg(input.root(), &input.package().name, input.cfg())?;
    scan_tree(source_tree, input.package().clone())
}

pub struct PackageScan {
    root: SourceContract,
    complete: SourceContract,
    root_visible_paths: HashMap<String, Path>,
}

impl PackageScan {
    pub fn root_visible_paths(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.root_visible_paths
            .iter()
            .map(|(id, path)| (id.as_str(), path))
    }

    pub fn root(&self) -> &SourceContract {
        &self.root
    }

    pub fn complete(&self) -> &SourceContract {
        &self.complete
    }

    pub fn root_with_support(&self) -> SourceContract {
        let root = self.root_crate();
        let mut source = self.root.clone();
        source.records = self
            .complete
            .records
            .iter()
            .cloned()
            .map(|mut record| {
                if !self.exposes_support_methods(&root, record.id.as_str()) {
                    record.methods.clear();
                }
                record
            })
            .collect();
        source.enums = self
            .complete
            .enums
            .iter()
            .cloned()
            .map(|mut enumeration| {
                if !self.exposes_support_methods(&root, enumeration.id.as_str()) {
                    enumeration.methods.clear();
                }
                enumeration
            })
            .collect();
        source.classes = self.complete.classes.clone();
        source.traits = self.complete.traits.clone();
        source.customs = self.complete.customs.clone();
        source.functions = self
            .complete
            .functions
            .iter()
            .filter(|function| {
                root.owns(function.id.as_str())
                    || self.root_visible_paths.contains_key(function.id.as_str())
            })
            .cloned()
            .collect();
        source
    }

    pub fn into_complete(self) -> SourceContract {
        self.complete
    }

    fn root_crate(&self) -> RootCrate {
        RootCrate::new(&self.root.package.name)
    }

    fn exposes_support_methods(&self, root: &RootCrate, id: &str) -> bool {
        root.owns(id) || self.root_visible_paths.contains_key(id)
    }
}

struct RootCrate {
    name: String,
    prefix: String,
}

impl RootCrate {
    fn new(name: &str) -> Self {
        let name = name.replace('-', "_");
        Self {
            prefix: format!("{name}::"),
            name,
        }
    }

    fn owns(&self, id: &str) -> bool {
        id == self.name || id.starts_with(&self.prefix)
    }
}

pub fn scan_package(input: &ScanInput) -> Result<PackageScan, ScanError> {
    let root_tree = SourceTree::load_with_cfg(input.root(), &input.package().name, input.cfg())?;
    let dependencies = dependencies(input.manifest_dir())?;
    let direct_dependency_modules = dependencies.direct_modules();
    let complete_tree = SourceTree::combine(
        dependencies
            .reachable
            .into_iter()
            .chain(std::iter::once(root_tree.clone())),
    );
    let root_marked = MarkedItems::collect(&root_tree)?;
    let complete_marked = MarkedItems::collect(&complete_tree)?;
    let declared_types = DeclaredTypes::index(&complete_tree, &complete_marked)?;
    let root =
        scan_marked_with_declarations(&root_marked, &declared_types, input.package().clone())?;
    let complete =
        scan_marked_with_declarations(&complete_marked, &declared_types, input.package().clone())?;
    let root_visible_paths = root_visible_paths(
        &declared_types,
        &complete_tree,
        &complete_marked,
        &input.package().name,
        &direct_dependency_modules,
    );
    Ok(PackageScan {
        root,
        complete,
        root_visible_paths,
    })
}

pub fn scan_source(
    path: impl AsRef<FsPath>,
    package: PackageInfo,
) -> Result<SourceContract, ScanError> {
    let source_tree = SourceTree::load(path.as_ref(), &package.name)?;
    scan_tree(source_tree, package)
}

pub fn scan_file(file: syn::File, package: PackageInfo) -> Result<SourceContract, ScanError> {
    let source_tree = SourceTree::inline(&package.name, file)?;
    scan_tree(source_tree, package)
}

fn scan_tree(source_tree: SourceTree, package: PackageInfo) -> Result<SourceContract, ScanError> {
    scan_tree_with_declarations(&source_tree, &source_tree, package)
}

fn scan_tree_with_declarations(
    source_tree: &SourceTree,
    declaration_tree: &SourceTree,
    package: PackageInfo,
) -> Result<SourceContract, ScanError> {
    let marked = MarkedItems::collect(source_tree)?;
    let declaration_marked = MarkedItems::collect(declaration_tree)?;
    let declared_types = DeclaredTypes::index(declaration_tree, &declaration_marked)?;
    scan_marked_with_declarations(&marked, &declared_types, package)
}

fn scan_marked_with_declarations(
    marked: &MarkedItems<'_>,
    declared_types: &DeclaredTypes,
    package: PackageInfo,
) -> Result<SourceContract, ScanError> {
    let classes = items::class::scan(marked.classes(), declared_types)?;
    let mut records = scan_each(marked.records(), declared_types, items::record::scan)?;
    let mut enums = scan_each(marked.enums(), declared_types, items::enumeration::scan)?;
    let traits = scan_each(marked.traits(), declared_types, items::callback::scan)?;
    let customs = scan_each(marked.customs(), declared_types, items::custom_type::scan)?;
    let streams = items::stream::scan(marked.classes(), declared_types)?;
    items::impl_block::attach_methods(marked.impls(), declared_types, &mut records, &mut enums)?;
    let functions = scan_each(marked.functions(), declared_types, items::function::scan)?;
    let constants = scan_each(marked.constants(), declared_types, items::constant::scan)?;

    let mut contract = SourceContract::new(package);
    contract.records = records;
    contract.enums = enums;
    contract.classes = classes;
    contract.traits = traits;
    contract.streams = streams;
    contract.functions = functions;
    contract.constants = constants;
    contract.customs = customs;
    Ok(contract)
}

struct PackageDependencies {
    direct: Vec<ExportedPackage>,
    reachable: Vec<SourceTree>,
}

impl PackageDependencies {
    fn empty() -> Self {
        Self {
            direct: Vec::new(),
            reachable: Vec::new(),
        }
    }

    fn direct_modules(&self) -> Vec<String> {
        self.direct
            .iter()
            .map(|package| package.module_name().to_owned())
            .collect()
    }
}

fn dependencies(manifest_dir: Option<&FsPath>) -> Result<PackageDependencies, ScanError> {
    let Some(manifest_dir) = manifest_dir else {
        return Ok(PackageDependencies::empty());
    };
    let Some(graph) = PackageGraph::load(manifest_dir).map_err(package_graph_error)? else {
        return Ok(PackageDependencies::empty());
    };
    let direct = graph.direct_exported_dependencies(graph.root_id());
    let reachable = graph
        .reachable_exported_dependencies(graph.root_id())
        .into_iter()
        .map(dependency_tree)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PackageDependencies { direct, reachable })
}

fn dependency_tree(package: ExportedPackage) -> Result<SourceTree, ScanError> {
    SourceTree::load(package.source_file(), package.module_name())
}

fn package_graph_error(error: LoadError) -> ScanError {
    ScanError::PackageGraph {
        message: error.to_string(),
    }
}

fn root_visible_paths(
    declared_types: &DeclaredTypes,
    source_tree: &SourceTree,
    marked: &MarkedItems<'_>,
    root_crate: &str,
    direct_dependencies: &[String],
) -> HashMap<String, Path> {
    let mut paths = declared_types
        .paths()
        .filter_map(|id| {
            declared_types
                .root_visible_path(id, root_crate, direct_dependencies)
                .map(|path| (id.to_owned(), contract_path(root_crate, &path)))
        })
        .collect::<HashMap<_, _>>();
    paths.extend(
        FunctionPaths::new(source_tree, marked.functions())
            .root_visible_paths(root_crate, direct_dependencies)
            .map(|(id, path)| (id, contract_path(root_crate, &path))),
    );
    paths
}

struct FunctionPaths<'source> {
    by_path: HashSet<String>,
    scopes: HashMap<String, &'source ModuleScope>,
}

impl<'source> FunctionPaths<'source> {
    fn new(
        source_tree: &'source SourceTree,
        functions: &[crate::marked::Marked<'source, syn::ItemFn>],
    ) -> Self {
        let by_path = functions
            .iter()
            .map(|function| {
                function
                    .scope()
                    .path()
                    .qualified(&function.item().sig.ident.to_string())
            })
            .collect();
        let scopes = source_tree
            .modules()
            .iter()
            .map(|module| (module.scope().path().segments().join("::"), module.scope()))
            .collect();
        Self { by_path, scopes }
    }

    fn root_visible_paths<'dependency>(
        &'source self,
        root_crate: &'dependency str,
        direct_dependencies: &'dependency [String],
    ) -> impl Iterator<Item = (String, String)> + 'dependency
    where
        'source: 'dependency,
    {
        self.by_path.iter().filter_map(move |id| {
            self.root_visible_path(id, root_crate, direct_dependencies)
                .map(|path| (id.clone(), path))
        })
    }

    fn root_visible_path(
        &self,
        id: &str,
        root_crate: &str,
        direct_dependencies: &[String],
    ) -> Option<String> {
        let segments = id.split("::").collect::<Vec<_>>();
        let root = segments.first().copied()?;
        if root == root_crate
            || direct_dependencies
                .iter()
                .any(|dependency| dependency == root)
        {
            return Some(id.to_owned());
        }
        let leaf = segments.last().copied()?;
        direct_dependencies.iter().find_map(|dependency| {
            let candidate = format!("{dependency}::{leaf}");
            (self.resolve_public_path(&candidate).as_deref() == Some(id)).then_some(candidate)
        })
    }

    fn resolve_public_path(&self, path: &str) -> Option<String> {
        self.by_path
            .contains(path)
            .then(|| path.to_owned())
            .or_else(|| self.resolve_reexported(path, &mut HashSet::new()))
    }

    fn resolve_reexported(&self, path: &str, visited: &mut HashSet<String>) -> Option<String> {
        if !visited.insert(path.to_owned()) {
            return None;
        }
        let segments = path.split("::").map(ToOwned::to_owned).collect::<Vec<_>>();
        let (name, module_segments) = segments.split_last()?;
        let scope = self.scopes.get(&module_segments.join("::"))?;
        self.resolve_explicit_reexport(scope, name, visited)
            .or_else(|| self.resolve_glob_reexport(scope, name, visited))
    }

    fn resolve_explicit_reexport(
        &self,
        scope: &ModuleScope,
        name: &str,
        visited: &mut HashSet<String>,
    ) -> Option<String> {
        match scope.reexported(name) {
            ImportLookup::Unique(imported) => {
                let candidate = imported.join("::");
                self.by_path
                    .contains(&candidate)
                    .then(|| candidate.clone())
                    .or_else(|| self.resolve_reexported(&candidate, visited))
            }
            ImportLookup::None | ImportLookup::Ambiguous => None,
        }
    }

    fn resolve_glob_reexport(
        &self,
        scope: &ModuleScope,
        name: &str,
        visited: &mut HashSet<String>,
    ) -> Option<String> {
        let mut matches = scope
            .reexport_glob_candidates_for_segments(&[name.to_owned()])
            .into_iter()
            .filter_map(|candidate| {
                self.by_path
                    .contains(&candidate)
                    .then(|| candidate.clone())
                    .or_else(|| self.resolve_reexported(&candidate, visited))
            })
            .collect::<Vec<_>>();
        matches.sort();
        matches.dedup();
        match matches.as_slice() {
            [path] => Some(path.clone()),
            [] | [_, ..] => None,
        }
    }
}

fn contract_path(root_crate: &str, path: &str) -> Path {
    let segments = path.split("::").collect::<Vec<_>>();
    match segments.split_first() {
        Some((root, rest)) if *root == root_crate => Path::new(
            PathRoot::Crate,
            rest.iter().copied().map(PathSegment::new).collect(),
        ),
        Some(_) | None => Path::new(
            PathRoot::Relative,
            segments.into_iter().map(PathSegment::new).collect(),
        ),
    }
}

fn scan_each<I, T>(
    items: &[I],
    declared_types: &DeclaredTypes,
    scan: impl Fn(&I, &DeclaredTypes) -> Result<T, ScanError>,
) -> Result<Vec<T>, ScanError> {
    items
        .iter()
        .map(|item| scan(item, declared_types))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        AttributeInput, ClassId, ConstExpr, ConstantId, CustomRemoteGenericArgument,
        CustomRemotePath, CustomRemotePathSegment, CustomRemoteType, CustomTypeConverter,
        CustomTypeId, DefaultValue, DeprecationInfo, EnumId, IntegerLiteral, Literal, Path,
        PathRoot, PathSegment, Primitive, Receiver, RecordDef, RecordId, ReturnDef, StreamId,
        StreamMode, TraitId, TypeExpr,
    };

    fn parse(source: &str) -> syn::File {
        syn::parse_str(source).expect("valid source file")
    }

    fn try_scan(source: &str) -> Result<SourceContract, ScanError> {
        scan_file(parse(source), PackageInfo::new("demo", None))
    }

    fn scan(source: &str) -> SourceContract {
        try_scan(source).expect("scan")
    }

    fn source_tree(crate_name: &str, source: &str) -> SourceTree {
        SourceTree::in_memory(crate_name, parse(source).items).expect("source tree")
    }

    fn point(contract: &SourceContract) -> &RecordDef {
        contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Point"))
            .expect("Point record")
    }

    fn value_return(return_def: &ReturnDef) -> &TypeExpr {
        match return_def {
            ReturnDef::Value(type_expr) => type_expr,
            ReturnDef::Void => panic!("expected value return"),
        }
    }

    fn record(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(id), source_path(path))
    }

    fn enumeration(id: &str, path: &str) -> TypeExpr {
        TypeExpr::enumeration(EnumId::new(id), source_path(path))
    }

    fn custom(id: &str, path: &str) -> TypeExpr {
        TypeExpr::custom(CustomTypeId::new(id), source_path(path))
    }

    fn custom_converter(role: &str) -> CustomTypeConverter {
        CustomTypeConverter::path(Path::new(
            PathRoot::Crate,
            vec![PathSegment::new(format!(
                "__boltffi_custom_type_utc_date_time_{role}"
            ))],
        ))
    }

    fn class(id: &str, path: &str) -> TypeExpr {
        TypeExpr::class(ClassId::new(id), source_path(path))
    }

    fn source_path(path: &str) -> Path {
        let (root, path) = path
            .strip_prefix("crate::")
            .map(|path| (PathRoot::Crate, path))
            .unwrap_or((PathRoot::Relative, path));
        Path::new(root, path.split("::").map(PathSegment::new).collect())
    }

    fn nullable(type_expr: TypeExpr) -> TypeExpr {
        TypeExpr::option(type_expr)
    }

    fn callback_trait(id: &str, path: &str) -> TypeExpr {
        TypeExpr::impl_trait(TraitId::new(id), Path::single(path))
    }

    fn boxed_callback(id: &str, path: &str) -> TypeExpr {
        TypeExpr::boxed(TypeExpr::dyn_trait(TraitId::new(id), Path::single(path)))
    }

    fn assert_custom(actual: &TypeExpr, expected: &str) {
        assert!(matches!(
            actual,
            TypeExpr::Custom { id, .. } if id == &CustomTypeId::new(expected)
        ));
    }

    #[test]
    fn scan_source_reads_and_parses_the_file_itself() {
        let path = std::env::temp_dir().join("boltffi_scan_entry_point.rs");
        std::fs::write(&path, "#[data] pub struct Point { pub x: f64 }").expect("write source");

        let contract = scan_source(&path, PackageInfo::new("demo", None)).expect("scan");

        std::fs::remove_file(&path).ok();
        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.records[0].id, RecordId::new("demo::Point"));
    }

    #[test]
    fn scan_source_reports_a_missing_file_as_a_read_error() {
        let path = std::env::temp_dir().join("boltffi_scan_does_not_exist.rs");
        std::fs::remove_file(&path).ok();

        let error = scan_source(&path, PackageInfo::new("demo", None))
            .expect_err("a missing file must reject");

        assert!(matches!(error, ScanError::Read { .. }));
    }

    #[test]
    fn scan_source_reports_invalid_rust_as_a_parse_error() {
        let path = std::env::temp_dir().join("boltffi_scan_invalid.rs");
        std::fs::write(&path, "#[data] pub struct {").expect("write source");

        let error = scan_source(&path, PackageInfo::new("demo", None))
            .expect_err("invalid source must reject");

        std::fs::remove_file(&path).ok();
        assert!(matches!(error, ScanError::Parse { .. }));
    }

    #[test]
    fn scan_source_populates_metadata_defaults_user_attrs_and_spans() {
        let source = "\
            #[data]\n\
            #[serde(rename = \"point\")]\n\
            #[deprecated(since = \"2.0\", note = \"use Vector\")]\n\
            /// Point docs\n\
            pub struct Point {\n\
                #[serde(rename = \"xValue\")]\n\
                #[default(7)]\n\
                /// x docs\n\
                pub x: i32,\n\
            }\n\
            #[export]\n\
            #[serde(rename = \"add\")]\n\
            #[deprecated = \"use sum\"]\n\
            /// Adds values\n\
            pub fn add(#[default(1)] #[serde(rename = \"left\")] a: i32) -> i32 { a }\n\
        ";
        let path = std::env::temp_dir().join("boltffi_scan_metadata.rs");
        std::fs::write(&path, source).expect("write source");

        let contract = scan_source(&path, PackageInfo::new("demo", None)).expect("scan");

        std::fs::remove_file(&path).ok();
        let record = &contract.records[0];
        let field = &record.fields[0];
        let function = &contract.functions[0];
        let parameter = &function.parameters[0];
        let record_span = record.source_span.as_ref().expect("record span");
        let field_span = field.source_span.as_ref().expect("field span");
        let function_span = function.source_span.as_ref().expect("function span");

        assert_eq!(
            record.doc.as_ref().map(|doc| doc.as_str()),
            Some("Point docs")
        );
        assert_eq!(
            record.deprecated,
            Some(DeprecationInfo::new(
                Some("use Vector".to_owned()),
                Some("2.0".to_owned())
            ))
        );
        assert_eq!(record.user_attrs.len(), 1);
        assert_eq!(record.user_attrs[0].path, Path::single("serde"));
        assert_eq!(
            record.user_attrs[0].input,
            AttributeInput::Tokens("rename = \"point\"".to_owned())
        );
        assert_eq!(field.doc.as_ref().map(|doc| doc.as_str()), Some("x docs"));
        assert_eq!(
            field.default,
            Some(DefaultValue::Integer(IntegerLiteral::new(7, "7")))
        );
        assert_eq!(field.user_attrs[0].path, Path::single("serde"));
        assert_eq!(
            function.doc.as_ref().map(|doc| doc.as_str()),
            Some("Adds values")
        );
        assert_eq!(
            function.deprecated,
            Some(DeprecationInfo::new(Some("use sum".to_owned()), None))
        );
        assert_eq!(function.user_attrs[0].path, Path::single("serde"));
        assert_eq!(
            parameter.default,
            Some(DefaultValue::Integer(IntegerLiteral::new(1, "1")))
        );
        assert_eq!(
            parameter.user_attrs[0].input,
            AttributeInput::Tokens("rename = \"left\"".to_owned())
        );
        assert_eq!(record.source.span, record.source_span);
        assert_eq!(field.source.span, field.source_span);
        assert_eq!(function.source.span, function.source_span);
        assert_eq!(record_span.file.as_str(), path.display().to_string());
        assert!(source[record_span.start..record_span.end].contains("pub struct Point"));
        assert!(source[field_span.start..field_span.end].contains("pub x: i32"));
        assert!(source[function_span.start..function_span.end].contains("pub fn add"));
    }

    #[test]
    fn scans_items_across_modules_and_qualifies_ids_by_module_path() {
        let contract = scan(
            "#[data] pub struct Shape { pub center: crate::geometry::Point } \
             pub mod geometry { #[data] pub struct Point { pub x: f64 } }",
        );

        assert!(
            contract
                .records
                .iter()
                .any(|record| record.id == RecordId::new("demo::geometry::Point"))
        );
        let shape = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Shape"))
            .expect("Shape record");
        assert_eq!(
            shape.fields[0].type_expr,
            record("demo::geometry::Point", "crate::geometry::Point")
        );
    }

    #[test]
    fn unqualified_reference_does_not_guess_across_modules() {
        let error = try_scan(
            "#[data] pub struct Shape { pub center: Point } \
             pub mod geometry { #[data] pub struct Point { pub x: f64 } }",
        )
        .expect_err("unqualified cross-module reference must reject");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn explicit_import_resolves_type_reexported_through_glob() {
        let contract = scan(
            "pub mod enums { \
                 pub mod repr_int { #[data] pub enum Priority { Low, High } } \
                 pub use repr_int::*; \
             } \
             pub mod records { \
                 use crate::enums::Priority; \
                 #[data] pub struct Task { pub priority: Priority } \
             }",
        );

        let task = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::records::Task"))
            .expect("Task record");

        assert_eq!(
            task.fields[0].type_expr,
            enumeration("demo::enums::repr_int::Priority", "Priority")
        );
    }

    #[test]
    fn explicit_import_resolves_type_reexported_by_name() {
        let contract = scan(
            "pub mod model { #[data] pub enum ForeignKind { Guest, Member } } \
             pub mod session { pub use crate::model::ForeignKind; } \
             pub mod api { \
                 use crate::session::ForeignKind; \
                 #[export] pub fn echo(kind: ForeignKind) -> ForeignKind { kind } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            enumeration("demo::model::ForeignKind", "ForeignKind")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &enumeration("demo::model::ForeignKind", "ForeignKind")
        );
    }

    #[test]
    fn c_style_enum_scan_accepts_borrowed_string_export_return() {
        let contract = scan(
            "#[data] \
             #[repr(i32)] \
             pub enum Direction { Default = 0, Up = 1, Down = 2 } \
             #[export] \
             pub fn direction_name(direction: Direction) -> &'static str { \
                 match direction { \
                     Direction::Default => \"default\", \
                     Direction::Up => \"up\", \
                     Direction::Down => \"down\", \
                 } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            enumeration("demo::Direction", "Direction")
        );
        assert_eq!(
            contract.functions[0].returns,
            ReturnDef::value(TypeExpr::Str)
        );
    }

    #[test]
    fn explicit_import_resolves_dependency_type_reexported_through_bare_module_paths() {
        // A uniform-path re-export (`pub use camera::CameraUpdate;` naming a
        // sibling module without a `crate::`/`self::` prefix) is stored
        // unqualified, so resolution must qualify it with the module path.
        let maplib = source_tree(
            "maplib",
            "mod camera { \
                 mod update { #[data] pub struct CameraUpdate { pub zoom: f64 } } \
                 pub use update::CameraUpdate; \
             } \
             pub use camera::CameraUpdate;",
        );
        let root = source_tree(
            "demo",
            "use maplib::CameraUpdate; \
             #[data] pub struct MapOptions { pub initial_camera: CameraUpdate }",
        );
        let complete = SourceTree::combine([maplib, root.clone()]);
        let root_marked = MarkedItems::collect(&root).expect("root marked items");
        let complete_marked = MarkedItems::collect(&complete).expect("complete marked items");
        let declared_types =
            DeclaredTypes::index(&complete, &complete_marked).expect("declared types");
        let contract = scan_marked_with_declarations(
            &root_marked,
            &declared_types,
            PackageInfo::new("demo", None),
        )
        .expect("dependency reexport chain resolves");

        assert_eq!(
            contract.records[0].fields[0].type_expr,
            record("maplib::camera::update::CameraUpdate", "CameraUpdate")
        );

        let paths = root_visible_paths(
            &declared_types,
            &complete,
            &complete_marked,
            "demo",
            &["maplib".to_owned()],
        );
        let path = paths
            .get("maplib::camera::update::CameraUpdate")
            .expect("dependency type spells through its crate root reexport");

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["maplib", "CameraUpdate"]
        );
    }

    #[test]
    fn scans_marked_items_nested_several_modules_deep() {
        let contract = scan(
            "pub mod a { pub mod b { \
                 #[data] pub struct Deep { pub x: i32 } \
                 #[export] pub fn deep() -> Deep { todo!() } \
             } }",
        );

        assert_eq!(contract.records[0].id, RecordId::new("demo::a::b::Deep"));
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::a::b::Deep", "Deep")
        );
    }

    #[test]
    fn resolves_record_reference_regardless_of_declaration_order() {
        let contract = scan(
            "#[data] pub struct Shape { pub center: Point } \
             #[data] pub struct Point { pub x: f64 }",
        );

        assert_eq!(contract.records.len(), 2);
        let shape = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Shape"))
            .expect("Shape record");
        assert_eq!(shape.fields[0].type_expr, record("demo::Point", "Point"));
    }

    #[test]
    fn scans_functions_and_resolves_their_record_references() {
        let contract = scan(
            "#[data] pub struct Point { pub x: f64 } \
             #[export] pub fn origin() -> Point { todo!() }",
        );

        assert_eq!(contract.functions.len(), 1);
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::Point", "Point")
        );
    }

    #[test]
    fn scans_custom_type_and_resolves_remote_uses() {
        let contract = scan(
            "custom_type!(pub UtcDateTime, remote = DateTime<Utc>, repr = i64, error = ConvertError, into_ffi = |dt: &DateTime<Utc>| dt.timestamp_millis(), try_from_ffi = |millis: i64| from_millis(millis)); \
             #[export] pub fn round_trip(value: DateTime<Utc>) -> DateTime<Utc> { value }",
        );

        assert_eq!(contract.customs.len(), 1);
        assert_eq!(
            contract.customs[0].id,
            CustomTypeId::new("demo::UtcDateTime")
        );
        assert_eq!(
            contract.customs[0].remote,
            CustomRemoteType::path(CustomRemotePath::new(
                PathRoot::Relative,
                vec![CustomRemotePathSegment::with_arguments(
                    "DateTime",
                    vec![CustomRemoteGenericArgument::Type(Box::new(
                        CustomRemoteType::single_path("Utc")
                    ))]
                )]
            ))
        );
        assert_eq!(
            contract.customs[0].repr,
            TypeExpr::Primitive(Primitive::I64)
        );
        assert_eq!(
            contract.customs[0].error,
            Some(CustomRemoteType::single_path("ConvertError"))
        );
        assert_eq!(
            contract.customs[0].converters.into_ffi,
            custom_converter("into_ffi")
        );
        assert_eq!(
            contract.customs[0].converters.try_from_ffi,
            custom_converter("try_from_ffi")
        );
        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            custom("demo::UtcDateTime", "DateTime")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &custom("demo::UtcDateTime", "DateTime")
        );
    }

    #[test]
    fn scans_custom_remote_used_through_type_alias() {
        let contract = scan(
            "custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, error = ConvertError, into_ffi = |dt: &chrono::DateTime<chrono::Utc>| dt.timestamp_millis(), try_from_ffi = |millis: i64| from_millis(millis)); \
             pub mod core { \
                 pub mod location { \
                     pub mod types { \
                         pub mod coor { \
                             use chrono::{DateTime, Utc}; \
                             pub type UtcDateTime = DateTime<Utc>; \
                         } \
                         pub mod location { \
                             use super::coor::UtcDateTime; \
                             #[data] pub struct CurrentLocation { pub timestamp: UtcDateTime } \
                         } \
                     } \
                 } \
             }",
        );
        let location = contract
            .records
            .iter()
            .find(|record| {
                record.id == RecordId::new("demo::core::location::types::location::CurrentLocation")
            })
            .expect("CurrentLocation record");

        assert_eq!(
            location.fields[0].type_expr,
            custom("demo::UtcDateTime", "UtcDateTime")
        );
    }

    #[test]
    fn scans_custom_repr_reexported_from_root() {
        let contract = scan(
            "pub use core::location::{GeoCoord, GeographicCoordinate}; \
             pub mod core { \
                 pub mod location { \
                     pub use types::coor::{GeoCoord, GeographicCoordinate}; \
                     pub mod types { \
                         pub mod coor { \
                             pub type GeoCoord = geo::Coord<f64>; \
                             #[data] pub struct GeographicCoordinate { pub latitude: f64, pub longitude: f64 } \
                         } \
                     } \
                 } \
             } \
             custom_type!(pub GeoCoord, remote = geo::Coord<f64>, repr = GeographicCoordinate, error = ConvertError, into_ffi = |coord: &geo::Coord<f64>| GeographicCoordinate { latitude: coord.y, longitude: coord.x }, try_from_ffi = |value: GeographicCoordinate| from_coordinate(value));",
        );

        assert_eq!(
            contract.customs[0].repr,
            record(
                "demo::core::location::types::coor::GeographicCoordinate",
                "GeographicCoordinate"
            )
        );
    }

    #[test]
    fn scans_custom_ffi_trait_impl_and_resolves_remote_uses() {
        let contract = scan(
            "pub struct Email(String); \
             #[custom_ffi] impl CustomFfiConvertible for Email { \
                 type FfiRepr = String; \
                 type Error = String; \
                 fn into_ffi(&self) -> String { self.0.clone() } \
                 fn try_from_ffi(value: String) -> Result<Self, String> { Ok(Self(value)) } \
             } \
             #[export] pub fn round_trip(value: Email) -> Email { value }",
        );

        assert_eq!(contract.customs.len(), 1);
        assert_eq!(contract.customs[0].id, CustomTypeId::new("demo::Email"));
        assert_eq!(
            contract.customs[0].remote,
            CustomRemoteType::single_path("Email")
        );
        assert_eq!(contract.customs[0].repr, TypeExpr::String);
        assert_eq!(
            contract.customs[0].error,
            Some(CustomRemoteType::single_path("String"))
        );
        let CustomTypeConverter::TraitMethod(into_ffi) = &contract.customs[0].converters.into_ffi
        else {
            panic!("expected trait method converter");
        };
        let CustomTypeConverter::TraitMethod(try_from_ffi) =
            &contract.customs[0].converters.try_from_ffi
        else {
            panic!("expected trait method converter");
        };
        assert_eq!(into_ffi.receiver, source_path("Email"));
        assert_eq!(into_ffi.method.as_str(), "into_ffi");
        assert_eq!(try_from_ffi.receiver, source_path("Email"));
        assert_eq!(try_from_ffi.method.as_str(), "try_from_ffi");
        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            custom("demo::Email", "Email")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &custom("demo::Email", "Email")
        );
    }

    #[test]
    fn declared_type_wins_over_custom_remote_with_same_source_path() {
        let contract = scan(
            "custom_type!(pub TimestampWire, remote = Timestamp, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             #[data] pub struct Timestamp { pub value: i64 } \
             #[export] pub fn keep(value: Timestamp) -> Timestamp { value }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            record("demo::Timestamp", "Timestamp")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::Timestamp", "Timestamp")
        );
    }

    #[test]
    fn custom_remote_resolution_is_scoped_to_the_declaring_module() {
        let contract = scan(
            "pub mod custom { \
                 custom_type!(pub TimestampWire, remote = Timestamp, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod data { \
                 #[data] pub struct Timestamp { pub value: i64 } \
                 #[export] pub fn keep(value: Timestamp) -> Timestamp { value } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            record("demo::data::Timestamp", "Timestamp")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::data::Timestamp", "Timestamp")
        );
    }

    #[test]
    fn unmarked_local_source_type_blocks_custom_remote_resolution() {
        let error = try_scan(
            "custom_type!(pub TimestampWire, remote = Timestamp, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             pub mod api { \
                 pub struct Timestamp; \
                 #[export] pub fn keep(value: Timestamp) {} \
             }",
        )
        .expect_err("unmarked local source type must reject");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Timestamp"
        ));
    }

    #[test]
    fn qualified_custom_remote_resolution_is_available_across_modules() {
        let contract = scan(
            "pub mod custom { \
                 custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod api { \
                 #[export] pub fn round_trip(value: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> { value } \
             }",
        );

        assert_custom(
            &contract.functions[0].parameters[0].type_expr,
            "demo::custom::UtcDateTime",
        );
        assert_custom(
            value_return(&contract.functions[0].returns),
            "demo::custom::UtcDateTime",
        );
    }

    #[test]
    fn custom_remote_shape_resolution_matches_imported_spelling() {
        let contract = scan(
            "pub mod custom { \
                 custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod api { \
                 #[export] pub fn round_trip(value: DateTime<Utc>) -> DateTime<Utc> { value } \
             }",
        );

        assert_custom(
            &contract.functions[0].parameters[0].type_expr,
            "demo::custom::UtcDateTime",
        );
        assert_custom(
            value_return(&contract.functions[0].returns),
            "demo::custom::UtcDateTime",
        );
    }

    #[test]
    fn root_visible_paths_use_direct_dependency_reexports() {
        let root = source_tree(
            "demo",
            "pub mod api { \
                use session::Thing; \
                #[export] pub fn keep(value: Thing) -> Thing { value } \
            }",
        );
        let session = source_tree(
            "session",
            "pub use model::{Counter, Thing, model_echo_kind};",
        );
        let model = source_tree(
            "model",
            "#[data] pub struct Thing { pub value: u32 } \
             pub struct Counter { value: u32 } \
             #[export] impl Counter { \
                 pub fn new(value: u32) -> Self { Self { value } } \
             } \
             #[export] pub fn model_echo_kind(kind: u32) -> u32 { kind }",
        );
        let complete = SourceTree::combine([model, session, root]);
        let marked = MarkedItems::collect(&complete).expect("marked items");
        let declared_types = DeclaredTypes::index(&complete, &marked).expect("declared types");
        let paths = root_visible_paths(
            &declared_types,
            &complete,
            &marked,
            "demo",
            &["session".to_owned()],
        );
        let path = paths
            .get("model::Thing")
            .expect("reexported model type is visible through session");

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "Thing"]
        );

        let path = paths
            .get("model::Counter")
            .expect("reexported model class is visible through session");

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "Counter"]
        );

        let path = paths
            .get("model::model_echo_kind")
            .expect("reexported model function is visible through session");

        assert_eq!(path.root, PathRoot::Relative);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "model_echo_kind"]
        );
    }

    #[test]
    fn root_visible_paths_prefer_public_reexports_over_private_module_paths() {
        let root = source_tree(
            "demo",
            "mod inner { #[data] pub struct Hidden { pub x: f64 } } \
             pub use inner::Hidden;",
        );
        let marked = MarkedItems::collect(&root).expect("marked items");
        let declared_types = DeclaredTypes::index(&root, &marked).expect("declared types");
        let paths = root_visible_paths(&declared_types, &root, &marked, "demo", &[]);
        let path = paths
            .get("demo::inner::Hidden")
            .expect("privately declared root type is visible");

        assert_eq!(path.root, PathRoot::Crate);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Hidden"]
        );
    }

    #[test]
    fn root_visible_paths_resolve_macro_generated_custom_ffi_types() {
        // The defining struct comes from a macro invocation, so it is absent
        // from the item index; the custom_ffi impl must still prove the type
        // exists for the re-export chain to verify against.
        let root = source_tree(
            "demo",
            "mod ids { \
                 new_key_type! { pub struct Token; } \
                 #[custom_ffi] impl CustomFfiConvertible for Token { \
                     type FfiRepr = u64; \
                     type Error = String; \
                     fn into_ffi(&self) -> u64 { 0 } \
                     fn try_from_ffi(value: u64) -> Result<Self, String> { todo!() } \
                 } \
             } \
             pub use ids::Token;",
        );
        let marked = MarkedItems::collect(&root).expect("marked items");
        let declared_types = DeclaredTypes::index(&root, &marked).expect("declared types");
        let paths = root_visible_paths(&declared_types, &root, &marked, "demo", &[]);
        let path = paths
            .get("demo::ids::Token")
            .expect("macro-generated custom type is visible");

        assert_eq!(path.root, PathRoot::Crate);
        assert_eq!(
            path.segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Token"]
        );
    }

    #[test]
    fn root_with_support_keeps_dependency_classes() {
        let root = source_tree("demo", "");
        let model = source_tree(
            "model",
            "pub struct ForeignCounter { value: i32 } \
             #[export] impl ForeignCounter { \
                 pub fn new(initial: i32) -> Self { Self { value: initial } } \
                 pub fn add(&self, amount: i32) -> i32 { self.value + amount } \
             }",
        );
        let complete = SourceTree::combine([model, root.clone()]);
        let scan = PackageScan {
            root: scan_tree(root, PackageInfo::new("demo", None)).expect("root scans"),
            complete: scan_tree(complete, PackageInfo::new("demo", None)).expect("complete scans"),
            root_visible_paths: HashMap::new(),
        };
        let source = scan.root_with_support();
        let counter = source
            .classes
            .iter()
            .find(|class| class.id == ClassId::new("model::ForeignCounter"))
            .expect("dependency class stays in root support contract");

        assert_eq!(counter.methods.len(), 2);
    }

    #[test]
    fn root_with_support_keeps_dependency_data_impl_methods() {
        let model = source_tree(
            "model",
            "#[data] pub struct ForeignPoint { pub x: f64 } \
             #[data] pub enum ForeignKind { Guest, Member }",
        );
        let root = source_tree(
            "demo",
            "use model::{ForeignKind, ForeignPoint}; \
             #[data(impl)] impl ForeignPoint { pub fn origin() -> Self { todo!() } } \
             #[data(impl)] impl ForeignKind { pub fn guest() -> Self { todo!() } }",
        );
        let complete = SourceTree::combine([model, root.clone()]);
        let root_marked = MarkedItems::collect(&root).expect("root marked items");
        let complete_marked = MarkedItems::collect(&complete).expect("complete marked items");
        let declared_types =
            DeclaredTypes::index(&complete, &complete_marked).expect("declared types");
        let scan = PackageScan {
            root: scan_marked_with_declarations(
                &root_marked,
                &declared_types,
                PackageInfo::new("demo", None),
            )
            .expect("root scans"),
            complete: scan_marked_with_declarations(
                &complete_marked,
                &declared_types,
                PackageInfo::new("demo", None),
            )
            .expect("complete scans"),
            root_visible_paths: HashMap::from([
                (
                    "model::ForeignPoint".to_owned(),
                    Path::new(
                        PathRoot::Relative,
                        vec![PathSegment::new("model"), PathSegment::new("ForeignPoint")],
                    ),
                ),
                (
                    "model::ForeignKind".to_owned(),
                    Path::new(
                        PathRoot::Relative,
                        vec![PathSegment::new("model"), PathSegment::new("ForeignKind")],
                    ),
                ),
            ]),
        };
        let source = scan.root_with_support();
        let point = source
            .records
            .iter()
            .find(|record| record.id == RecordId::new("model::ForeignPoint"))
            .expect("dependency record stays in root support contract");
        let kind = source
            .enums
            .iter()
            .find(|enumeration| enumeration.id == EnumId::new("model::ForeignKind"))
            .expect("dependency enum stays in root support contract");

        assert_eq!(point.methods.len(), 1);
        assert_eq!(point.methods[0].id.as_str(), "model::ForeignPoint::origin");
        assert_eq!(kind.methods.len(), 1);
        assert_eq!(kind.methods[0].id.as_str(), "model::ForeignKind::guest");
    }

    #[test]
    fn root_with_support_removes_dependency_data_impl_methods_without_visible_paths() {
        let model = source_tree(
            "model",
            "#[data] pub struct ForeignPoint { pub x: f64 } \
             #[data] pub enum ForeignKind { Guest, Member }",
        );
        let root = source_tree(
            "demo",
            "use model::{ForeignKind, ForeignPoint}; \
             #[data(impl)] impl ForeignPoint { pub fn origin() -> Self { todo!() } } \
             #[data(impl)] impl ForeignKind { pub fn guest() -> Self { todo!() } }",
        );
        let complete = SourceTree::combine([model, root.clone()]);
        let root_marked = MarkedItems::collect(&root).expect("root marked items");
        let complete_marked = MarkedItems::collect(&complete).expect("complete marked items");
        let declared_types =
            DeclaredTypes::index(&complete, &complete_marked).expect("declared types");
        let scan = PackageScan {
            root: scan_marked_with_declarations(
                &root_marked,
                &declared_types,
                PackageInfo::new("demo", None),
            )
            .expect("root scans"),
            complete: scan_marked_with_declarations(
                &complete_marked,
                &declared_types,
                PackageInfo::new("demo", None),
            )
            .expect("complete scans"),
            root_visible_paths: HashMap::new(),
        };
        let source = scan.root_with_support();
        let point = source
            .records
            .iter()
            .find(|record| record.id == RecordId::new("model::ForeignPoint"))
            .expect("dependency record stays in root support contract");
        let kind = source
            .enums
            .iter()
            .find(|enumeration| enumeration.id == EnumId::new("model::ForeignKind"))
            .expect("dependency enum stays in root support contract");

        assert!(point.methods.is_empty());
        assert!(kind.methods.is_empty());
    }

    #[test]
    fn custom_remote_resolution_uses_explicit_import_aliases() {
        let contract = scan(
            "pub mod custom { \
                 custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod api { \
                 use chrono::{DateTime as Dt, Utc as Zone}; \
                 #[export] pub fn round_trip(value: Dt<Zone>) -> Dt<Zone> { value } \
             }",
        );

        assert_custom(
            &contract.functions[0].parameters[0].type_expr,
            "demo::custom::UtcDateTime",
        );
        assert_custom(
            value_return(&contract.functions[0].returns),
            "demo::custom::UtcDateTime",
        );
    }

    #[test]
    fn custom_remote_resolution_uses_glob_imports_for_exact_matching() {
        let contract = scan(
            "pub mod custom { \
                 custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod api { \
                 use chrono::*; \
                 #[export] pub fn round_trip(value: DateTime<Utc>) -> DateTime<Utc> { value } \
             }",
        );

        assert_custom(
            &contract.functions[0].parameters[0].type_expr,
            "demo::custom::UtcDateTime",
        );
        assert_custom(
            value_return(&contract.functions[0].returns),
            "demo::custom::UtcDateTime",
        );
    }

    #[test]
    fn declared_type_resolution_uses_explicit_import_aliases() {
        let contract = scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::Point as P; \
                 #[export] pub fn keep(value: P) -> P { value } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            record("demo::geometry::Point", "P")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::geometry::Point", "P")
        );
    }

    #[test]
    fn class_impl_target_resolution_uses_import_aliases_without_renaming_the_class() {
        let contract = scan(
            "pub mod runtime { pub struct Engine; } \
             pub mod api { \
                 use crate::runtime::Engine as Runtime; \
                 #[export] impl Runtime { pub fn start(&self) {} } \
             }",
        );

        assert_eq!(
            contract.classes[0].id,
            ClassId::new("demo::runtime::Engine")
        );
        assert_eq!(contract.classes[0].name.as_path_string(), "engine");
        assert_eq!(
            contract.classes[0].methods[0].id.as_str(),
            "demo::runtime::Engine::start"
        );
    }

    #[test]
    fn data_impl_target_resolution_uses_import_aliases() {
        let contract = scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::Point as P; \
                 #[data(impl)] impl P { pub fn origin() -> Self { todo!() } } \
             }",
        );
        let point = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::geometry::Point"))
            .expect("geometry point record");

        assert_eq!(point.methods.len(), 1);
        assert_eq!(
            point.methods[0].id.as_str(),
            "demo::geometry::Point::origin"
        );
        assert_eq!(
            point.methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
    }

    #[test]
    fn declared_type_resolution_uses_local_glob_imports() {
        let contract = scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::*; \
                 #[export] pub fn keep(value: Point) -> Point { value } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            record("demo::geometry::Point", "Point")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::geometry::Point", "Point")
        );
    }

    #[test]
    fn class_impl_target_rejects_local_type_and_explicit_import_ambiguity() {
        let error = try_scan(
            "pub mod runtime { pub struct Engine; } \
             pub mod api { \
                 use crate::runtime::Engine; \
                 pub struct Engine; \
                 #[export] impl Engine { pub fn start(&self) {} } \
             }",
        )
        .expect_err("class impl target must follow Rust type namespace ambiguity");

        assert_eq!(
            error,
            ScanError::AmbiguousPath {
                path: "Engine".to_owned()
            }
        );
    }

    #[test]
    fn data_impl_target_does_not_fall_back_to_glob_when_local_type_exists() {
        let error = try_scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::*; \
                 pub struct Point; \
                 #[data(impl)] impl Point { pub fn origin() -> Self { todo!() } } \
             }",
        )
        .expect_err("data impl target must resolve to the local unmarked type");

        assert_eq!(
            error,
            ScanError::UnsupportedMarkedImpl {
                target: "Point".to_owned()
            }
        );
    }

    #[test]
    fn local_declarations_win_over_glob_imports() {
        let contract = scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::*; \
                 #[data] pub struct Point { pub y: f64 } \
                 #[export] pub fn keep(value: Point) -> Point { value } \
             }",
        );

        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            record("demo::api::Point", "Point")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &record("demo::api::Point", "Point")
        );
    }

    #[test]
    fn explicit_import_blocks_declared_type_glob_fallback() {
        let error = try_scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use other::Point; \
                 use crate::geometry::*; \
                 #[export] pub fn keep(value: Point) {} \
             }",
        )
        .expect_err("explicit imports must block glob fallback");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn local_type_and_explicit_import_with_same_name_are_ambiguous() {
        let error = try_scan(
            "pub mod other { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::other::Point; \
                 #[data] pub struct Point { pub y: f64 } \
                 #[export] pub fn keep(value: Point) {} \
             }",
        )
        .expect_err("local type plus explicit import must reject");

        assert_eq!(
            error,
            ScanError::AmbiguousPath {
                path: "Point".to_owned()
            }
        );
    }

    #[test]
    fn local_type_and_explicit_import_alias_with_same_name_are_ambiguous() {
        let error = try_scan(
            "pub mod other { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::other::Point as P; \
                 #[data] pub struct P { pub y: f64 } \
                 #[export] pub fn keep(value: P) {} \
             }",
        )
        .expect_err("local type plus explicit import alias must reject");

        assert_eq!(
            error,
            ScanError::AmbiguousPath {
                path: "P".to_owned()
            }
        );
    }

    #[test]
    fn local_unmarked_type_blocks_glob_declared_type() {
        let error = try_scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::geometry::*; \
                 pub struct Point; \
                 #[export] pub fn keep(value: Point) {} \
             }",
        )
        .expect_err("local unmarked type must win over glob declared type");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn unmarked_glob_type_participates_in_ambiguity() {
        let error = try_scan(
            "pub mod geometry { #[data] pub struct Point { pub x: f64 } } \
             pub mod hidden { pub struct Point; } \
             pub mod api { \
                 use crate::geometry::*; \
                 use crate::hidden::*; \
                 #[export] pub fn keep(value: Point) {} \
             }",
        )
        .expect_err("all source glob types must participate in ambiguity");

        assert_eq!(
            error,
            ScanError::AmbiguousPath {
                path: "Point".to_owned()
            }
        );
    }

    #[test]
    fn conflicting_explicit_imports_reject_unqualified_type_references() {
        let error = try_scan(
            "pub mod left { #[data] pub struct Point { pub x: f64 } } \
             pub mod right { #[data] pub struct Point { pub x: f64 } } \
             pub mod api { \
                 use crate::left::Point; \
                 use crate::right::Point; \
                 #[export] pub fn keep(value: Point) {} \
             }",
        )
        .expect_err("conflicting imports must reject");

        assert_eq!(
            error,
            ScanError::AmbiguousPath {
                path: "Point".to_owned()
            }
        );
    }

    #[test]
    fn custom_remote_shape_fallback_does_not_override_explicit_imports() {
        let error = try_scan(
            "pub mod custom { \
                 custom_type!(pub UtcDateTime, remote = chrono::DateTime<chrono::Utc>, repr = i64, into_ffi = to_millis, try_from_ffi = from_millis); \
             } \
             pub mod api { \
                 use other::{DateTime, Utc}; \
                 #[export] pub fn round_trip(value: DateTime<Utc>) {} \
             }",
        )
        .expect_err("explicit imports must block shape fallback");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "DateTime<Utc>"
        ));
    }

    #[test]
    fn scans_exported_traits_and_resolves_callback_references() {
        let contract = scan(
            "#[export] pub trait Listener { fn on_value(&self, value: i32) -> i64; } \
             #[export] pub fn register(callback: impl Listener) {} \
             #[export] pub fn maybe_register(callback: Option<Box<dyn Listener>>) {}",
        );

        assert_eq!(contract.traits.len(), 1);
        assert_eq!(contract.traits[0].id, TraitId::new("demo::Listener"));
        assert_eq!(contract.traits[0].methods.len(), 1);
        assert_eq!(contract.traits[0].methods[0].receiver, Receiver::Shared);
        assert_eq!(
            contract.traits[0].methods[0].returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::I64))
        );
        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            callback_trait("demo::Listener", "Listener")
        );
        assert_eq!(
            contract.functions[1].parameters[0].type_expr,
            nullable(boxed_callback("demo::Listener", "Listener"))
        );
    }

    #[test]
    fn scans_exported_classes_and_resolves_class_references() {
        let contract = scan(
            "pub struct Engine; \
             #[export] impl Engine { \
                 pub fn new(seed: u64) -> Self { todo!() } \
                 pub fn start(&mut self) {} \
                 pub fn peer(&self, other: Option<Engine>) -> Engine { todo!() } \
             } \
             #[export] pub fn open(engine: Engine) -> Option<Engine> { todo!() }",
        );

        assert_eq!(contract.classes.len(), 1);
        assert_eq!(contract.classes[0].id, ClassId::new("demo::Engine"));
        assert_eq!(contract.classes[0].methods.len(), 3);
        assert_eq!(contract.classes[0].methods[0].receiver, Receiver::None);
        assert_eq!(
            contract.classes[0].methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
        assert_eq!(contract.classes[0].methods[1].receiver, Receiver::Mutable);
        assert_eq!(
            contract.classes[0].methods[2].parameters[0].type_expr,
            nullable(class("demo::Engine", "Engine"))
        );
        assert_eq!(
            value_return(&contract.classes[0].methods[2].returns),
            &class("demo::Engine", "Engine")
        );
        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            class("demo::Engine", "Engine")
        );
        assert_eq!(
            value_return(&contract.functions[0].returns),
            &nullable(class("demo::Engine", "Engine"))
        );
    }

    #[test]
    fn scans_class_streams_and_keeps_them_out_of_methods() {
        let contract = scan(
            "use std::sync::Arc; \
             use boltffi::EventSubscription; \
             pub struct Engine; \
             #[data] pub struct Point { pub x: f64 } \
             #[export] impl Engine { \
                #[ffi_stream(item = Point)] \
                pub fn points(&self) -> Arc<EventSubscription<Point>> { todo!() } \
                #[ffi_stream(item = i32, mode = \"callback\")] \
                pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() } \
                pub fn version(&self) -> u32 { 1 } \
             }",
        );

        assert_eq!(contract.classes.len(), 1);
        assert_eq!(contract.classes[0].methods.len(), 1);
        assert_eq!(
            contract.classes[0].methods[0].id.as_str(),
            "demo::Engine::version"
        );
        assert_eq!(contract.streams.len(), 2);
        assert_eq!(
            contract.streams[0].id,
            StreamId::new("demo::Engine::points")
        );
        assert_eq!(
            contract.streams[0].owner,
            Some(ClassId::new("demo::Engine"))
        );
        assert_eq!(
            contract.streams[0].item_type,
            record("demo::Point", "Point")
        );
        assert_eq!(contract.streams[0].mode, StreamMode::Async);
        assert_eq!(
            contract.streams[1].id,
            StreamId::new("demo::Engine::values")
        );
        assert_eq!(
            contract.streams[1].item_type,
            TypeExpr::Primitive(Primitive::I32)
        );
        assert_eq!(contract.streams[1].mode, StreamMode::Callback);
    }

    #[test]
    fn rejects_class_and_value_type_with_the_same_source_path() {
        let error = try_scan(
            "#[data] pub struct Engine { pub id: u32 } \
             #[export] impl Engine { pub fn new() -> Self { todo!() } }",
        )
        .expect_err("same path cannot declare two exported domains");

        assert_eq!(
            error,
            ScanError::ConflictingDeclarations {
                path: "demo::Engine".to_owned(),
                first: "record".to_owned(),
                second: "class".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_duplicate_value_type_declarations_with_the_same_source_path() {
        let error = try_scan(
            "#[data] pub struct Point { pub x: f64 } \
             #[data] pub struct Point { pub y: f64 }",
        )
        .expect_err("duplicate value declaration rejected");

        assert_eq!(
            error,
            ScanError::ConflictingDeclarations {
                path: "demo::Point".to_owned(),
                first: "record".to_owned(),
                second: "record".to_owned(),
            }
        );
    }

    #[test]
    fn rejects_exported_trait_impl_before_registering_a_class() {
        let error = try_scan(
            "pub struct Engine; \
             #[export] impl Display for Engine {}",
        )
        .expect_err("trait impl cannot declare a class");

        assert_eq!(
            error,
            ScanError::UnsupportedClassImplShape {
                target: "Engine".to_owned(),
            }
        );
    }

    #[test]
    fn scans_enums_and_resolves_enum_typed_fields() {
        let contract = scan(
            "#[data] pub enum Mode { Fast, Slow } \
             #[data] pub struct Engine { pub mode: Mode }",
        );

        assert_eq!(contract.enums.len(), 1);
        assert_eq!(contract.enums[0].id, EnumId::new("demo::Mode"));
        let engine = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Engine"))
            .expect("Engine record");
        assert_eq!(
            engine.fields[0].type_expr,
            enumeration("demo::Mode", "Mode")
        );
    }

    #[test]
    fn scans_exported_constants_and_resolves_declared_types() {
        let contract = scan(
            "#[data] pub enum Mode { Fast, Slow } \
             #[export] pub const DEFAULT_MODE: Mode = Mode::Fast; \
             #[export] pub const ANSWER: u32 = 42;",
        );

        assert_eq!(contract.constants.len(), 2);
        assert_eq!(
            contract.constants[0].id,
            ConstantId::new("demo::DEFAULT_MODE")
        );
        assert_eq!(
            contract.constants[0].type_expr,
            enumeration("demo::Mode", "Mode")
        );
        assert_eq!(
            contract.constants[1].value,
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(42, "42")))
        );
    }

    #[test]
    fn attaches_impl_methods_to_their_record() {
        let contract = scan(
            "#[data] pub struct Point { pub x: f64, pub y: f64 } \
             #[data(impl)] impl Point { \
                 pub fn origin() -> Self { todo!() } \
                 pub fn distance(&self, other: Point) -> f64 { 0.0 } \
             }",
        );
        let point = point(&contract);

        assert_eq!(point.methods.len(), 2);
        assert_eq!(point.methods[0].receiver, Receiver::None);
        assert_eq!(
            point.methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
        assert_eq!(point.methods[1].receiver, Receiver::Shared);
        assert_eq!(
            point.methods[1].parameters[0].type_expr,
            record("demo::Point", "Point")
        );
        assert_eq!(
            point.methods[1].returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::F64))
        );
    }

    #[test]
    fn attaches_impl_methods_to_their_enum() {
        let contract = scan(
            "#[data] pub enum Mode { Fast, Slow } \
             #[data(impl)] impl Mode { \
                 pub fn parse(value: i32) -> Self { todo!() } \
             }",
        );

        assert_eq!(contract.enums[0].methods.len(), 1);
        assert_eq!(
            contract.enums[0].methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
    }

    #[test]
    fn error_types_scan_as_value_types_and_preserve_the_error_attribute() {
        let contract = scan(
            "#[error] pub struct IoError { pub code: i32 } \
             #[error] pub enum ParseError { Eof, Unexpected }",
        );

        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.enums.len(), 1);

        let record = &contract.records[0];
        assert_eq!(record.id, RecordId::new("demo::IoError"));
        assert_eq!(record.user_attrs, vec![error_attr()]);

        let enumeration = &contract.enums[0];
        assert_eq!(enumeration.id, EnumId::new("demo::ParseError"));
        assert_eq!(enumeration.user_attrs, vec![error_attr()]);
    }

    #[test]
    fn data_types_carry_no_error_attribute() {
        let contract = scan("#[data] pub struct Point { pub x: f64 }");

        assert!(contract.records[0].user_attrs.is_empty());
    }

    #[test]
    fn references_to_error_types_resolve_like_any_value_type() {
        let contract = scan(
            "#[error] pub enum ParseError { Eof } \
             #[export] pub fn parse() -> Result<i32, ParseError> { todo!() }",
        );

        assert_eq!(
            value_return(&contract.functions[0].returns),
            &TypeExpr::Result {
                ok: Box::new(TypeExpr::Primitive(Primitive::I32)),
                err: Box::new(enumeration("demo::ParseError", "ParseError")),
            }
        );
    }

    fn error_attr() -> boltffi_ast::UserAttr {
        boltffi_ast::UserAttr::new(
            boltffi_ast::Path::single("error"),
            boltffi_ast::AttributeInput::Empty,
        )
    }

    #[test]
    fn unmarked_items_are_not_scanned() {
        let contract = scan(
            "pub struct Hidden { pub x: i32 } \
             pub enum Internal { A, B } \
             pub fn helper() {} \
             impl Hidden { pub fn touch(&self) {} }",
        );

        assert!(contract.records.is_empty());
        assert!(contract.enums.is_empty());
        assert!(contract.functions.is_empty());
    }

    #[test]
    fn qualified_markers_are_scanned() {
        let contract = scan(
            "#[boltffi::data] pub struct Point { pub x: f64 } \
             #[boltffi::export] pub fn origin() -> Point { todo!() } \
             #[boltffi::export] pub const ANSWER: u32 = 42;",
        );

        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.functions.len(), 1);
        assert_eq!(contract.constants.len(), 1);
    }

    #[test]
    fn invalid_marker_arguments_are_rejected() {
        let error = try_scan("#[data(foo)] pub struct Point { pub x: f64 }")
            .expect_err("invalid marker argument must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarker {
                attribute: "data(foo)".to_owned()
            }
        );
    }

    #[test]
    fn marker_on_wrong_item_kind_is_rejected() {
        let error = try_scan("#[export] pub struct Point { pub x: f64 }")
            .expect_err("wrong marker placement must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "struct".to_owned()
            }
        );
    }

    #[test]
    fn marker_on_module_is_rejected_after_module_loading() {
        let error = try_scan("#[data] pub mod geometry {}")
            .expect_err("wrong marker placement must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "data".to_owned(),
                item: "module".to_owned()
            }
        );
    }

    #[test]
    fn marked_impl_for_unknown_type_is_rejected() {
        let error = try_scan("#[data(impl)] impl Missing { pub fn run(&self) {} }")
            .expect_err("marked impl target must resolve");

        assert_eq!(
            error,
            ScanError::UnsupportedMarkedImpl {
                target: "Missing".to_owned()
            }
        );
    }

    #[test]
    fn non_declaration_items_are_ignored() {
        let contract =
            scan("use std::collections::HashMap; #[data] pub struct Point { pub x: f64 }");

        assert_eq!(contract.records.len(), 1);
        assert!(contract.functions.is_empty());
    }

    #[test]
    fn reference_to_unmarked_type_is_rejected() {
        let error = try_scan(
            "#[data] pub struct Shape { pub center: Point } \
             pub struct Point { pub x: f64 }",
        )
        .expect_err("reference to an unmarked type must reject");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }
}
