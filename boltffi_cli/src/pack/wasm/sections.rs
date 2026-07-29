//! Removal of custom sections from a wasm module.
//!
//! `wasm-opt` keeps DWARF unless it is asked to drop it, and it is an optional
//! external tool anyway, so the packer does this itself. Only custom sections
//! are touched: they carry no semantics, so dropping them cannot change how the
//! module behaves.

use std::path::Path;

use crate::cli::{CliError, Result};

/// Prefix shared by the DWARF sections a debug-carrying profile emits
/// (`.debug_info`, `.debug_str`, ...). Matching on the prefix rather than a
/// fixed list means a section added by a future toolchain is still dropped.
const DEBUG_SECTION_PREFIX: &str = ".debug_";

/// Descriptor table the `wasm-bindgen` proc macro leaves behind for its own
/// CLI to consume. Nothing in the BoltFFI pipeline reads it, and the CLI never
/// runs, so it would otherwise ship as dead weight.
const WASM_BINDGEN_DESCRIPTORS: &str = "__wasm_bindgen_unstable";

/// Strips the sections and returns how many bytes went.
pub(crate) fn strip_wasm_sections(path: &Path, strip_debug: bool) -> Result<usize> {
    let bytes = std::fs::read(path).map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    let (stripped, removed) =
        strip_bytes(&bytes, strip_debug).ok_or_else(|| CliError::CommandFailed {
            command: format!("{} is not a valid wasm module", path.display()),
            status: None,
        })?;

    if removed == 0 {
        return Ok(0);
    }

    std::fs::write(path, stripped).map_err(|source| CliError::WriteFailed {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(removed)
}

/// Returns the module without the removed sections, or `None` if the input is
/// not a wasm module the walker understands.
fn strip_bytes(bytes: &[u8], strip_debug: bool) -> Option<(Vec<u8>, usize)> {
    const HEADER: usize = 8;
    if bytes.len() < HEADER || &bytes[0..4] != b"\0asm" {
        return None;
    }

    let mut kept = Vec::with_capacity(bytes.len());
    kept.extend_from_slice(&bytes[..HEADER]);
    let mut removed = 0;
    let mut offset = HEADER;

    while offset < bytes.len() {
        let start = offset;
        let id = *bytes.get(offset)?;
        offset += 1;
        let (size, after_size) = read_var_u32(bytes, offset)?;
        let body = after_size;
        let end = body.checked_add(size as usize)?;
        if end > bytes.len() {
            return None;
        }

        let drop_section = (id == 0)
            .then(|| custom_section_name(bytes, body, end))
            .flatten()
            .is_some_and(|name| {
                name == WASM_BINDGEN_DESCRIPTORS
                    || (strip_debug && name.starts_with(DEBUG_SECTION_PREFIX))
            });

        if drop_section {
            removed += end - start;
        } else {
            kept.extend_from_slice(&bytes[start..end]);
        }
        offset = end;
    }

    Some((kept, removed))
}

fn custom_section_name(bytes: &[u8], body: usize, end: usize) -> Option<&str> {
    let (name_len, name_start) = read_var_u32(bytes, body)?;
    let name_end = name_start.checked_add(name_len as usize)?;
    if name_end > end {
        return None;
    }
    std::str::from_utf8(&bytes[name_start..name_end]).ok()
}

fn read_var_u32(bytes: &[u8], mut offset: usize) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(offset)?;
        offset += 1;
        result |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((result, offset));
        }
        shift += 7;
        if shift > 31 {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a module with the given custom sections plus one non-custom
    /// section that must survive untouched.
    fn module(custom: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = b"\0asm\x01\x00\x00\x00".to_vec();
        // A type section (id 1) with an empty body, standing in for real content.
        bytes.push(1);
        bytes.push(1);
        bytes.push(0);
        for (name, payload) in custom {
            let mut body = Vec::new();
            body.push(name.len() as u8);
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(payload);
            bytes.push(0);
            bytes.push(body.len() as u8);
            bytes.extend_from_slice(&body);
        }
        bytes
    }

    fn section_names(bytes: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        let mut offset = 8;
        while offset < bytes.len() {
            let id = bytes[offset];
            offset += 1;
            let (size, body) = read_var_u32(bytes, offset).expect("size");
            let end = body + size as usize;
            names.push(match id {
                0 => format!("custom:{}", custom_section_name(bytes, body, end).unwrap()),
                other => format!("id{other}"),
            });
            offset = end;
        }
        names
    }

    #[test]
    fn drops_debug_sections_and_descriptors() {
        let bytes = module(&[
            (".debug_info", &[1, 2, 3]),
            ("name", &[4, 5]),
            (WASM_BINDGEN_DESCRIPTORS, &[6, 7, 8, 9]),
        ]);
        let (stripped, removed) = strip_bytes(&bytes, true).expect("valid module");

        assert_eq!(section_names(&stripped), vec!["id1", "custom:name"]);
        assert!(removed > 0);
    }

    #[test]
    fn keeps_debug_sections_when_not_stripping() {
        let bytes = module(&[
            (".debug_info", &[1, 2, 3]),
            (WASM_BINDGEN_DESCRIPTORS, &[6, 7]),
        ]);
        let (stripped, removed) = strip_bytes(&bytes, false).expect("valid module");

        // Descriptors go regardless: the wasm-bindgen CLI never runs here.
        assert_eq!(section_names(&stripped), vec!["id1", "custom:.debug_info"]);
        assert!(removed > 0);
    }

    #[test]
    fn drops_debug_sections_the_code_does_not_enumerate() {
        let bytes = module(&[(".debug_something_new", &[1, 2, 3]), ("name", &[4])]);
        let (stripped, removed) = strip_bytes(&bytes, true).expect("valid module");

        assert_eq!(section_names(&stripped), vec!["id1", "custom:name"]);
        assert!(removed > 0);
    }

    #[test]
    fn keeps_the_name_section() {
        let bytes = module(&[("name", &[1, 2, 3])]);
        let (stripped, removed) = strip_bytes(&bytes, true).expect("valid module");

        assert_eq!(section_names(&stripped), vec!["id1", "custom:name"]);
        assert_eq!(removed, 0);
    }

    #[test]
    fn leaves_a_module_without_matching_sections_byte_identical() {
        let bytes = module(&[("name", &[1, 2, 3])]);
        let (stripped, _) = strip_bytes(&bytes, true).expect("valid module");
        assert_eq!(stripped, bytes);
    }

    #[test]
    fn rejects_input_that_is_not_wasm() {
        assert!(strip_bytes(b"not wasm at all", true).is_none());
    }

    #[test]
    fn rejects_a_section_running_past_the_end() {
        let mut bytes = b"\0asm\x01\x00\x00\x00".to_vec();
        bytes.push(0);
        bytes.push(64); // claims 64 bytes, none follow
        assert!(strip_bytes(&bytes, true).is_none());
    }
}
