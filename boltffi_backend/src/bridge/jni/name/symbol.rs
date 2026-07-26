//! JNI exported symbol names.
//!
//! Java finds native methods through `Java_<package>_<class>_<method>` symbols.
//! The symbol is not simple string concatenation: underscores and non-identifier
//! characters have JNI-specific escaping rules, and overloaded method forms add
//! their own suffix rules.
//!
//! This module owns that exported symbol spelling. Native method contracts ask
//! for a `JniSymbolName`; they do not build `Java_*` names manually.

use std::fmt::{self, Write};

use crate::{
    bridge::{c::Identifier, jni::name::JvmClassPath},
    core::{Error, Result},
};

/// C symbol name exported through JNI's `Java_*` naming convention.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JniSymbolName {
    identifier: Identifier,
}

struct EscapedPart {
    spelling: String,
    ambiguous: bool,
}

impl JniSymbolName {
    /// Creates a JNI native-method symbol for a JVM class and method name.
    pub fn native_method(class: &JvmClassPath, method: &str) -> Result<Self> {
        let class = class.jni_prefix();
        let method = Self::escape_part(method)?;
        Ok(Self {
            identifier: Identifier::parse(format!("Java_{class}_{method}"))?,
        })
    }

    /// Returns the generated C identifier.
    pub fn as_identifier(&self) -> &Identifier {
        &self.identifier
    }

    pub(in crate::bridge::jni::name) fn escape_part(part: &str) -> Result<String> {
        EscapedPart::new(part).checked()
    }

    pub(in crate::bridge::jni::name) fn escape_validated_part(part: &str) -> String {
        EscapedPart::new(part).spelling
    }
}

impl EscapedPart {
    fn new(part: &str) -> Self {
        let (spelling, _, ambiguous) = part.encode_utf16().fold(
            (String::with_capacity(part.len()), true, false),
            |(mut escaped, escaped_boundary, ambiguous), code_unit| {
                let ambiguous = ambiguous || (escaped_boundary && matches!(code_unit, 0x30..=0x33));
                match code_unit {
                    0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a => {
                        escaped.push(char::from_u32(u32::from(code_unit)).expect("ASCII code unit"))
                    }
                    0x5f => escaped.push_str("_1"),
                    0x3b => escaped.push_str("_2"),
                    0x5b => escaped.push_str("_3"),
                    0x2f => escaped.push('_'),
                    code_unit => {
                        write!(&mut escaped, "_0{code_unit:04x}")
                            .expect("writing a JNI escape to a string");
                    }
                }
                (escaped, code_unit == 0x2f, ambiguous)
            },
        );
        Self {
            spelling,
            ambiguous,
        }
    }

    fn checked(self) -> Result<String> {
        match self.ambiguous {
            true => Err(Error::UnsupportedBridge {
                bridge: "jni",
                shape: "JVM name with an ambiguous JNI escape",
            }),
            false => Ok(self.spelling),
        }
    }
}

impl fmt::Display for JniSymbolName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identifier.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use crate::bridge::jni::JvmClassPath;

    use super::JniSymbolName;

    #[test]
    fn escapes_every_non_ascii_alphanumeric_utf16_unit() {
        let class = JvmClassPath::new("com.bøltffi.包", "Native$桥").expect("JVM class");
        let symbol = JniSymbolName::native_method(&class, "méthod_[];$😀").expect("JNI symbol");

        assert_eq!(
            symbol.to_string(),
            "Java_com_b_000f8ltffi__05305_Native_00024_06865_m_000e9thod_1_3_0005d_2_00024_0d83d_0de00"
        );
    }

    #[test]
    fn preserves_only_ascii_alphanumeric_units() {
        let class = JvmClassPath::new("com.boltffi", "Native").expect("JVM class");
        let symbol = JniSymbolName::native_method(&class, "azAZ09/-").expect("JNI symbol");

        assert_eq!(symbol.to_string(), "Java_com_boltffi_Native_azAZ09__0002d");
    }

    #[test]
    fn rejects_binary_names_with_ambiguous_jni_escapes() {
        let valid = JvmClassPath::new("com.boltffi", "Native").expect("JVM class");

        assert!(JvmClassPath::new("com.boltffi", "3Native").is_err());
        assert!(JniSymbolName::native_method(&valid, "2call").is_err());
        assert!(JniSymbolName::native_method(&valid, "call/3").is_err());
        assert!(JniSymbolName::native_method(&valid, "call_3").is_ok());
    }
}
