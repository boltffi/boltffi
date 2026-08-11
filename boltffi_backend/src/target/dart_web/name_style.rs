use boltffi_binding::CanonicalName;

use crate::core::name_case;

use super::syntax::escape_dart_identifier;

/// JS reserved words that `target::typescript` escapes on *top-level*
/// bindings (function/const names). Must match that target's own escaping
/// exactly — this target calls into the identically-named export it
/// produces, so a mismatch here means binding to a JS symbol that does not
/// exist. Copied rather than shared because it is a small, stable list and
/// pulling in a cross-target dependency for it would cost more than it
/// saves.
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

/// Naming for a single declaration, derived the same way
/// `target::typescript::name_style::Name` derives its own — both start
/// from the same shared `name_case` helpers, so the base spelling always
/// agrees. Escaping then diverges by *purpose*: `js_export_name` mirrors
/// `target::typescript`'s own top-level escaping (it must resolve to the
/// exact JS export that target produced), while `dart_identifier` applies
/// Dart's own keyword rules to whatever this becomes on the Dart side.
pub struct Name<'name>(&'name CanonicalName);

impl<'name> Name<'name> {
    pub fn new(name: &'name CanonicalName) -> Self {
        Self(name)
    }

    /// The lowerCamelCase spelling `target::typescript` exports this
    /// declaration under at module top level (functions, free constants).
    /// Top-level `export function`/`export const` bindings are real JS
    /// identifiers, so a reserved word there must be escaped exactly the
    /// way that target escapes it.
    pub fn js_export_name(&self) -> String {
        let base = name_case::lower_camel(self.0);
        if JS_RESERVED.contains(&base.as_str()) {
            format!("{base}_")
        } else {
            base
        }
    }

    /// The lowerCamelCase spelling `target::typescript` gives this
    /// declaration as a class/callback *member* (method, initializer).
    /// Unlike top-level bindings, member names are always reached through
    /// this target's `callMethod`/`getProperty` (string-keyed access), so
    /// there is no JS grammar restriction to escape around — and
    /// `target::typescript` itself does not rename them (a JS class is
    /// free to declare `static new() {}`; only top-level `export function
    /// new` would be illegal). Escaping here would bind to a property
    /// that does not exist.
    pub fn js_member_name(&self) -> String {
        name_case::lower_camel(self.0)
    }

    /// The lowerCamelCase Dart identifier for this declaration (function
    /// names, parameter names, method names).
    pub fn dart_identifier(&self) -> String {
        escape_dart_identifier(name_case::lower_camel(self.0))
    }

    /// The UpperCamelCase Dart type name (classes, records, enums,
    /// callback interfaces) — matches `target::typescript`'s own
    /// UpperCamelCase type name string-for-string, since both come from
    /// the same `upper_camel` helper.
    pub fn dart_type_name(&self) -> String {
        name_case::upper_camel(self.0)
    }

    /// UPPER_SNAKE_CASE, for associated/free constants.
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
