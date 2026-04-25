use std::fmt;

use boltffi_ffi_rules::naming::{GlobalSymbol, Name};

/// The C-side function symbol used as the `EntryPoint` of a `[DllImport]` attribute.
///
/// Examples:
/// ```csharp
/// [DllImport(LibName, EntryPoint = "boltffi_echo_i32")]
/// //                                ^^^^^^^^^^^^^^^^
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CFunctionName(String);

impl CFunctionName {
    pub fn new(name: String) -> Self {
        Self(name)
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
        assert_eq!(name.to_string(), "boltffi_echo_i32");
    }
}
