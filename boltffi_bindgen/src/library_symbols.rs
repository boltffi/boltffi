//! The symbols a built library exports, to check generated bindings against.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use object::read::archive::ArchiveFile;
use object::{BinaryFormat, File, FileKind, NameOrOrdinal, Object, ObjectSymbol};
use thiserror::Error;

/// Symbols the bindings call that the built library does not export.
#[derive(Debug, Error)]
pub enum LibrarySymbolsError {
    /// The library could not be read.
    #[error("read built library `{path}`: {source}")]
    Read {
        /// Library path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// The library is not an object format the check understands.
    #[error("parse built library `{path}`: {source}")]
    Parse {
        /// Library path.
        path: PathBuf,
        /// Object parse error.
        source: object::Error,
    },
    /// Bindings name symbols the library lacks, so they would fail when loaded.
    #[error(
        "bindings call symbols that `{path}` does not export: {}; \
         the crates declaring them are not linked into the library; \
         name each such crate from the library crate, for example `use <crate> as _;`",
        missing.join(", ")
    )]
    Missing {
        /// Library path.
        path: PathBuf,
        /// Missing symbol names, sorted.
        missing: Vec<String>,
    },
}

/// Checks that `library` exports every name in `symbols`.
pub fn require_exported<'name>(
    library: &Path,
    symbols: impl IntoIterator<Item = &'name str>,
) -> Result<(), LibrarySymbolsError> {
    let exported = exported(library)?;
    let mut missing = symbols
        .into_iter()
        .filter(|symbol| !exported.contains(*symbol))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort();
    missing.dedup();
    Err(LibrarySymbolsError::Missing {
        path: library.to_path_buf(),
        missing,
    })
}

fn exported(library: &Path) -> Result<HashSet<String>, LibrarySymbolsError> {
    let bytes = std::fs::read(library).map_err(|source| LibrarySymbolsError::Read {
        path: library.to_path_buf(),
        source,
    })?;
    let parse = |source| LibrarySymbolsError::Parse {
        path: library.to_path_buf(),
        source,
    };
    match FileKind::parse(bytes.as_slice()).map_err(parse)? {
        FileKind::Archive => {
            let archive = ArchiveFile::parse(bytes.as_slice()).map_err(parse)?;
            let mut symbols = HashSet::new();
            for member in archive.members() {
                let member = member.map_err(parse)?;
                let data = member.data(bytes.as_slice()).map_err(parse)?;
                if let Ok(file) = File::parse(data) {
                    symbols.extend(defined_globals(&file));
                }
            }
            Ok(symbols)
        }
        _ => {
            let file = File::parse(bytes.as_slice()).map_err(parse)?;
            if file.format() == BinaryFormat::Wasm {
                return Ok(defined_globals(&file).into_iter().collect());
            }
            let exports = file.exports().map_err(parse)?;
            exports
                .map(|export| {
                    export.map_err(parse).map(|export| match export.name() {
                        NameOrOrdinal::Name(name) => std::str::from_utf8(name)
                            .ok()
                            .map(|name| unmangled(file.format(), name)),
                        NameOrOrdinal::Ordinal(_) => None,
                    })
                })
                .filter_map(Result::transpose)
                .collect()
        }
    }
}

fn defined_globals(file: &File<'_>) -> Vec<String> {
    file.symbols()
        .filter(|symbol| symbol.is_global() && !symbol.is_undefined())
        .filter_map(|symbol| {
            symbol
                .name()
                .ok()
                .map(|name| unmangled(file.format(), name))
        })
        .collect()
}

fn unmangled(format: BinaryFormat, name: &str) -> String {
    match format {
        BinaryFormat::MachO => name.strip_prefix('_').unwrap_or(name).to_owned(),
        _ => name.to_owned(),
    }
}
