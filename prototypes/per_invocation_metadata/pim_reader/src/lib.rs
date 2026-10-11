//! Extracts per-invocation metadata records from a compiled artifact and resolves their slots.

mod resolve;

pub use resolve::{
    Callback, CallbackMethod, Class, Field, Function, Item, Method, ResolveError, Resolved,
    Scaffolding, Stream, resolve,
};

use std::fmt;
use std::io;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{FileKind, Object, ObjectSection};
use pim_runtime::{MACH_O_SECTION_NAME, OBJECT_SECTION_NAME, ParseError, RawRecord, records};

/// Reads every metadata record out of a cdylib, an executable, or an rlib.
pub fn read_artifact(path: &Path) -> Result<Vec<RawRecord>, ReadError> {
    let data = std::fs::read(path)?;

    let mut found = match FileKind::parse(&*data)? {
        FileKind::Archive => read_archive(&data)?,
        _ => read_object(&data)?,
    };

    found.sort();
    found.dedup();
    Ok(found)
}

fn read_archive(data: &[u8]) -> Result<Vec<RawRecord>, ReadError> {
    let archive = ArchiveFile::parse(data)?;
    let mut found = Vec::new();

    for member in archive.members() {
        let member = member?;
        let bytes = member.data(data)?;
        if FileKind::parse(bytes).is_err() {
            continue;
        }
        found.extend(read_object(bytes)?);
    }

    Ok(found)
}

fn read_object(data: &[u8]) -> Result<Vec<RawRecord>, ReadError> {
    let object = object::File::parse(data)?;
    let mut found = Vec::new();

    for section in object.sections() {
        let name = section.name()?;
        if name != MACH_O_SECTION_NAME && name != OBJECT_SECTION_NAME {
            continue;
        }
        found.extend(records(&section.uncompressed_data()?)?);
    }

    Ok(found)
}

#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    Object(object::Error),
    Record(ParseError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cannot read the artifact: {error}"),
            Self::Object(error) => write!(formatter, "cannot parse the artifact: {error}"),
            Self::Record(error) => write!(formatter, "cannot parse a metadata record: {error}"),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<object::Error> for ReadError {
    fn from(error: object::Error) -> Self {
        Self::Object(error)
    }
}

impl From<ParseError> for ReadError {
    fn from(error: ParseError) -> Self {
        Self::Record(error)
    }
}
