/// Frame marker for one per-invocation source record.
pub const SOURCE_RECORD_MAGIC: [u8; 8] = *b"BFFISRC1";

/// Mach-O `link_section` value for per-invocation source records.
pub const SOURCE_SECTION_MACH_O: &str = "__DATA,__boltffisrc";

/// Mach-O section name without the segment, as the reader sees it.
pub const SOURCE_SECTION_MACH_O_NAME: &str = "__boltffisrc";

/// ELF/COFF/wasm `link_section` value for per-invocation source records.
pub const SOURCE_SECTION_OBJECT: &str = ".boltffisrc";

/// Byte length of the record produced by [`record`] for the same inputs.
pub const fn record_len(
    package: &str,
    version: &str,
    module: &str,
    slots: &[&str],
    json: &[u8],
) -> usize {
    let mut length = SOURCE_RECORD_MAGIC.len() + size_of::<u64>();
    length += size_of::<u16>() + package.len();
    length += size_of::<u16>() + version.len();
    length += size_of::<u16>() + module.len();
    length += size_of::<u16>();

    let mut index = 0;
    while index < slots.len() {
        length += size_of::<u16>() + slots[index].len();
        index += 1;
    }

    length + size_of::<u32>() + json.len()
}

/// Builds one framed source record in const context.
///
/// `N` must equal [`record_len`] for the same inputs; the build fails otherwise.
pub const fn record<const N: usize>(
    package: &str,
    version: &str,
    module: &str,
    slots: &[&str],
    json: &[u8],
) -> [u8; N] {
    let mut bytes = [0u8; N];
    let mut at = write(&mut bytes, 0, &SOURCE_RECORD_MAGIC);

    let payload = (N - SOURCE_RECORD_MAGIC.len() - size_of::<u64>()) as u64;
    at = write(&mut bytes, at, &payload.to_le_bytes());
    at = write_str(&mut bytes, at, package);
    at = write_str(&mut bytes, at, version);
    at = write_str(&mut bytes, at, module);

    assert!(
        slots.len() <= u16::MAX as usize,
        "record has too many slots"
    );
    at = write(&mut bytes, at, &(slots.len() as u16).to_le_bytes());

    let mut index = 0;
    while index < slots.len() {
        at = write_str(&mut bytes, at, slots[index]);
        index += 1;
    }

    assert!(
        json.len() <= u32::MAX as usize,
        "record payload is too large"
    );
    at = write(&mut bytes, at, &(json.len() as u32).to_le_bytes());
    at = write(&mut bytes, at, json);

    assert!(at == N, "record length disagrees with record_len");
    bytes
}

const fn write(bytes: &mut [u8], at: usize, source: &[u8]) -> usize {
    let mut index = 0;
    while index < source.len() {
        bytes[at + index] = source[index];
        index += 1;
    }
    at + source.len()
}

const fn write_str(bytes: &mut [u8], at: usize, value: &str) -> usize {
    assert!(
        value.len() <= u16::MAX as usize,
        "record string is too long"
    );
    let at = write(bytes, at, &(value.len() as u16).to_le_bytes());
    write(bytes, at, value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str = "demo-pkg";
    const VERSION: &str = "1.2.3";
    const MODULE: &str = "demo_pkg::geometry";
    const SLOTS: &[&str] = &["{\"id\":\"demo_pkg::Point\"}"];
    const JSON: &[u8] = br#"{"kind":"record"}"#;
    const LEN: usize = record_len(PACKAGE, VERSION, MODULE, SLOTS, JSON);
    static RECORD: [u8; LEN] = record(PACKAGE, VERSION, MODULE, SLOTS, JSON);

    fn read_u16(bytes: &[u8], at: usize) -> (usize, usize) {
        let value = u16::from_le_bytes(bytes[at..at + 2].try_into().expect("u16 width"));
        (value as usize, at + 2)
    }

    #[test]
    fn frames_a_record_built_in_const_context() {
        assert_eq!(&RECORD[..8], b"BFFISRC1", "record starts with the magic");
        let length = u64::from_le_bytes(RECORD[8..16].try_into().expect("u64 width"));
        assert_eq!(
            length as usize,
            LEN - 16,
            "payload length covers everything after the header"
        );

        let (package_len, at) = read_u16(&RECORD, 16);
        assert_eq!(
            &RECORD[at..at + package_len],
            PACKAGE.as_bytes(),
            "package name follows the header"
        );

        let (version_len, at) = read_u16(&RECORD, at + package_len);
        assert_eq!(
            &RECORD[at..at + version_len],
            VERSION.as_bytes(),
            "package version follows the name"
        );

        let (module_len, at) = read_u16(&RECORD, at + version_len);
        assert_eq!(
            &RECORD[at..at + module_len],
            MODULE.as_bytes(),
            "module path follows the version"
        );

        let (slot_count, mut at) = read_u16(&RECORD, at + module_len);
        assert_eq!(slot_count, SLOTS.len(), "slot count matches");
        for slot in SLOTS {
            let (slot_len, start) = read_u16(&RECORD, at);
            assert_eq!(
                &RECORD[start..start + slot_len],
                slot.as_bytes(),
                "slot descriptor round-trips"
            );
            at = start + slot_len;
        }

        let json_len = u32::from_le_bytes(RECORD[at..at + 4].try_into().expect("u32 width"));
        let at = at + 4;
        assert_eq!(
            &RECORD[at..at + json_len as usize],
            JSON,
            "json payload closes the record"
        );
        assert_eq!(at + json_len as usize, LEN, "nothing trails the payload");
    }

    #[test]
    fn frames_an_empty_version_and_no_slots() {
        const LEN: usize = record_len("p", "", "p", &[], b"{}");
        static RECORD: [u8; LEN] = record("p", "", "p", &[], b"{}");
        let (version_len, _) = (
            u16::from_le_bytes(RECORD[19..21].try_into().expect("u16 width")),
            0,
        );
        assert_eq!(version_len, 0, "missing version is an empty string");
        assert_eq!(RECORD[LEN - 2..], *b"{}", "payload still lands at the end");
    }
}
