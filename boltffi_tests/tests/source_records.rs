//! Pins the per-invocation record contract between the const writer in `boltffi_core`
//! and the reader in `boltffi_binding`, which mirror each other's framing by hand.

use boltffi_binding::SourceRecordSectionBytes;
use boltffi_core::capture::{self, DescBuf, TypeDesc, TypeInfo};

struct ProbeTag;

struct Point;

impl<Tag> TypeInfo<Tag> for Point {
    const MODULE: &'static str = "demo::geometry";
    const NAME: &'static str = "Point";
}

impl<Tag> TypeDesc<Tag> for Point {
    const DESC: DescBuf = DescBuf::named(
        <Point as TypeInfo<Tag>>::MODULE,
        <Point as TypeInfo<Tag>>::NAME,
    );
}

type Points = Vec<Point>;

const PACKAGE: &str = "demo";
const VERSION: &str = "1.2.3";
const MODULE: &str = "demo::geometry";
const POINTS_DESC: &DescBuf = &<Points as TypeDesc<ProbeTag>>::DESC;
const SLOTS: &[&str] = &[POINTS_DESC.as_str()];
const JSON: &[u8] = br#"{"kind":"record","id":"$self::Route","name":{"spelling":"Route","canonical":{"parts":["route"]}}}"#;

const RECORD_LEN: usize = capture::record_len(PACKAGE, VERSION, MODULE, SLOTS, JSON);
static RECORD: [u8; RECORD_LEN] = capture::record(PACKAGE, VERSION, MODULE, SLOTS, JSON);

#[test]
fn const_written_records_decode_through_the_binding_reader() {
    let mut section = RECORD.to_vec();
    section.extend_from_slice(&RECORD);

    let records = SourceRecordSectionBytes::new(&section)
        .records()
        .expect("const-written records decode");

    assert_eq!(records.len(), 2, "concatenated statics stay parseable");
    assert_eq!(records[0].package.name, PACKAGE);
    assert_eq!(records[0].package.version.as_deref(), Some(VERSION));
    assert_eq!(records[0].module, MODULE);
    assert_eq!(
        records[0].slots,
        vec![r#"{"shape":"Vec","args":[{"id":"demo::geometry::Point"}]}"#.to_owned()],
        "the descriptor composed in const context matches the reader's grammar"
    );
    assert_eq!(records[0].json, JSON);
}

#[test]
fn section_names_agree_between_writer_and_reader() {
    assert!(boltffi_binding::is_source_record_section(
        capture::SOURCE_SECTION_MACH_O_NAME
    ));
    assert!(boltffi_binding::is_source_record_section(
        capture::SOURCE_SECTION_OBJECT
    ));
    assert_eq!(
        capture::SOURCE_RECORD_MAGIC.as_slice(),
        boltffi_binding::SOURCE_RECORD_MAGIC.as_slice(),
        "the frame marker is byte-identical on both sides"
    );
    assert_eq!(
        capture::SOURCE_SECTION_MACH_O,
        format!("__DATA,{}", boltffi_binding::SOURCE_SECTION_MACH_O_NAME),
        "the Mach-O link_section value carries the reader's section name"
    );
}
