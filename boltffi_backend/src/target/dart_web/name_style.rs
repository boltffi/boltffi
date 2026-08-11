use boltffi_binding::CanonicalName;

use crate::core::name_case;

use super::syntax::escape_dart_identifier;

// Must match target::typescript's own top-level escaping exactly, or this
// binds to a JS export that doesn't exist.
const JS_RESERVED: &[&str] = &[
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

pub struct Name<'name>(&'name CanonicalName);

impl<'name> Name<'name> {
    pub fn new(name: &'name CanonicalName) -> Self {
        Self(name)
    }

    pub fn js_export_name(&self) -> String {
        let base = name_case::lower_camel(self.0);
        if JS_RESERVED.contains(&base.as_str()) {
            format!("{base}_")
        } else {
            base
        }
    }

    // Members are reached through callMethod/getProperty (string-keyed),
    // not JS grammar, and target::typescript doesn't rename them either
    // -- escaping here would bind to a property that doesn't exist.
    pub fn js_member_name(&self) -> String {
        name_case::lower_camel(self.0)
    }

    pub fn dart_identifier(&self) -> String {
        escape_dart_identifier(name_case::lower_camel(self.0))
    }

    pub fn dart_type_name(&self) -> String {
        name_case::upper_camel(self.0)
    }

    pub fn dart_constant_name(&self) -> String {
        escape_dart_identifier(
            self.0
                .parts()
                .iter()
                .map(boltffi_binding::NamePart::as_str)
                .collect::<Vec<_>>()
                .join("_")
                .to_ascii_uppercase(),
        )
    }
}
