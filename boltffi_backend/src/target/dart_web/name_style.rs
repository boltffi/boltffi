use boltffi_binding::CanonicalName;

use crate::core::name_case;
use crate::target::dart::name_style::Name as NativeName;

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
            format!("_{base}")
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

    // Delegates to target::dart's own Name for every Dart-facing
    // conversion (lowerCamelCase, UpperCamelCase, keyword escaping) --
    // the unified package's whole premise is that app code sees one Dart
    // API regardless of which half (native or web) it's actually running
    // against, so these two targets cannot have their own independent
    // casing/escaping rules.
    pub fn dart_identifier(&self) -> String {
        NativeName::new(self.0)
            .lower_camel()
            .expect(
                "a canonical name lowered from a valid Rust identifier is a valid Dart identifier",
            )
            .as_str()
            .to_owned()
    }

    pub fn dart_type_name(&self) -> String {
        NativeName::new(self.0)
            .upper_camel()
            .expect(
                "a canonical name lowered from a valid Rust identifier is a valid Dart identifier",
            )
            .as_str()
            .to_owned()
    }

    // Matches target::dart's own Constant::from_declaration, which uses
    // lowerCamelCase (not SCREAMING_SNAKE_CASE) for associated/top-level
    // constants.
    pub fn dart_constant_name(&self) -> String {
        self.dart_identifier()
    }
}
