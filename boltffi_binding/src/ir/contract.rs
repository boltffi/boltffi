use std::{
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    error, fmt,
};

use serde::{Deserialize, Serialize};

use super::error_payloads::ErrorPayloadTypes;
use super::reference::DeclarationReferences;

use crate::{
    BindingError, BindingErrorKind, CanonicalName, Decl, DeclarationId, DeclarationRef, Native,
    NativeSymbol, NativeSymbolTable, Surface, Wasm32,
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
        self.version.validate()?;
        self.symbols.validate()?;
        self.validate_unique_decl_ids()?;
        self.validate_references()?;
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
        mut decls: Vec<Decl<S>>,
    ) -> Result<Self, BindingError> {
        ErrorPayloadTypes::from_decls(&decls).apply_decls(&mut decls);
        let symbols = NativeSymbolTable::from_decls(&decls)?;
        let bindings = Self {
            version,
            package,
            decls,
            symbols,
        };
        bindings.validate()?;
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

    fn validate_references(&self) -> Result<(), BindingError> {
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

/// A binding contract paired with its target surface tag.
///
/// Used at the storage boundary: in-process the `Bindings<S>` types are
/// generic, but a `.rlib` artifact (or any byte stream a tool consumes)
/// needs to identify its surface at run time. The variant tag carries
/// that identity; downstream tooling pattern-matches once and dispatches
/// the inner value to a backend typed against the surface.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum SerializedBindings {
    /// Bindings produced for the [`Native`] surface.
    Native(Bindings<Native>),
    /// Bindings produced for the [`Wasm32`] surface.
    Wasm32(Bindings<Wasm32>),
}

impl SerializedBindings {
    /// Wraps a native-surface bindings value.
    pub fn native(bindings: Bindings<Native>) -> Self {
        Self::Native(bindings)
    }

    /// Wraps a wasm32-surface bindings value.
    pub fn wasm32(bindings: Bindings<Wasm32>) -> Self {
        Self::Wasm32(bindings)
    }

    /// Returns the surface tag carried by this serialized contract.
    pub const fn surface(&self) -> BindingMetadataSurface {
        match self {
            Self::Native(_) => BindingMetadataSurface::Native,
            Self::Wasm32(_) => BindingMetadataSurface::Wasm32,
        }
    }

    /// Returns the package carried by this serialized contract.
    pub fn package(&self) -> &PackageInfo {
        match self {
            Self::Native(bindings) => bindings.package(),
            Self::Wasm32(bindings) => bindings.package(),
        }
    }

    fn payload_bytes(&self) -> Result<Vec<u8>, BindingMetadataError> {
        serde_json::to_vec(self).map_err(BindingMetadataError::encode)
    }
}

const BINDING_METADATA_MAGIC: &str = "boltffi.bindings";
const BINDING_METADATA_RECORD_MAGIC: &[u8; 8] = b"BFFIMD01";
const BINDING_METADATA_RECORD_LENGTH_LEN: usize = std::mem::size_of::<u64>();
const BINDING_METADATA_RECORD_HEADER_LEN: usize =
    BINDING_METADATA_RECORD_MAGIC.len() + BINDING_METADATA_RECORD_LENGTH_LEN;
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Envelope format version for serialized binding metadata.
///
/// This version describes the outer metadata wrapper, not the
/// `Bindings` schema inside the payload.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct BindingMetadataFormat {
    major: u16,
    minor: u16,
}

impl BindingMetadataFormat {
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

    /// Returns `true` when this crate can read the format.
    pub const fn readable(self) -> bool {
        self.major == Self::CURRENT.major && self.minor <= Self::CURRENT.minor
    }
}

/// Target surface tag written into a metadata envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum BindingMetadataSurface {
    /// Native dynamic-library surface.
    Native,
    /// WebAssembly `wasm32-unknown-unknown` surface.
    Wasm32,
}

/// Environment variable the bindgen metadata build sets to switch the macro
/// metadata emitter on.
///
/// The variable carries no payload: its presence is the whole signal. The
/// metadata build sets it, the proc-macro reads it, and normal builds leave
/// it unset so they never pay for the scan-and-lower pass.
pub const BINDING_METADATA_BUILD_ENV: &str = "BOLTFFI_BINDING_METADATA";

/// Environment variable carrying the crate source root the macro must scan.
///
/// Bindgen resolves the target's source path through Cargo and sets this so
/// the proc-macro never guesses crate layout. The value is an absolute path
/// to the crate root file, such as the `src_path` Cargo reports for the
/// library target.
pub const BINDING_METADATA_SOURCE_ENV: &str = "BOLTFFI_BINDING_METADATA_SOURCE";

/// Environment variable carrying the manifest directory of the crate bindgen
/// is generating bindings for.
///
/// `RUSTFLAGS` enables the metadata cfg for the whole build graph, so the
/// macro fires inside dependency crates too. The macro emits metadata only
/// when its own `CARGO_MANIFEST_DIR` matches this value, which keeps a single
/// generation targeted at one crate. Per-dependency metadata is a multicrate
/// concern handled separately.
pub const BINDING_METADATA_ROOT_ENV: &str = "BOLTFFI_BINDING_METADATA_ROOT";

/// Environment variable carrying the metadata surface requested by bindgen.
///
/// The macro uses this value to lower only the target surface Cargo is
/// building for, instead of requiring every source crate to lower every
/// possible surface during metadata extraction.
pub const BINDING_METADATA_SURFACE_ENV: &str = "BOLTFFI_BINDING_METADATA_SURFACE";

/// Environment variable carrying active Cargo features for IR source scanning.
///
/// Cargo feature cfgs are active during the crate build, but proc-macros do
/// not reliably see `CARGO_FEATURE_*` for the target crate. Metadata and
/// wrapper-expansion builds resolve the active local features and pass them
/// through this variable so source scanning applies the same cfg gates as the
/// build.
pub const BINDING_METADATA_FEATURES_ENV: &str = "BOLTFFI_BINDING_METADATA_FEATURES";

/// Environment variable the build orchestrator sets to switch IR wrapper
/// expansion on.
///
/// The variable carries no payload: its presence is the whole signal. Normal
/// macro expansion leaves it unset, so existing legacy expansion remains the
/// default path.
pub const BINDING_EXPANSION_BUILD_ENV: &str = "BOLTFFI_BINDING_EXPANSION";

/// Environment variable carrying the crate source root the IR wrapper build
/// must scan.
pub const BINDING_EXPANSION_SOURCE_ENV: &str = "BOLTFFI_BINDING_EXPANSION_SOURCE";

/// Environment variable carrying the manifest directory of the crate whose IR
/// wrappers are being compiled.
pub const BINDING_EXPANSION_ROOT_ENV: &str = "BOLTFFI_BINDING_EXPANSION_ROOT";

/// Environment variable carrying the target surface requested for IR wrapper
/// expansion.
pub const BINDING_EXPANSION_SURFACE_ENV: &str = "BOLTFFI_BINDING_EXPANSION_SURFACE";

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

/// A linker section used to store binding metadata records.
///
/// The section names are intentionally short enough for the object
/// formats that impose fixed section-name limits. Apple targets use a
/// Mach-O segment and section pair; other supported object formats use
/// one short section name.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BindingMetadataSection {
    /// Mach-O `__DATA,__boltffi` section.
    MachO,
    /// ELF and COFF `.boltffi` section.
    Object,
}

impl BindingMetadataSection {
    /// Returns the string accepted by `#[link_section]`.
    pub const fn link_section(self) -> &'static str {
        match self {
            Self::MachO => "__DATA,__boltffi",
            Self::Object => ".boltffi",
        }
    }

    /// Returns the section component stored in the object file.
    pub const fn section_name(self) -> &'static str {
        match self {
            Self::MachO => "__boltffi",
            Self::Object => ".boltffi",
        }
    }

    /// Returns the Mach-O segment component when this section has one.
    pub const fn segment_name(self) -> Option<&'static str> {
        match self {
            Self::MachO => Some("__DATA"),
            Self::Object => None,
        }
    }

    /// Returns `true` when object-file section metadata matches this
    /// section.
    pub fn matches(self, section_name: &str, segment_name: Option<&str>) -> bool {
        section_name == self.section_name()
            && match self.segment_name() {
                Some(expected) => segment_name == Some(expected),
                None => true,
            }
    }
}

/// Stable hash of the serialized binding payload.
///
/// The value is used to deduplicate repeated metadata blobs emitted by
/// multiple macro invocations in the same crate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct BindingMetadataHash(u64);

impl BindingMetadataHash {
    /// Hashes serialized binding bytes.
    pub fn new(bytes: &[u8]) -> Self {
        Self(bytes.iter().fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        }))
    }

    /// Returns the raw hash value.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the hash as sixteen lowercase hexadecimal digits.
    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

impl fmt::Display for BindingMetadataHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

/// Serialized binding metadata embedded in a compiled Rust artifact.
///
/// The envelope carries a magic string, wrapper format version, surface
/// tag, payload hash, and the validated `SerializedBindings` payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct BindingMetadataEnvelope {
    magic: String,
    format: BindingMetadataFormat,
    surface: BindingMetadataSurface,
    package: PackageInfo,
    contract_hash: BindingMetadataHash,
    bindings: SerializedBindings,
}

impl BindingMetadataEnvelope {
    /// Builds an envelope around serialized bindings.
    pub fn new(bindings: SerializedBindings) -> Result<Self, BindingMetadataError> {
        let contract_hash = BindingMetadataHash::new(&bindings.payload_bytes()?);
        Ok(Self {
            magic: BINDING_METADATA_MAGIC.to_owned(),
            format: BindingMetadataFormat::current(),
            surface: bindings.surface(),
            package: bindings.package().clone(),
            contract_hash,
            bindings,
        })
    }

    /// Decodes and validates a metadata envelope.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BindingMetadataError> {
        let envelope =
            serde_json::from_slice::<Self>(bytes).map_err(BindingMetadataError::decode)?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Serializes this metadata envelope.
    pub fn to_bytes(&self) -> Result<Vec<u8>, BindingMetadataError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(BindingMetadataError::encode)
    }

    /// Serializes this envelope as one self-delimiting linker-section
    /// record.
    pub fn to_section_bytes(&self) -> Result<Vec<u8>, BindingMetadataError> {
        let payload = self.to_bytes()?;
        let length = u64::try_from(payload.len()).map_err(|_| {
            BindingMetadataError::SectionRecordPayloadTooLarge {
                length: payload.len(),
            }
        })?;
        Ok(BINDING_METADATA_RECORD_MAGIC
            .iter()
            .copied()
            .chain(length.to_le_bytes())
            .chain(payload)
            .collect())
    }

    /// Returns the metadata format version.
    pub const fn format(&self) -> BindingMetadataFormat {
        self.format
    }

    /// Returns the target surface tag.
    pub const fn surface(&self) -> BindingMetadataSurface {
        self.surface
    }

    /// Returns the producing package.
    pub fn package(&self) -> &PackageInfo {
        &self.package
    }

    /// Returns the hash of the serialized binding payload.
    pub const fn contract_hash(&self) -> BindingMetadataHash {
        self.contract_hash
    }

    /// Returns the serialized binding payload.
    pub const fn bindings(&self) -> &SerializedBindings {
        &self.bindings
    }

    /// Consumes the envelope and returns the serialized binding payload.
    pub fn into_bindings(self) -> SerializedBindings {
        self.bindings
    }

    fn validate(&self) -> Result<(), BindingMetadataError> {
        if self.magic != BINDING_METADATA_MAGIC {
            return Err(BindingMetadataError::InvalidMagic {
                actual: self.magic.clone(),
            });
        }
        if !self.format.readable() {
            return Err(BindingMetadataError::UnsupportedFormat {
                actual: self.format,
                current: BindingMetadataFormat::current(),
            });
        }
        if self.surface != self.bindings.surface() {
            return Err(BindingMetadataError::SurfaceMismatch {
                envelope: self.surface,
                payload: self.bindings.surface(),
            });
        }
        if &self.package != self.bindings.package() {
            return Err(BindingMetadataError::PackageMismatch {
                envelope: Box::new(self.package.clone()),
                payload: Box::new(self.bindings.package().clone()),
            });
        }
        let actual = BindingMetadataHash::new(&self.bindings.payload_bytes()?);
        if self.contract_hash != actual {
            return Err(BindingMetadataError::HashMismatch {
                expected: self.contract_hash,
                actual,
            });
        }
        Ok(())
    }
}

/// Bytes read from a compiled artifact metadata section.
///
/// A section may contain several records because each macro expansion
/// can emit the same contract metadata. Records are length-prefixed so
/// concatenated statics remain parseable after the linker merges them
/// into one section.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BindingMetadataSectionBytes<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> BindingMetadataSectionBytes<'bytes> {
    /// Stores the raw section bytes.
    pub const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes }
    }

    /// Decodes every metadata record in section order.
    pub fn envelopes(self) -> Result<Vec<BindingMetadataEnvelope>, BindingMetadataError> {
        let mut offset = 0;
        std::iter::from_fn(|| {
            (offset < self.bytes.len()).then(|| {
                let record = self.record_at(offset);
                offset = record.as_ref().map_or(self.bytes.len(), |(_, next)| *next);
                record.map(|(envelope, _)| envelope)
            })
        })
        .collect()
    }

    fn record_at(
        self,
        offset: usize,
    ) -> Result<(BindingMetadataEnvelope, usize), BindingMetadataError> {
        let header_end = offset
            .checked_add(BINDING_METADATA_RECORD_HEADER_LEN)
            .ok_or(BindingMetadataError::SectionRecordLengthOverflow { offset })?;
        let header = self
            .bytes
            .get(offset..header_end)
            .ok_or(BindingMetadataError::TruncatedSectionRecord { offset })?;
        if &header[..BINDING_METADATA_RECORD_MAGIC.len()] != BINDING_METADATA_RECORD_MAGIC {
            return Err(BindingMetadataError::InvalidSectionRecordMagic { offset });
        }
        let length_bytes = header[BINDING_METADATA_RECORD_MAGIC.len()..]
            .try_into()
            .expect("metadata record length header is always eight bytes");
        let payload_length = u64::from_le_bytes(length_bytes);
        let payload_length = usize::try_from(payload_length).map_err(|_| {
            BindingMetadataError::SectionRecordTooLarge {
                offset,
                length: payload_length,
            }
        })?;
        let payload_start = header_end;
        let payload_end = payload_start
            .checked_add(payload_length)
            .ok_or(BindingMetadataError::SectionRecordLengthOverflow { offset })?;
        let payload = self
            .bytes
            .get(payload_start..payload_end)
            .ok_or(BindingMetadataError::TruncatedSectionRecord { offset })?;
        BindingMetadataEnvelope::from_bytes(payload).map(|envelope| (envelope, payload_end))
    }
}

/// Metadata envelope serialization failure.
#[derive(Debug)]
pub enum BindingMetadataError {
    /// The envelope could not be serialized.
    Encode {
        /// Serialization error text.
        message: String,
    },
    /// The envelope bytes could not be decoded.
    Decode {
        /// Deserialization error text.
        message: String,
    },
    /// The magic string is not a BoltFFI binding metadata marker.
    InvalidMagic {
        /// Magic string found in the metadata.
        actual: String,
    },
    /// The envelope format is not readable by this crate.
    UnsupportedFormat {
        /// Format found in the metadata.
        actual: BindingMetadataFormat,
        /// Format supported by this crate.
        current: BindingMetadataFormat,
    },
    /// The envelope surface does not match the payload surface.
    SurfaceMismatch {
        /// Surface written on the envelope.
        envelope: BindingMetadataSurface,
        /// Surface carried by the payload.
        payload: BindingMetadataSurface,
    },
    /// The envelope package does not match the payload package.
    PackageMismatch {
        /// Package written on the envelope.
        envelope: Box<PackageInfo>,
        /// Package carried by the payload.
        payload: Box<PackageInfo>,
    },
    /// The payload hash does not match the serialized payload.
    HashMismatch {
        /// Hash written on the envelope.
        expected: BindingMetadataHash,
        /// Hash computed from the payload.
        actual: BindingMetadataHash,
    },
    /// An envelope payload is too large to frame for linker-section
    /// storage.
    SectionRecordPayloadTooLarge {
        /// Payload length in bytes.
        length: usize,
    },
    /// A linker-section record does not start with the BoltFFI record
    /// marker.
    InvalidSectionRecordMagic {
        /// Byte offset of the invalid record.
        offset: usize,
    },
    /// A linker-section record ended before its header or payload was
    /// complete.
    TruncatedSectionRecord {
        /// Byte offset of the truncated record.
        offset: usize,
    },
    /// A linker-section record length cannot be represented on this
    /// platform.
    SectionRecordTooLarge {
        /// Byte offset of the record.
        offset: usize,
        /// Payload length written in the record header.
        length: u64,
    },
    /// A linker-section record length overflows the enclosing section.
    SectionRecordLengthOverflow {
        /// Byte offset of the record.
        offset: usize,
    },
}

impl BindingMetadataError {
    fn encode(error: serde_json::Error) -> Self {
        Self::Encode {
            message: error.to_string(),
        }
    }

    fn decode(error: serde_json::Error) -> Self {
        Self::Decode {
            message: error.to_string(),
        }
    }
}

impl fmt::Display for BindingMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode { message } => {
                write!(formatter, "binding metadata encode failed: {message}")
            }
            Self::Decode { message } => {
                write!(formatter, "binding metadata decode failed: {message}")
            }
            Self::InvalidMagic { actual } => {
                write!(formatter, "invalid binding metadata magic: {actual}")
            }
            Self::UnsupportedFormat { actual, current } => write!(
                formatter,
                "unsupported binding metadata format {}.{}, current {}.{}",
                actual.major(),
                actual.minor(),
                current.major(),
                current.minor()
            ),
            Self::SurfaceMismatch { envelope, payload } => {
                write!(
                    formatter,
                    "binding metadata surface mismatch: envelope {envelope:?}, payload {payload:?}"
                )
            }
            Self::PackageMismatch { envelope, payload } => {
                write!(
                    formatter,
                    "binding metadata package mismatch: envelope {envelope:?}, payload {payload:?}"
                )
            }
            Self::HashMismatch { expected, actual } => {
                write!(
                    formatter,
                    "binding metadata hash mismatch: expected {expected}, actual {actual}"
                )
            }
            Self::SectionRecordPayloadTooLarge { length } => {
                write!(
                    formatter,
                    "binding metadata payload is too large for a section record: {length} bytes"
                )
            }
            Self::InvalidSectionRecordMagic { offset } => {
                write!(
                    formatter,
                    "invalid binding metadata section record magic at byte {offset}"
                )
            }
            Self::TruncatedSectionRecord { offset } => {
                write!(
                    formatter,
                    "truncated binding metadata section record at byte {offset}"
                )
            }
            Self::SectionRecordTooLarge { offset, length } => {
                write!(
                    formatter,
                    "binding metadata section record at byte {offset} is too large: {length} bytes"
                )
            }
            Self::SectionRecordLengthOverflow { offset } => {
                write!(
                    formatter,
                    "binding metadata section record length overflows at byte {offset}"
                )
            }
        }
    }
}

impl error::Error for BindingMetadataError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use boltffi_ast::PackageInfo as SourcePackageInfo;
    use serde_json::{Value, json};

    use crate::{
        BindingErrorKind, BindingMetadataEnvelope, BindingMetadataError, BindingMetadataSection,
        BindingMetadataSectionBytes, Bindings, CanonicalName, Decl, DeclarationId, DeclarationRef,
        FunctionId, Native, NativeSymbolTable, PackageInfo, RecordDecl, SerializedBindings, lower,
    };

    #[test]
    fn metadata_envelope_round_trips_native_bindings() {
        let bindings = empty_native_bindings();
        let envelope = BindingMetadataEnvelope::new(SerializedBindings::native(bindings))
            .expect("metadata envelope");

        let bytes = envelope.to_bytes().expect("metadata bytes");
        let decoded = BindingMetadataEnvelope::from_bytes(&bytes).expect("decoded metadata");

        assert_eq!(decoded.surface(), envelope.surface());
        assert_eq!(decoded.package(), envelope.package());
        assert_eq!(decoded.contract_hash(), envelope.contract_hash());
        assert_eq!(decoded.bindings(), envelope.bindings());
    }

    #[test]
    fn metadata_envelope_rejects_modified_hash() {
        let envelope =
            BindingMetadataEnvelope::new(SerializedBindings::native(empty_native_bindings()))
                .expect("metadata envelope");
        let mut value = serde_json::to_value(&envelope).expect("metadata value");
        let Value::Object(object) = &mut value else {
            panic!("metadata envelope must serialize as object");
        };
        object.insert("contract_hash".to_owned(), json!(0));

        let bytes = serde_json::to_vec(&value).expect("metadata bytes");
        let error = BindingMetadataEnvelope::from_bytes(&bytes).expect_err("hash mismatch");

        assert!(matches!(error, BindingMetadataError::HashMismatch { .. }));
    }

    #[test]
    fn metadata_section_names_fit_object_format_limits() {
        assert!(BindingMetadataSection::MachO.section_name().len() <= 16);
        assert!(BindingMetadataSection::Object.section_name().len() <= 8);
    }

    #[test]
    fn metadata_section_bytes_decode_repeated_records() {
        let envelope =
            BindingMetadataEnvelope::new(SerializedBindings::native(empty_native_bindings()))
                .expect("metadata envelope");
        let mut section = envelope.to_section_bytes().expect("section record");
        section.extend(envelope.to_section_bytes().expect("second section record"));

        let decoded = BindingMetadataSectionBytes::new(&section)
            .envelopes()
            .expect("section records");

        assert_eq!(
            decoded
                .iter()
                .map(BindingMetadataEnvelope::contract_hash)
                .collect::<Vec<_>>(),
            vec![envelope.contract_hash(), envelope.contract_hash()]
        );
    }

    #[test]
    fn metadata_section_bytes_reject_raw_envelope_json() {
        let envelope =
            BindingMetadataEnvelope::new(SerializedBindings::native(empty_native_bindings()))
                .expect("metadata envelope");
        let bytes = envelope.to_bytes().expect("raw envelope bytes");

        let error = BindingMetadataSectionBytes::new(&bytes)
            .envelopes()
            .expect_err("raw json is not a section record");

        assert!(matches!(
            error,
            BindingMetadataError::InvalidSectionRecordMagic { offset: 0 }
        ));
    }

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
