use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::error_payloads::ErrorPayloadTypes;
use super::reference::DeclarationReferences;

use crate::{
    BindingError, BindingErrorKind, CanonicalName, Decl, DeclarationId, DeclarationRef,
    NativeSymbol, NativeSymbolTable, Surface,
};

/// Schema marker carried in every serialized binding contract.
///
/// The major component changes when the schema becomes incompatible:
/// code compiled against an older major cannot make sense of the new
/// bytes. The minor component grows additively for fields older readers
/// can safely ignore. [`readable`](Self::readable) is the rule both
/// halves enforce together.
///
/// # Example
///
/// `ContractVersion::new(1, 3)` is readable by code built against
/// `CURRENT = (1, 5)`. `ContractVersion::new(2, 0)` is not, because the
/// major component disagrees.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ContractVersion {
    major: u16,
    minor: u16,
}

impl ContractVersion {
    /// Version written by this crate.
    pub const CURRENT: Self = Self { major: 0, minor: 1 };

    /// Returns [`Self::CURRENT`].
    pub const fn current() -> Self {
        Self::CURRENT
    }

    /// Builds a version from its components.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Returns the major component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor component.
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Returns `true` when the major matches [`Self::CURRENT`] and the
    /// minor is no greater than [`Self::CURRENT`].
    pub const fn readable(self) -> bool {
        self.major == Self::CURRENT.major && self.minor <= Self::CURRENT.minor
    }

    fn validate(self) -> Result<(), BindingError> {
        if self.readable() {
            Ok(())
        } else {
            Err(BindingError::new(BindingErrorKind::UnsupportedVersion {
                actual: self,
                current: Self::current(),
            }))
        }
    }
}

/// The Rust package whose API a [`Bindings`] describes.
///
/// The name is the source-of-truth identifier that generated module
/// names, diagnostics, and on-disk artifacts refer back to. The version
/// is the `Cargo.toml` value when present and exists for human-readable
/// messages; it is not part of contract identity.
///
/// # Example
///
/// A `Cargo.toml` with `name = "demo"` and `version = "0.2.1"` produces
/// a `PackageInfo` whose name canonicalizes to `["demo"]` and whose
/// version is `Some("0.2.1")`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PackageInfo {
    name: CanonicalName,
    version: Option<String>,
}

impl PackageInfo {
    pub(crate) fn new(name: CanonicalName, version: Option<String>) -> Self {
        Self { name, version }
    }

    /// Returns the package name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the package version.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// A validated classified API contract for one Rust crate and target call surface.
///
/// The declarations form a dependency-closed set paired with the FFI decisions for
/// surface `S`. A contract may contain the crate's complete exported API or a valid
/// subset selected for target coverage. The native symbol table contains exactly the
/// linker names referenced by that set.
///
/// A `Bindings<S>` is always valid by construction. Pattern matching
/// cannot witness duplicate ids, an unreadable schema version, or a
/// symbol table with inconsistent entries; the crate exposes no
/// fallible accessor that would hand back a partially constructed
/// value. A backend typed against `Bindings<S>` cannot accidentally
/// receive a `Bindings<S2>` for a different surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedBindings<S>")]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, S::CallbackProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::CallbackProtocol: serde::de::DeserializeOwned"
))]
pub struct Bindings<S: Surface> {
    version: ContractVersion,
    package: PackageInfo,
    decls: Vec<Decl<S>>,
    symbols: NativeSymbolTable,
}

#[derive(Deserialize)]
#[serde(bound(
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::CallbackProtocol: serde::de::DeserializeOwned"
))]
struct UncheckedBindings<S: Surface> {
    version: ContractVersion,
    package: PackageInfo,
    decls: Vec<Decl<S>>,
    symbols: NativeSymbolTable,
}

impl<S: Surface> Bindings<S> {
    pub(crate) fn new(
        package: PackageInfo,
        decls: Vec<Decl<S>>,
        symbols: NativeSymbolTable,
    ) -> Result<Self, BindingError> {
        let mut decls = decls;
        ErrorPayloadTypes::from_decls(&decls).apply_decls(&mut decls);
        let bindings = Self {
            version: ContractVersion::current(),
            package,
            decls,
            symbols,
        };
        bindings.validate()?;
        Ok(bindings)
    }

    /// Returns the schema version.
    pub const fn version(&self) -> ContractVersion {
        self.version
    }

    /// Returns the producing package.
    pub fn package(&self) -> &PackageInfo {
        &self.package
    }

    /// Returns the declarations.
    pub fn decls(&self) -> &[Decl<S>] {
        &self.decls
    }

    /// Returns the native symbol table.
    pub fn symbols(&self) -> &NativeSymbolTable {
        &self.symbols
    }

    /// Returns `true` when [`Self::version`] is readable by this crate.
    pub const fn readable(&self) -> bool {
        self.version.readable()
    }

    /// Returns `Ok` when:
    ///
    /// - the contract version is readable by this crate;
    /// - every native symbol has a callable spelling and a unique id and
    ///   name;
    /// - every declaration id is unique within its family;
    /// - every declaration reference resolves to the required declaration shape;
    /// - every callable inside every declaration satisfies its own
    ///   invariants (return slot, buffer shape compatibility, and so
    ///   on);
    /// - the symbol table contains exactly the native symbols referenced
    ///   by the declarations.
    ///
    /// Returns the first failed invariant otherwise.
    pub fn validate(&self) -> Result<(), BindingError> {
        self.validate_with(References::Complete)
    }

    fn validate_with(&self, references: References) -> Result<(), BindingError> {
        self.version.validate()?;
        self.symbols.validate()?;
        self.validate_unique_decl_ids()?;
        self.validate_references(references)?;
        self.validate_streams()?;
        self.validate_classes()?;
        self.validate_callables()?;
        self.validate_symbol_membership()
    }

    /// Builds a contract from declarations, deriving the symbol table
    /// from every native symbol referenced by the declarations.
    ///
    /// The resulting table is the deduplicated union of every symbol
    /// the decls reference. Membership validation in
    /// [`Self::validate`] is therefore guaranteed to pass for the
    /// returned value; constructing through this entry point is the
    /// canonical way for the classifier to assemble a `Bindings<S>`
    /// without keeping a parallel symbol list in sync by hand.
    pub(crate) fn from_decls(
        package: PackageInfo,
        decls: Vec<Decl<S>>,
    ) -> Result<Self, BindingError> {
        Self::from_decls_with_version(ContractVersion::current(), package, decls)
    }

    /// Returns the greatest dependency-closed subset of the requested declarations.
    ///
    /// Every retained declaration keeps its original identity and order. A requested
    /// declaration is removed when any declaration it references is not requested.
    /// Native symbols and derived declaration roles are rebuilt from the retained set.
    pub fn dependency_closed(
        &self,
        requested: &BTreeSet<DeclarationId>,
    ) -> Result<Self, BindingError> {
        let known = self.decls.iter().map(Decl::id).collect::<BTreeSet<_>>();
        if let Some(unknown) = requested.difference(&known).next() {
            return Err(BindingError::new(BindingErrorKind::UnknownDeclarationId(
                *unknown,
            )));
        }

        let reverse_references = self.decls.iter().fold(
            BTreeMap::<DeclarationId, BTreeSet<DeclarationId>>::new(),
            |mut reverse_references, declaration| {
                DeclarationReferences::from_decl(declaration)
                    .iter()
                    .for_each(|reference| {
                        reverse_references
                            .entry(reference.id())
                            .or_default()
                            .insert(declaration.id());
                    });
                reverse_references
            },
        );
        let mut retained = requested.clone();
        let mut removed = known
            .difference(requested)
            .copied()
            .collect::<VecDeque<_>>();
        while let Some(removed_id) = removed.pop_front() {
            reverse_references
                .get(&removed_id)
                .into_iter()
                .flat_map(|dependents| dependents.iter().copied())
                .for_each(|dependent| {
                    if retained.remove(&dependent) {
                        removed.push_back(dependent);
                    }
                });
        }

        let decls = self
            .decls
            .iter()
            .filter(|declaration| retained.contains(&declaration.id()))
            .cloned()
            .collect();
        Self::from_decls_with_version(self.version, self.package.clone(), decls)
    }

    fn from_decls_with_version(
        version: ContractVersion,
        package: PackageInfo,
        decls: Vec<Decl<S>>,
    ) -> Result<Self, BindingError> {
        Self::assemble(version, package, decls, References::Complete)
    }

    /// Builds the declarations one macro invocation owns. Declarations they reference
    /// live in other invocations, so only references to present declarations are checked.
    pub(crate) fn from_invocation_decls(
        package: PackageInfo,
        decls: Vec<Decl<S>>,
    ) -> Result<Self, BindingError> {
        Self::assemble(
            ContractVersion::current(),
            package,
            decls,
            References::Present,
        )
    }

    fn assemble(
        version: ContractVersion,
        package: PackageInfo,
        mut decls: Vec<Decl<S>>,
        references: References,
    ) -> Result<Self, BindingError> {
        ErrorPayloadTypes::from_decls(&decls).apply_decls(&mut decls);
        let symbols = NativeSymbolTable::from_decls(&decls)?;
        let bindings = Self {
            version,
            package,
            decls,
            symbols,
        };
        bindings.validate_with(references)?;
        Ok(bindings)
    }

    fn validate_unique_decl_ids(&self) -> Result<(), BindingError> {
        self.decls
            .iter()
            .map(Decl::id)
            .try_fold(HashSet::new(), |mut seen, decl_id| {
                if seen.insert(decl_id) {
                    Ok(seen)
                } else {
                    Err(BindingError::new(BindingErrorKind::DuplicateDeclarationId(
                        decl_id,
                    )))
                }
            })
            .map(|_| ())
    }

    fn validate_callables(&self) -> Result<(), BindingError> {
        for decl in &self.decls {
            for callable in decl.exported_callables() {
                callable.validate()?;
            }
            for callable in decl.imported_callables() {
                callable.validate()?;
            }
        }
        Ok(())
    }

    fn validate_references(&self, references: References) -> Result<(), BindingError> {
        let declarations = self
            .decls
            .iter()
            .map(|declaration| (declaration.id(), DeclarationRef::from(declaration)))
            .collect::<BTreeMap<_, _>>();
        self.decls.iter().try_for_each(|declaration| {
            let owner = declaration.id();
            DeclarationReferences::from_decl(declaration)
                .iter()
                .try_for_each(|reference| {
                    let referenced = reference.id();
                    match declarations.get(&referenced).copied() {
                        None if references == References::Present => Ok(()),
                        None => Err(BindingError::new(
                            BindingErrorKind::MissingDeclarationReference { owner, referenced },
                        )),
                        Some(actual) if !reference.accepts(actual) => Err(BindingError::new(
                            BindingErrorKind::InvalidDeclarationReference {
                                owner,
                                referenced,
                                expected: reference.shape(),
                            },
                        )),
                        Some(_) => Ok(()),
                    }
                })
        })
    }

    fn validate_streams(&self) -> Result<(), BindingError> {
        for decl in &self.decls {
            if let Decl::Stream(stream) = decl {
                stream.item().validate()?;
            }
        }
        Ok(())
    }

    fn validate_classes(&self) -> Result<(), BindingError> {
        self.decls.iter().try_for_each(|decl| match decl {
            Decl::Class(class) => class.validate(),
            _ => Ok(()),
        })
    }

    fn validate_symbol_membership(&self) -> Result<(), BindingError> {
        let registered: HashSet<&NativeSymbol> = self.symbols.symbols().iter().collect();
        let referenced = self.decls.iter().flat_map(Decl::native_symbols).try_fold(
            HashSet::new(),
            |mut referenced, symbol| {
                if !registered.contains(symbol) {
                    return Err(BindingError::new(BindingErrorKind::UnregisteredSymbol(
                        symbol.name().as_str().to_owned(),
                    )));
                }
                referenced.insert(symbol);
                Ok(referenced)
            },
        )?;
        self.symbols
            .symbols()
            .iter()
            .find(|symbol| !referenced.contains(symbol))
            .map_or(Ok(()), |symbol| {
                Err(BindingError::new(BindingErrorKind::UnreferencedSymbol(
                    symbol.name().as_str().to_owned(),
                )))
            })
    }
}

impl<S: Surface> TryFrom<UncheckedBindings<S>> for Bindings<S> {
    type Error = BindingError;

    fn try_from(unchecked: UncheckedBindings<S>) -> Result<Self, Self::Error> {
        let mut decls = unchecked.decls;
        ErrorPayloadTypes::from_decls(&decls).apply_decls(&mut decls);
        let bindings = Self {
            version: unchecked.version,
            package: unchecked.package,
            decls,
            symbols: unchecked.symbols,
        };
        bindings.validate()?;
        Ok(bindings)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum References {
    Complete,
    Present,
}

/// Target surface a lowering or a build is for.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum BindingMetadataSurface {
    /// Native dynamic-library surface.
    Native,
    /// WebAssembly `wasm32-unknown-unknown` surface.
    Wasm32,
}

impl BindingMetadataSurface {
    /// Returns the stable metadata-build environment value for this surface.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Wasm32 => "wasm32",
        }
    }

    /// Parses a metadata-build environment value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "native" => Some(Self::Native),
            "wasm32" => Some(Self::Wasm32),
            _ => None,
        }
    }

    /// Selects the surface a Cargo target triple compiles for.
    ///
    /// Any `wasm32*` triple resolves to [`Self::Wasm32`]; everything else,
    /// including the absence of an explicit triple, resolves to
    /// [`Self::Native`].
    pub fn from_target_triple(triple: Option<&str>) -> Self {
        match triple {
            Some(triple) if triple.starts_with("wasm32") => Self::Wasm32,
            _ => Self::Native,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use boltffi_ast::PackageInfo as SourcePackageInfo;
    use serde_json::Value;

    use crate::{
        BindingErrorKind, Bindings, CanonicalName, Decl, DeclarationId, DeclarationRef, FunctionId,
        Native, NativeSymbolTable, PackageInfo, RecordDecl, lower,
    };

    #[test]
    fn dependency_closed_prunes_dependency_chains() {
        let bindings = lower_bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data]
            pub struct Envelope {
                pub point: Point,
            }

            #[export]
            pub fn echo(value: Envelope) -> Envelope { value }
            "#,
        );
        let envelope = declaration_id(&bindings, "Envelope");
        let echo = declaration_id(&bindings, "echo");

        let bindings = bindings
            .dependency_closed(&BTreeSet::from([envelope, echo]))
            .expect("closed retention");

        assert!(bindings.decls().is_empty());
        assert!(bindings.symbols().symbols().is_empty());
    }

    #[test]
    fn dependency_closed_preserves_admitted_cycles() {
        let bindings = lower_bindings(
            r#"
            #[data]
            pub struct Left {
                pub right: Option<Right>,
            }

            #[data]
            pub struct Right {
                pub left: Option<Left>,
            }
            "#,
        );
        let left = declaration_id(&bindings, "Left");
        let right = declaration_id(&bindings, "Right");
        let retained_cycle = bindings
            .dependency_closed(&BTreeSet::from([left, right]))
            .expect("cycle retention");
        let broken_cycle = bindings
            .dependency_closed(&BTreeSet::from([left]))
            .expect("broken cycle retention");

        assert_eq!(
            retained_cycle
                .decls()
                .iter()
                .map(Decl::id)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([left, right])
        );
        assert!(broken_cycle.decls().is_empty());
    }

    #[test]
    fn dependency_closed_recomputes_derived_roles() {
        let bindings = lower_bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Problem {
                pub code: i32,
            }

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[repr(i32)]
            #[data]
            pub enum Failure {
                Invalid = 1,
            }

            #[export]
            pub fn fail_record() -> Result<(), Problem> { Ok(()) }

            #[export]
            pub fn fail_enum() -> Result<(), Failure> { Ok(()) }

            #[export]
            pub fn maybe(value: Option<Point>) -> Option<Point> { value }
            "#,
        );
        let problem = declaration_id(&bindings, "Problem");
        let point = declaration_id(&bindings, "Point");
        let failure = declaration_id(&bindings, "Failure");
        assert!(record(&bindings, problem).is_error_payload());
        assert!(record(&bindings, problem).is_codec_payload());
        assert!(record(&bindings, point).is_codec_payload());
        assert!(enumeration(&bindings, failure).is_error_payload());

        let bindings = bindings
            .dependency_closed(&BTreeSet::from([problem, point, failure]))
            .expect("role retention");

        assert!(!record(&bindings, problem).is_error_payload());
        assert!(!record(&bindings, problem).is_codec_payload());
        assert!(!record(&bindings, point).is_codec_payload());
        assert!(!enumeration(&bindings, failure).is_error_payload());
    }

    #[test]
    fn derived_roles_follow_nested_type_and_codec_plans() {
        let bindings = lower_bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Problem {
                pub code: i32,
            }

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[repr(i32)]
            #[data]
            pub enum Failure {
                Invalid = 1,
            }

            #[data]
            pub struct Envelope {
                pub record_results: Vec<Result<i32, Problem>>,
                pub enum_results: Option<Result<i32, Failure>>,
                pub points: Vec<Point>,
            }
            "#,
        );
        let problem = declaration_id(&bindings, "Problem");
        let point = declaration_id(&bindings, "Point");
        let failure = declaration_id(&bindings, "Failure");

        assert!(record(&bindings, problem).is_error_payload());
        assert!(record(&bindings, problem).is_codec_payload());
        assert!(record(&bindings, point).is_codec_payload());
        assert!(enumeration(&bindings, failure).is_error_payload());
    }

    #[test]
    fn dependency_closed_preserves_version_identity_order_and_first_seen_symbols() {
        let bindings = lower_bindings(
            r#"
            #[export]
            pub fn first(value: i32) -> i32 { value }

            #[export]
            pub fn second(value: i32) -> i32 { value }

            #[export]
            pub fn third(value: i32) -> i32 { value }
            "#,
        );
        let version = bindings.version();
        let package = bindings.package().clone();
        let first = declaration_id(&bindings, "first");
        let third = declaration_id(&bindings, "third");

        let bindings = bindings
            .dependency_closed(&BTreeSet::from([first, third]))
            .expect("ordered retention");

        assert_eq!(bindings.version(), version);
        assert_eq!(bindings.package(), &package);
        assert_eq!(
            bindings.decls().iter().map(Decl::id).collect::<Vec<_>>(),
            vec![first, third]
        );
        assert_eq!(
            bindings
                .symbols()
                .symbols()
                .iter()
                .map(|symbol| symbol.name().as_str())
                .collect::<Vec<_>>(),
            vec!["boltffi_function_demo_first", "boltffi_function_demo_third"]
        );
    }

    #[test]
    fn dependency_closed_rejects_unknown_ids_without_mutation() {
        let bindings = lower_bindings(
            r#"
            #[export]
            pub fn value() -> i32 { 1 }
            "#,
        );
        let original = bindings.clone();
        let unknown = DeclarationId::Function(FunctionId::from_raw(u32::MAX));

        let error = bindings
            .dependency_closed(&BTreeSet::from([unknown]))
            .expect_err("unknown declaration");

        assert_eq!(
            error.kind(),
            &BindingErrorKind::UnknownDeclarationId(unknown)
        );
        assert_eq!(bindings, original);
    }

    #[test]
    fn validation_rejects_missing_declaration_references() {
        let mut bindings = lower_bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[export]
            pub fn echo(value: Point) -> Point { value }
            "#,
        );
        let point = declaration_id(&bindings, "Point");
        let echo = declaration_id(&bindings, "echo");
        bindings
            .decls
            .retain(|declaration| declaration.id() != point);

        let error = bindings.validate().expect_err("missing record reference");

        assert_eq!(
            error.kind(),
            &BindingErrorKind::MissingDeclarationReference {
                owner: echo,
                referenced: point,
            }
        );
    }

    #[test]
    fn validation_rejects_unreferenced_native_symbols() {
        let mut bindings = lower_bindings(
            r#"
            #[export]
            pub fn first() -> i32 { 1 }

            #[export]
            pub fn second() -> i32 { 2 }
            "#,
        );
        let second = declaration_id(&bindings, "second");
        bindings
            .decls
            .retain(|declaration| declaration.id() != second);

        let error = bindings.validate().expect_err("unreferenced native symbol");

        assert_eq!(
            error.kind(),
            &BindingErrorKind::UnreferencedSymbol("boltffi_function_demo_second".to_owned())
        );
    }

    #[test]
    fn validation_rejects_referenced_symbols_missing_from_the_table() {
        let mut bindings = lower_bindings(
            r#"
            #[export]
            pub fn value() -> i32 { 1 }
            "#,
        );
        let missing = bindings.symbols().symbols()[0].clone();
        bindings.symbols = NativeSymbolTable::from_symbols(Vec::new()).expect("empty symbol table");

        let error = bindings.validate().expect_err("unregistered native symbol");

        assert_eq!(
            error.kind(),
            &BindingErrorKind::UnregisteredSymbol(missing.name().as_str().to_owned())
        );
    }

    #[test]
    fn deserialization_rejects_reference_shape_mismatches() {
        let bindings = lower_bindings(
            r#"
            #[data]
            pub struct Message {
                pub text: String,
            }

            #[export]
            pub fn echo(value: Message) -> Message { value }
            "#,
        );
        let mut value = serde_json::to_value(bindings).expect("serialized bindings");
        assert!(replace_variant(&mut value, "EncodedRecord", "DirectRecord"));

        let error = serde_json::from_value::<Bindings<Native>>(value)
            .expect_err("direct reference to encoded record");

        assert!(error.to_string().contains("as direct record"));
    }

    fn empty_native_bindings() -> Bindings<Native> {
        Bindings::from_decls(
            PackageInfo::new(CanonicalName::single("demo"), None),
            Vec::new(),
        )
        .expect("empty bindings")
    }

    fn lower_bindings(source: &str) -> Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, SourcePackageInfo::new("demo", None))
            .expect("source fixture scans");
        lower::<Native>(&source).expect("source fixture lowers")
    }

    fn declaration_id(bindings: &Bindings<Native>, name: &str) -> DeclarationId {
        bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .find(|declaration| declaration_name(*declaration) == name)
            .map(DeclarationRef::id)
            .unwrap_or_else(|| panic!("missing declaration {name}"))
    }

    fn declaration_name(declaration: DeclarationRef<'_, Native>) -> &str {
        match declaration {
            DeclarationRef::Record(record) => record.name(),
            DeclarationRef::Enum(enumeration) => enumeration.name(),
            DeclarationRef::Function(function) => function.name(),
            DeclarationRef::Class(class) => class.name(),
            DeclarationRef::Callback(callback) => callback.name(),
            DeclarationRef::Stream(stream) => stream.name(),
            DeclarationRef::Constant(constant) => constant.name(),
            DeclarationRef::CustomType(custom_type) => custom_type.name(),
        }
        .source_spelling()
        .expect("source spelling")
    }

    fn record(bindings: &Bindings<Native>, id: DeclarationId) -> &RecordDecl<Native> {
        bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .find(|declaration| declaration.id() == id)
            .and_then(DeclarationRef::record)
            .expect("record declaration")
    }

    fn enumeration(bindings: &Bindings<Native>, id: DeclarationId) -> &crate::EnumDecl<Native> {
        bindings
            .decls()
            .iter()
            .map(DeclarationRef::from)
            .find(|declaration| declaration.id() == id)
            .and_then(DeclarationRef::enumeration)
            .expect("enum declaration")
    }

    fn replace_variant(value: &mut Value, from: &str, to: &str) -> bool {
        match value {
            Value::Array(values) => values
                .iter_mut()
                .any(|value| replace_variant(value, from, to)),
            Value::Object(fields) => match fields.remove(from) {
                Some(payload) => {
                    fields.insert(to.to_owned(), payload);
                    true
                }
                None => fields
                    .values_mut()
                    .any(|value| replace_variant(value, from, to)),
            },
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
    }
}
