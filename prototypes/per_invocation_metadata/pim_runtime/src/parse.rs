use std::fmt;

use crate::MAGIC;

/// One metadata static, as written by a single macro invocation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawRecord {
    pub module: String,
    pub slots: Vec<String>,
    pub json: Vec<u8>,
}

/// Walks concatenated records, tolerating linker padding between them.
pub fn records(section: &[u8]) -> Result<Vec<RawRecord>, ParseError> {
    let mut records = Vec::new();
    let mut at = 0;

    while let Some(start) = find_magic(section, at) {
        let cursor = Cursor::new(section, start + MAGIC.len());
        let (record, end) = read_record(cursor)?;
        records.push(record);
        at = end;
    }

    Ok(records)
}

fn find_magic(section: &[u8], from: usize) -> Option<usize> {
    section
        .get(from..)?
        .windows(MAGIC.len())
        .position(|window| window == MAGIC)
        .map(|offset| from + offset)
}

fn read_record(mut cursor: Cursor<'_>) -> Result<(RawRecord, usize), ParseError> {
    let payload = cursor.u64()? as usize;
    let end = cursor.at + payload;
    if end > cursor.section.len() {
        return Err(ParseError::Truncated);
    }

    let module = cursor.string()?;
    let count = cursor.u16()? as usize;
    let mut slots = Vec::with_capacity(count);
    for _ in 0..count {
        slots.push(cursor.string()?);
    }

    let json = cursor.u32()? as usize;
    let json = cursor.take(json)?.to_vec();

    if cursor.at != end {
        return Err(ParseError::PayloadLength {
            declared: payload,
            actual: payload + cursor.at - end,
        });
    }

    Ok((
        RawRecord {
            module,
            slots,
            json,
        },
        end,
    ))
}

struct Cursor<'a> {
    section: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(section: &'a [u8], at: usize) -> Self {
        Self { section, at }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ParseError> {
        let bytes = self
            .section
            .get(self.at..self.at + length)
            .ok_or(ParseError::Truncated)?;
        self.at += length;
        Ok(bytes)
    }

    fn u16(&mut self) -> Result<u16, ParseError> {
        let bytes = self.take(size_of::<u16>())?;
        Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        let bytes = self.take(size_of::<u32>())?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
    }

    fn u64(&mut self) -> Result<u64, ParseError> {
        let bytes = self.take(size_of::<u64>())?;
        Ok(u64::from_le_bytes(bytes.try_into().expect("eight bytes")))
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let length = self.u16()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ParseError::NotUtf8)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
    NotUtf8,
    PayloadLength { declared: usize, actual: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(formatter, "record ends past the section"),
            Self::NotUtf8 => write!(formatter, "record holds a non-utf-8 string"),
            Self::PayloadLength { declared, actual } => write!(
                formatter,
                "record declares a {declared} byte payload but holds {actual}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{id_len, join_id, record, record_len, str_from};

    const MODULE: &str = "toy::geometry";
    const SLOTS: &[&str] = &["toy::geometry", "Point"];
    const JSON: &[u8] = br#"{"name":"Shape"}"#;
    const LEN: usize = record_len(MODULE, SLOTS, JSON);
    static ONE: [u8; LEN] = record::<LEN>(MODULE, SLOTS, JSON);

    const EMPTY_LEN: usize = record_len("toy", &[], b"{}");
    static EMPTY: [u8; EMPTY_LEN] = record::<EMPTY_LEN>("toy", &[], b"{}");

    #[test]
    fn round_trips_a_single_record() {
        let records = records(&ONE).expect("section parses");
        assert_eq!(
            records,
            vec![RawRecord {
                module: MODULE.to_owned(),
                slots: vec!["toy::geometry".to_owned(), "Point".to_owned()],
                json: JSON.to_vec(),
            }],
            "record survives const framing"
        );
    }

    #[test]
    fn round_trips_concatenated_records() {
        let mut section = ONE.to_vec();
        section.extend_from_slice(&EMPTY);

        let records = records(&section).expect("section parses");
        assert_eq!(records.len(), 2, "both records are recovered");
        assert_eq!(records[1].module, "toy", "second record follows the first");
        assert!(records[1].slots.is_empty(), "zero-slot records are legal");
    }

    #[test]
    fn skips_padding_between_records() {
        let mut section = ONE.to_vec();
        section.extend_from_slice(&[0; 7]);
        section.extend_from_slice(&EMPTY);

        let records = records(&section).expect("section parses");
        assert_eq!(records.len(), 2, "padding does not hide the second record");
    }

    #[test]
    fn rejects_a_truncated_record() {
        let truncated = &ONE[..ONE.len() - 1];

        assert_eq!(
            records(truncated),
            Err(ParseError::Truncated),
            "a clipped payload is an error, not a silent drop"
        );
    }

    /// The buffer must be a `const`, not a `static`: reading a `static` back out in const context
    /// ICEs clippy 1.95 (`mir/consts.rs:176: expected memory, got Static`).
    #[test]
    fn joins_a_canonical_id_in_const() {
        const ID_LEN: usize = id_len("toy::geometry", "Point");
        const ID: [u8; ID_LEN] = join_id::<ID_LEN>("toy::geometry", "Point");
        const ID_STR: &str = str_from(&ID);

        assert_eq!(ID_STR, "toy::geometry::Point", "const concat yields the id");
    }
}
