//! Identifiers for C-side FFI symbols. The C# identifiers (class /
//! method / property / param / local / namespace names) live in
//! [`super::super::ast::identifier`]: they speak the C# grammar and
//! belong to the AST layer. This file holds the one identifier that
//! names a *C* symbol crossing the ABI boundary.
//!
//! Splitting `C*` from `CSharp*` at the type-name level (and now at
//! the module level) tells the reader at a glance which side of the
//! boundary each value sits on.

use std::fmt;

use boltffi_ffi_rules::naming::{GlobalSymbol, Name};

/// The name of a C function exported from the native library
/// (e.g., `"boltffi_echo_i32"`, `"boltffi_free_buf"`). Goes inside a
/// `DllImport` `EntryPoint` attribute. The lowerer constructs the
/// complete symbol using the `naming::*` helpers and wraps it with
/// [`Self::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CFunctionName(String);

impl CFunctionName {
    pub fn new(name: String) -> Self {
        Self(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CFunctionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Name<GlobalSymbol>> for CFunctionName {
    fn from(symbol: Name<GlobalSymbol>) -> Self {
        Self(symbol.into_string())
    }
}

impl From<&Name<GlobalSymbol>> for CFunctionName {
    fn from(symbol: &Name<GlobalSymbol>) -> Self {
        Self(symbol.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_function_name_wraps_complete_symbol() {
        let name = CFunctionName::new("boltffi_echo_i32".to_string());
        assert_eq!(name.as_str(), "boltffi_echo_i32");
        assert_eq!(name.to_string(), "boltffi_echo_i32");
    }
}
