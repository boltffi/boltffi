//! Per-invocation source records read back from a compiled artifact.
//!
//! Each `#[data]`/`#[export]` invocation emits one framed record into a dedicated link
//! section. The frame constants here mirror `boltffi_core::capture` byte for byte; the
//! two crates cannot share them without a dependency cycle, so `boltffi_tests` pins the
//! round-trip between the const writer and this reader.

use boltffi_ast::PackageInfo;
use serde::{Deserialize, Serialize};

/// Frame marker for one per-invocation source record.
pub const SOURCE_RECORD_MAGIC: &[u8; 8] = b"BFFISRC1";

/// Mach-O section name carrying source records, without the segment.
pub const SOURCE_SECTION_MACH_O_NAME: &str = "__boltffisrc";

/// ELF/COFF/wasm section name carrying source records.
pub const SOURCE_SECTION_OBJECT_NAME: &str = ".boltffisrc";

/// Returns whether a section name carries per-invocation source records.
pub fn is_source_record_section(name: &str) -> bool {
    name == SOURCE_SECTION_MACH_O_NAME || name == SOURCE_SECTION_OBJECT_NAME
}

/// One per-invocation source record, frame-decoded but not yet interpreted.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RawSourceRecord {
    /// Cargo package that compiled the emitting invocation.
    pub package: PackageInfo,
    /// `module_path!()` at the emitting invocation.
    pub module: String,
    /// Slot descriptors, resolved by rustc at each referenced type's definition site.
    pub slots: Vec<String>,
    /// JSON payload describing the invocation's declaration.
    pub json: Vec<u8>,
}

/// Bytes read from a compiled artifact's source-record section.
///
/// A section holds one record per macro invocation; records are length-prefixed so
/// concatenated statics remain parseable after the linker merges them.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceRecordSectionBytes<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> SourceRecordSectionBytes<'bytes> {
    /// Stores the raw section bytes.
    pub const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes }
    }

    /// Decodes every source record in section order.
    pub fn records(self) -> Result<Vec<RawSourceRecord>, SourceRecordError> {
        let mut offset = 0;
        std::iter::from_fn(|| {
            (offset < self.bytes.len()).then(|| {
                let record = self.record_at(offset);
                offset = record.as_ref().map_or(self.bytes.len(), |(_, next)| *next);
                record.map(|(record, _)| record)
            })
        })
        .collect()
    }

    fn record_at(self, offset: usize) -> Result<(RawSourceRecord, usize), SourceRecordError> {
        let mut cursor = Cursor {
            bytes: self.bytes,
            at: offset,
            record: offset,
        };
        let magic = cursor.take(SOURCE_RECORD_MAGIC.len())?;
        if magic != SOURCE_RECORD_MAGIC {
            return Err(SourceRecordError::InvalidMagic { offset });
        }
        let payload_length = cursor.length_u64()?;
        let payload_end = cursor
            .at
            .checked_add(payload_length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(SourceRecordError::Truncated { offset })?;

        let name = cursor.string_u16()?;
        let version = cursor.string_u16()?;
        let module = cursor.string_u16()?;
        let slot_count = cursor.value_u16()?;
        let slots = (0..slot_count)
            .map(|_| cursor.string_u16())
            .collect::<Result<Vec<_>, _>>()?;
        let json_length = cursor.value_u32()?;
        let json = cursor.take(json_length)?.to_vec();

        if cursor.at != payload_end {
            return Err(SourceRecordError::PayloadLengthMismatch { offset });
        }

        let record = RawSourceRecord {
            package: PackageInfo::new(name, Some(version).filter(|value| !value.is_empty())),
            module,
            slots,
            json,
        };
        Ok((record, payload_end))
    }
}

struct Cursor<'bytes> {
    bytes: &'bytes [u8],
    at: usize,
    record: usize,
}

impl<'bytes> Cursor<'bytes> {
    fn take(&mut self, length: usize) -> Result<&'bytes [u8], SourceRecordError> {
        let end = self
            .at
            .checked_add(length)
            .ok_or(SourceRecordError::Truncated {
                offset: self.record,
            })?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or(SourceRecordError::Truncated {
                offset: self.record,
            })?;
        self.at = end;
        Ok(bytes)
    }

    fn value_u16(&mut self) -> Result<usize, SourceRecordError> {
        let bytes = self.take(size_of::<u16>())?;
        Ok(u16::from_le_bytes(bytes.try_into().expect("u16 width")) as usize)
    }

    fn value_u32(&mut self) -> Result<usize, SourceRecordError> {
        let bytes = self.take(size_of::<u32>())?;
        let value = u32::from_le_bytes(bytes.try_into().expect("u32 width"));
        usize::try_from(value).map_err(|_| SourceRecordError::Truncated {
            offset: self.record,
        })
    }

    fn length_u64(&mut self) -> Result<usize, SourceRecordError> {
        let bytes = self.take(size_of::<u64>())?;
        let value = u64::from_le_bytes(bytes.try_into().expect("u64 width"));
        usize::try_from(value).map_err(|_| SourceRecordError::TooLarge {
            offset: self.record,
            length: value,
        })
    }

    fn string_u16(&mut self) -> Result<String, SourceRecordError> {
        let length = self.value_u16()?;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| SourceRecordError::InvalidUtf8 {
            offset: self.record,
        })
    }
}

/// Source-record frame decoding failure.
#[derive(Debug)]
pub enum SourceRecordError {
    /// A record does not start with the source-record marker.
    InvalidMagic {
        /// Byte offset of the invalid record.
        offset: usize,
    },
    /// A record ended before its header or payload was complete.
    Truncated {
        /// Byte offset of the truncated record.
        offset: usize,
    },
    /// A record length cannot be represented on this platform.
    TooLarge {
        /// Byte offset of the record.
        offset: usize,
        /// Payload length written in the record header.
        length: u64,
    },
    /// The declared payload length disagrees with the decoded fields.
    PayloadLengthMismatch {
        /// Byte offset of the record.
        offset: usize,
    },
    /// A record string field is not UTF-8.
    InvalidUtf8 {
        /// Byte offset of the record.
        offset: usize,
    },
}

impl std::fmt::Display for SourceRecordError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic { offset } => {
                write!(formatter, "source record at {offset} has an invalid marker")
            }
            Self::Truncated { offset } => {
                write!(formatter, "source record at {offset} is truncated")
            }
            Self::TooLarge { offset, length } => write!(
                formatter,
                "source record at {offset} declares an unrepresentable length {length}"
            ),
            Self::PayloadLengthMismatch { offset } => write!(
                formatter,
                "source record at {offset} declares a payload length its fields disagree with"
            ),
            Self::InvalidUtf8 { offset } => write!(
                formatter,
                "source record at {offset} holds a non-UTF-8 string field"
            ),
        }
    }
}

impl std::error::Error for SourceRecordError {}

/// The source records of a workspace's last records build, kept in the workspace.
///
/// `boltffi generate` writes the store after its records build. Later builds read it so
/// an invocation learns which declaration every type it names resolves to, as rustc
/// resolved it, whatever spelling the signature uses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordStore {
    version: u32,
    records: Vec<StoredRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StoredRecord {
    package: String,
    package_version: Option<String>,
    module: String,
    slots: Vec<String>,
    json: String,
}

impl RecordStore {
    /// Where the store lives, relative to the workspace root.
    pub const RELATIVE_PATH: &'static str = ".boltffi/records.json";

    const VERSION: u32 = 1;

    /// Keeps the records of one records build.
    pub fn new(records: &[RawSourceRecord]) -> Self {
        Self {
            version: Self::VERSION,
            records: records
                .iter()
                .map(|record| StoredRecord {
                    package: record.package.name.clone(),
                    package_version: record.package.version.clone(),
                    module: record.module.clone(),
                    slots: record.slots.clone(),
                    json: String::from_utf8_lossy(&record.json).into_owned(),
                })
                .collect(),
        }
    }

    /// Reads a store, or returns `None` when the text is not a store of this version.
    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str::<Self>(text)
            .ok()
            .filter(|store| store.version == Self::VERSION)
    }

    /// Returns the store as JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Returns the stored records.
    pub fn records(&self) -> Vec<RawSourceRecord> {
        self.records
            .iter()
            .map(|record| RawSourceRecord {
                package: PackageInfo::new(record.package.clone(), record.package_version.clone()),
                module: record.module.clone(),
                slots: record.slots.clone(),
                json: record.json.clone().into_bytes(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_keeps_its_records_exactly() {
        let records = vec![RawSourceRecord {
            package: PackageInfo::new("demo-app", Some("1.2.3".to_owned())),
            module: "demo_app::geometry".to_owned(),
            slots: vec![r#"{"id":"demo_app::geometry::Point","fingerprint":"0a1b"}"#.to_owned()],
            json: br#"{"function":{"id":"$self::norm"}}"#.to_vec(),
        }];

        let store = RecordStore::new(&records);
        let read = RecordStore::from_json(&store.to_json().expect("store serializes"))
            .expect("a store of this version reads back");

        assert_eq!(read.records(), records);
    }

    #[test]
    fn a_store_of_another_version_is_not_read() {
        let text = r#"{"version":0,"records":[]}"#;

        assert!(RecordStore::from_json(text).is_none());
    }

    fn frame(package: &str, version: &str, module: &str, slots: &[&str], json: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        for field in [package, version, module] {
            payload.extend_from_slice(&(field.len() as u16).to_le_bytes());
            payload.extend_from_slice(field.as_bytes());
        }
        payload.extend_from_slice(&(slots.len() as u16).to_le_bytes());
        for slot in slots {
            payload.extend_from_slice(&(slot.len() as u16).to_le_bytes());
            payload.extend_from_slice(slot.as_bytes());
        }
        payload.extend_from_slice(&(json.len() as u32).to_le_bytes());
        payload.extend_from_slice(json);

        let mut bytes = SOURCE_RECORD_MAGIC.to_vec();
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&payload);
        bytes
    }

    #[test]
    fn decodes_concatenated_records_in_section_order() {
        let mut bytes = frame(
            "demo",
            "1.0.0",
            "demo::geometry",
            &[r#"{"id":"demo::Point"}"#],
            br#"{"kind":"record"}"#,
        );
        bytes.extend_from_slice(&frame("demo", "", "demo", &[], br#"{"kind":"function"}"#));

        let records = SourceRecordSectionBytes::new(&bytes)
            .records()
            .expect("both records decode");
        assert_eq!(records.len(), 2, "section holds one record per invocation");
        assert_eq!(
            records[0].package,
            PackageInfo::new("demo", Some("1.0.0".into()))
        );
        assert_eq!(records[0].module, "demo::geometry");
        assert_eq!(records[0].slots, vec![r#"{"id":"demo::Point"}"#.to_owned()]);
        assert_eq!(records[0].json, br#"{"kind":"record"}"#);
        assert_eq!(
            records[1].package,
            PackageInfo::new("demo", None),
            "an empty version decodes as no version"
        );
        assert!(records[1].slots.is_empty(), "records may carry no slots");
    }

    #[test]
    fn rejects_a_wrong_marker() {
        let mut bytes = frame("demo", "", "demo", &[], b"{}");
        bytes[0] = b'X';
        let error = SourceRecordSectionBytes::new(&bytes)
            .records()
            .expect_err("marker mismatch fails");
        assert!(
            matches!(error, SourceRecordError::InvalidMagic { offset: 0 }),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn rejects_a_truncated_record() {
        let bytes = frame("demo", "", "demo", &[], b"{}");
        let error = SourceRecordSectionBytes::new(&bytes[..bytes.len() - 1])
            .records()
            .expect_err("short section fails");
        assert!(
            matches!(
                error,
                SourceRecordError::Truncated { offset: 0 }
                    | SourceRecordError::PayloadLengthMismatch { offset: 0 }
            ),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn rejects_a_payload_length_that_disagrees_with_fields() {
        let mut bytes = frame("demo", "", "demo", &[], b"{}");
        let length = u64::from_le_bytes(bytes[8..16].try_into().expect("u64 width"));
        bytes[8..16].copy_from_slice(&(length + 4).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let error = SourceRecordSectionBytes::new(&bytes)
            .records()
            .expect_err("padding after fields fails");
        assert!(
            matches!(
                error,
                SourceRecordError::PayloadLengthMismatch { offset: 0 }
            ),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn matches_only_the_source_sections() {
        assert!(is_source_record_section("__boltffisrc"));
        assert!(is_source_record_section(".boltffisrc"));
        assert!(!is_source_record_section("__boltffi"));
        assert!(!is_source_record_section(".boltffi"));
    }
}
