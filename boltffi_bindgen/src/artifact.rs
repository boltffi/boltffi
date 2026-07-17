//! Reads BoltFFI binding metadata from compiled Rust artifacts.
//!
//! The reader owns object-file and archive traversal. The metadata
//! schema, section names, record framing, and contract validation stay
//! in `boltffi_binding`.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use boltffi_binding::{
    BindingMetadataEnvelope, BindingMetadataError, BindingMetadataHash, BindingMetadataSection,
    BindingMetadataSectionBytes, RawSourceRecord, SourceRecordError, SourceRecordSectionBytes,
    is_source_record_section,
};
use object::read::archive::{ArchiveFile, ArchiveMember};
use object::read::macho::{FatArch, MachOFatFile};
use object::{File, FileKind, Object, ObjectSection};
use thiserror::Error;

/// Reads binding metadata from compiled Rust artifacts.
///
/// The reader accepts object files, shared libraries, static libraries,
/// and Rust archives. Every metadata record is validated by
/// `boltffi_binding` before it is returned. Repeated records with the
/// same contract hash are returned once.
pub struct BindingMetadataReader {
    artifacts: Vec<PathBuf>,
}

impl BindingMetadataReader {
    /// Creates a reader for compiled artifact paths.
    pub fn new(artifacts: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            artifacts: artifacts.into_iter().collect(),
        }
    }

    /// Reads validated metadata envelopes from every artifact.
    pub fn read(&self) -> Result<Vec<BindingMetadataEnvelope>, BindingMetadataReadError> {
        self.payloads::<BindingMetadataEnvelope>()
            .map(|envelopes| DeduplicatedEnvelopes::from_envelopes(envelopes).into_vec())
    }

    /// Reads per-invocation source records from every artifact.
    pub fn read_source_records(&self) -> Result<Vec<RawSourceRecord>, BindingMetadataReadError> {
        self.payloads::<RawSourceRecord>()
    }

    fn payloads<P: SectionPayload>(&self) -> Result<Vec<P>, BindingMetadataReadError> {
        self.artifacts
            .iter()
            .map(|artifact| ArtifactBytes::read(artifact))
            .map(|artifact| artifact.and_then(|artifact| artifact.payloads()))
            .collect::<Result<Vec<_>, _>>()
            .map(|artifacts| artifacts.into_iter().flatten().collect())
    }

    /// Reads validated metadata envelopes and rejects empty results.
    pub fn read_required(&self) -> Result<Vec<BindingMetadataEnvelope>, BindingMetadataReadError> {
        let envelopes = self.read()?;
        if envelopes.is_empty() {
            Err(BindingMetadataReadError::NoMetadata {
                artifacts: self.artifacts.clone(),
            })
        } else {
            Ok(envelopes)
        }
    }
}

/// Failure while reading binding metadata from compiled artifacts.
#[derive(Debug, Error)]
pub enum BindingMetadataReadError {
    /// The artifact bytes could not be read from disk.
    #[error("read artifact `{path}`: {source}")]
    Read {
        /// Artifact path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// The artifact object format could not be parsed.
    #[error("parse artifact `{path}`: {source}")]
    Parse {
        /// Artifact path.
        path: PathBuf,
        /// Object parser error.
        source: object::Error,
    },
    /// A metadata section record failed validation.
    #[error("decode binding metadata from `{path}`: {source}")]
    Metadata {
        /// Artifact path.
        path: PathBuf,
        /// Metadata validation error.
        source: BindingMetadataError,
    },
    /// A source-record section failed to decode.
    #[error("decode source records from `{path}`: {source}")]
    SourceRecord {
        /// Artifact path.
        path: PathBuf,
        /// Source record decoding error.
        source: SourceRecordError,
    },
    /// No binding metadata records were found in the artifact set.
    #[error("no BoltFFI binding metadata found in compiled artifacts: {artifacts:?}")]
    NoMetadata {
        /// Artifact paths that were searched.
        artifacts: Vec<PathBuf>,
    },
}

struct ArtifactBytes {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl ArtifactBytes {
    fn read(path: &Path) -> Result<Self, BindingMetadataReadError> {
        fs::read(path)
            .map(|bytes| Self {
                path: path.to_path_buf(),
                bytes,
            })
            .map_err(|source| BindingMetadataReadError::Read {
                path: path.to_path_buf(),
                source,
            })
    }

    fn payloads<P: SectionPayload>(&self) -> Result<Vec<P>, BindingMetadataReadError> {
        ArtifactImage::new(&self.path, &self.bytes).payloads()
    }
}

/// One decodable payload kind stored in a dedicated artifact section.
trait SectionPayload: Sized {
    fn matches_section(name: &str, segment: Option<&str>) -> bool;
    fn decode_section(path: &Path, bytes: &[u8]) -> Result<Vec<Self>, BindingMetadataReadError>;
}

impl SectionPayload for BindingMetadataEnvelope {
    fn matches_section(name: &str, segment: Option<&str>) -> bool {
        [
            BindingMetadataSection::MachO,
            BindingMetadataSection::Object,
        ]
        .into_iter()
        .any(|section| section.matches(name, segment))
    }

    fn decode_section(path: &Path, bytes: &[u8]) -> Result<Vec<Self>, BindingMetadataReadError> {
        BindingMetadataSectionBytes::new(bytes)
            .envelopes()
            .map_err(|source| BindingMetadataReadError::Metadata {
                path: path.to_path_buf(),
                source,
            })
    }
}

impl SectionPayload for RawSourceRecord {
    fn matches_section(name: &str, _segment: Option<&str>) -> bool {
        is_source_record_section(name)
    }

    fn decode_section(path: &Path, bytes: &[u8]) -> Result<Vec<Self>, BindingMetadataReadError> {
        SourceRecordSectionBytes::new(bytes)
            .records()
            .map_err(|source| BindingMetadataReadError::SourceRecord {
                path: path.to_path_buf(),
                source,
            })
    }
}

#[derive(Clone, Copy)]
struct ArtifactImage<'artifact> {
    path: &'artifact Path,
    bytes: &'artifact [u8],
}

impl<'artifact> ArtifactImage<'artifact> {
    const fn new(path: &'artifact Path, bytes: &'artifact [u8]) -> Self {
        Self { path, bytes }
    }

    fn payloads<P: SectionPayload>(self) -> Result<Vec<P>, BindingMetadataReadError> {
        match FileKind::parse(self.bytes).map_err(|source| self.parse_error(source))? {
            FileKind::Archive => self.archive_payloads(),
            FileKind::MachOFat32 => self.macho_fat_payloads::<object::macho::FatArch32, P>(),
            FileKind::MachOFat64 => self.macho_fat_payloads::<object::macho::FatArch64, P>(),
            _ => self.object_payloads(self.bytes),
        }
    }

    fn archive_payloads<P: SectionPayload>(self) -> Result<Vec<P>, BindingMetadataReadError> {
        ArchiveFile::parse(self.bytes)
            .map_err(|source| self.parse_error(source))?
            .members()
            .map(|member| {
                member
                    .map_err(|source| self.parse_error(source))
                    .and_then(|member| self.archive_member_payloads(member))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|members| members.into_iter().flatten().collect())
    }

    fn archive_member_payloads<P: SectionPayload>(
        self,
        member: ArchiveMember<'artifact>,
    ) -> Result<Vec<P>, BindingMetadataReadError> {
        if member.is_thin() {
            return self.thin_archive_member_payloads(member.name());
        }
        let bytes = member
            .data(self.bytes)
            .map_err(|source| self.parse_error(source))?;
        self.nested_payloads(bytes)
    }

    fn thin_archive_member_payloads<P: SectionPayload>(
        self,
        name: &[u8],
    ) -> Result<Vec<P>, BindingMetadataReadError> {
        let path = ThinArchiveMember::new(self.path, name)?.path();
        fs::read(&path)
            .map_err(|source| BindingMetadataReadError::Read {
                path: path.clone(),
                source,
            })
            .and_then(|bytes| ArtifactImage::new(&path, &bytes).payloads())
    }

    fn nested_payloads<P: SectionPayload>(
        self,
        bytes: &'artifact [u8],
    ) -> Result<Vec<P>, BindingMetadataReadError> {
        match FileKind::parse(bytes) {
            Ok(FileKind::Archive) => ArtifactImage::new(self.path, bytes).archive_payloads(),
            Ok(FileKind::MachOFat32) => ArtifactImage::new(self.path, bytes)
                .macho_fat_payloads::<object::macho::FatArch32, P>(),
            Ok(FileKind::MachOFat64) => ArtifactImage::new(self.path, bytes)
                .macho_fat_payloads::<object::macho::FatArch64, P>(),
            Ok(file_kind) if ArchiveMemberKind::from(file_kind).is_object() => {
                self.object_payloads(bytes)
            }
            Ok(_) | Err(_) => Ok(Vec::new()),
        }
    }

    fn macho_fat_payloads<Fat, P>(self) -> Result<Vec<P>, BindingMetadataReadError>
    where
        Fat: FatArch,
        P: SectionPayload,
    {
        MachOFatFile::<Fat>::parse(self.bytes)
            .map_err(|source| self.parse_error(source))?
            .arches()
            .iter()
            .map(|arch| {
                arch.data(self.bytes)
                    .map_err(|source| self.parse_error(source))
                    .and_then(|bytes| ArtifactImage::new(self.path, bytes).payloads())
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|arches| arches.into_iter().flatten().collect())
    }

    fn object_payloads<P: SectionPayload>(
        self,
        bytes: &'artifact [u8],
    ) -> Result<Vec<P>, BindingMetadataReadError> {
        let file = File::parse(bytes).map_err(|source| self.parse_error(source))?;
        file.sections()
            .map(|section| self.section_payloads(section))
            .collect::<Result<Vec<_>, _>>()
            .map(|sections| sections.into_iter().flatten().collect())
    }

    fn section_payloads<P: SectionPayload>(
        self,
        section: impl ObjectSection<'artifact>,
    ) -> Result<Vec<P>, BindingMetadataReadError> {
        let name = section.name().map_err(|source| self.parse_error(source))?;
        let segment = section
            .segment_name()
            .map_err(|source| self.parse_error(source))?;
        if !P::matches_section(name, segment) {
            return Ok(Vec::new());
        }
        section
            .uncompressed_data()
            .map_err(|source| self.parse_error(source))
            .and_then(|bytes: Cow<'artifact, [u8]>| P::decode_section(self.path, bytes.as_ref()))
    }

    fn parse_error(self, source: object::Error) -> BindingMetadataReadError {
        BindingMetadataReadError::Parse {
            path: self.path.to_path_buf(),
            source,
        }
    }
}

struct ThinArchiveMember {
    archive: PathBuf,
    member: PathBuf,
}

impl ThinArchiveMember {
    fn new(archive: &Path, name: &[u8]) -> Result<Self, BindingMetadataReadError> {
        let member = std::str::from_utf8(name)
            .map(PathBuf::from)
            .map_err(|source| BindingMetadataReadError::Read {
                path: archive.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
            })?;
        Ok(Self {
            archive: archive.to_path_buf(),
            member,
        })
    }

    fn path(self) -> PathBuf {
        if self.member.is_absolute() {
            self.member
        } else if let Some(parent) = self.archive.parent() {
            parent.join(self.member)
        } else {
            self.member
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArchiveMemberKind {
    Object,
    NonObject,
}

impl ArchiveMemberKind {
    const fn from(file_kind: FileKind) -> Self {
        match file_kind {
            FileKind::Archive
            | FileKind::CoffImport
            | FileKind::DyldCache
            | FileKind::MachOFat32
            | FileKind::MachOFat64 => Self::NonObject,
            _ => Self::Object,
        }
    }

    const fn is_object(self) -> bool {
        matches!(self, Self::Object)
    }
}

#[derive(Default)]
struct DeduplicatedEnvelopes {
    seen: HashSet<BindingMetadataHash>,
    envelopes: Vec<BindingMetadataEnvelope>,
}

impl DeduplicatedEnvelopes {
    fn from_envelopes(envelopes: impl IntoIterator<Item = BindingMetadataEnvelope>) -> Self {
        envelopes.into_iter().fold(Self::default(), Self::insert)
    }

    fn insert(mut self, envelope: BindingMetadataEnvelope) -> Self {
        if self.seen.insert(envelope.contract_hash()) {
            self.envelopes.push(envelope);
        }
        self
    }

    fn into_vec(self) -> Vec<BindingMetadataEnvelope> {
        self.envelopes
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_ast::{PackageInfo, SourceContract};
    use boltffi_binding::{
        BindingMetadataEnvelope, BindingMetadataSection, Native, SerializedBindings,
        lower_with_declarations,
    };

    use super::{BindingMetadataReadError, BindingMetadataReader};

    #[test]
    fn reads_metadata_from_compiled_static_library() {
        let artifact = MetadataArtifact::compile();

        let envelopes = BindingMetadataReader::new([artifact.path()])
            .read()
            .expect("artifact metadata reads");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].package().name().as_path_string(), "demo");
    }

    #[test]
    fn repeated_metadata_records_are_deduplicated() {
        let artifact = MetadataArtifact::compile_with_repeated_record();

        let envelopes = BindingMetadataReader::new([artifact.path()])
            .read_required()
            .expect("artifact metadata reads");

        assert_eq!(envelopes.len(), 1);
    }

    #[test]
    fn required_read_rejects_artifact_without_metadata() {
        let artifact = MetadataArtifact::compile_without_metadata();

        let error = BindingMetadataReader::new([artifact.path()])
            .read_required()
            .expect_err("metadata is required");

        assert!(matches!(error, BindingMetadataReadError::NoMetadata { .. }));
    }

    #[test]
    fn archive_member_with_invalid_object_payload_is_rejected() {
        let artifact = RawArtifact::new("libbroken.a", malformed_archive_member());

        let error = BindingMetadataReader::new([artifact.path()])
            .read_required()
            .expect_err("broken object member must reject");

        assert!(matches!(error, BindingMetadataReadError::Parse { .. }));
    }

    #[test]
    fn archive_member_with_non_object_binary_payload_is_ignored() {
        let artifact = RawArtifact::new("libfat.a", non_object_binary_archive_member());

        let error = BindingMetadataReader::new([artifact.path()])
            .read_required()
            .expect_err("non-object archive member has no metadata");

        assert!(matches!(error, BindingMetadataReadError::NoMetadata { .. }));
    }

    #[test]
    fn mach_o_fat_artifact_reads_metadata_from_arch_slice() {
        let artifact = MetadataArtifact::compile();
        let bytes = fs::read(artifact.path()).expect("read metadata artifact");
        let fat = RawArtifact::new("libdemo-universal", mach_o_fat32(&bytes));

        let envelopes = BindingMetadataReader::new([fat.path()])
            .read_required()
            .expect("fat artifact metadata reads");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].package().name().as_path_string(), "demo");
    }

    #[test]
    fn thin_archive_member_is_read_from_archive_directory() {
        let object = MetadataArtifact::compile_object();
        let object_bytes = fs::read(object.path()).expect("read metadata object");
        let archive = ThinArchive::new("metadata.o", object_bytes);

        let envelopes = BindingMetadataReader::new([archive.path()])
            .read_required()
            .expect("thin archive metadata reads");

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].package().name().as_path_string(), "demo");
    }

    struct MetadataArtifact {
        root: PathBuf,
        path: PathBuf,
    }

    impl MetadataArtifact {
        fn compile() -> Self {
            Self::compile_with_records(1)
        }

        fn compile_with_repeated_record() -> Self {
            Self::compile_with_records(2)
        }

        fn compile_without_metadata() -> Self {
            Self::compile_with_records(0)
        }

        fn compile_object() -> Self {
            Self::compile_with_records_and_kind(1, ArtifactKind::Object)
        }

        fn compile_with_records(records: usize) -> Self {
            Self::compile_with_records_and_kind(records, ArtifactKind::StaticLibrary)
        }

        fn compile_with_records_and_kind(records: usize, kind: ArtifactKind) -> Self {
            let root = temp_root("boltffi-bindgen-metadata");
            fs::create_dir_all(&root).expect("create metadata fixture root");
            let source = root.join("lib.rs");
            let artifact = root.join(kind.file_name());
            fs::write(&source, Self::source(records)).expect("write metadata fixture source");

            let output = Command::new(rustc())
                .arg("--crate-name")
                .arg("demo")
                .arg("--edition=2024")
                .args(kind.rustc_args())
                .arg(&source)
                .arg("-o")
                .arg(&artifact)
                .output()
                .expect("run rustc for metadata fixture");
            assert!(
                output.status.success(),
                "metadata fixture failed to compile\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            Self {
                root,
                path: artifact,
            }
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }

        fn source(records: usize) -> String {
            let record = metadata_record();
            let mach_o_section = BindingMetadataSection::MachO.link_section();
            let object_section = BindingMetadataSection::Object.link_section();
            let length = record.len();
            let bytes = record
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            (0..records)
                .map(|index| {
                    format!(
                        "#[cfg_attr(target_vendor = \"apple\", unsafe(link_section = \"{mach_o_section}\"))]\n#[cfg_attr(not(target_vendor = \"apple\"), unsafe(link_section = \"{object_section}\"))]\n#[used]\nstatic BOLTFFI_METADATA_{index}: [u8; {length}] = [{bytes}];"
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    #[derive(Clone, Copy)]
    enum ArtifactKind {
        Object,
        StaticLibrary,
    }

    impl ArtifactKind {
        const fn file_name(self) -> &'static str {
            match self {
                Self::Object => "metadata.o",
                Self::StaticLibrary => "libdemo.a",
            }
        }

        fn rustc_args(self) -> &'static [&'static str] {
            match self {
                Self::Object => &["--crate-type", "lib", "--emit=obj"],
                Self::StaticLibrary => &["--crate-type", "staticlib"],
            }
        }
    }

    impl Drop for MetadataArtifact {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    struct RawArtifact {
        root: PathBuf,
        path: PathBuf,
    }

    impl RawArtifact {
        fn new(name: &str, bytes: Vec<u8>) -> Self {
            let root = temp_root("boltffi-bindgen-raw-artifact");
            fs::create_dir_all(&root).expect("create raw artifact root");
            let path = root.join(name);
            fs::write(&path, bytes).expect("write raw artifact");
            Self { root, path }
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }
    }

    impl Drop for RawArtifact {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    struct ThinArchive {
        root: PathBuf,
        path: PathBuf,
    }

    impl ThinArchive {
        fn new(member_name: &str, member_bytes: Vec<u8>) -> Self {
            let root = temp_root("boltffi-bindgen-thin-archive");
            fs::create_dir_all(&root).expect("create thin archive root");
            let member = root.join(member_name);
            let path = root.join("libthin.a");
            fs::write(member, &member_bytes).expect("write thin archive member");
            fs::write(&path, thin_archive(member_name, member_bytes.len()))
                .expect("write thin archive");
            Self { root, path }
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }
    }

    impl Drop for ThinArchive {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn metadata_record() -> Vec<u8> {
        let source = SourceContract::new(PackageInfo::new("demo", None));
        let lowered = lower_with_declarations::<Native>(&source).expect("empty source lowers");
        BindingMetadataEnvelope::new(SerializedBindings::native(lowered.into_bindings()))
            .expect("metadata envelope")
            .to_section_bytes()
            .expect("metadata section bytes")
    }

    fn malformed_archive_member() -> Vec<u8> {
        let payload = b"\x7fELF\x02BROKEN_OBJECT";
        archive_member("broken.o/", payload)
    }

    fn non_object_binary_archive_member() -> Vec<u8> {
        let payload = b"dyld_v1 NOT_AN_OBJECT";
        archive_member("fat.o/", payload)
    }

    fn archive_member(name: &str, payload: &[u8]) -> Vec<u8> {
        let header = format!(
            "{name:<16}0           0     0     100644  {:<10}`\n",
            payload.len()
        );
        [b"!<arch>\n".as_slice(), header.as_bytes(), payload].concat()
    }

    fn thin_archive(name: &str, member_size: usize) -> Vec<u8> {
        let name = format!("{name}/");
        let header = format!("{name:<16}0           0     0     100644  {member_size:<10}`\n");
        [b"!<thin>\n".as_slice(), header.as_bytes()].concat()
    }

    fn mach_o_fat32(bytes: &[u8]) -> Vec<u8> {
        let offset = 32u32;
        [
            0xcafebabe_u32.to_be_bytes().as_slice(),
            1u32.to_be_bytes().as_slice(),
            0x01000007u32.to_be_bytes().as_slice(),
            3u32.to_be_bytes().as_slice(),
            offset.to_be_bytes().as_slice(),
            (bytes.len() as u32).to_be_bytes().as_slice(),
            0u32.to_be_bytes().as_slice(),
            &[0; 4],
            bytes,
        ]
        .concat()
    }

    fn temp_root(prefix: &str) -> PathBuf {
        static TEMP_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos(),
            TEMP_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn rustc() -> OsString {
        std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"))
    }
}
