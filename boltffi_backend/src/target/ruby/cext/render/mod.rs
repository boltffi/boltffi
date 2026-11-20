//! Ruby target rendering: C for the extension, Ruby for the gem around it.

use crate::core::{FilePath, GeneratedFile, Result};

pub mod extension;
pub mod package;

/// Creates a generated file that ends with a newline. Askama drops the
/// final newline of each template.
fn file(path: String, mut source: String) -> Result<GeneratedFile> {
    if !source.ends_with('\n') {
        source.push('\n');
    }
    Ok(GeneratedFile::new(FilePath::new(path)?, source))
}
