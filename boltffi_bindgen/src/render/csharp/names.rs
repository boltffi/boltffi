//! How elements get named in C#. Stateless identifier transforms
//! (snake_case source → PascalCase / camelCase), plus escaping with
//! `@` for reserved keywords. Used by `plan`, `lower`, and `emit`
//! wherever the backend needs to render a name.

use boltffi_ffi_rules::naming;

/// C# naming conventions. Methods and properties use PascalCase,
/// fields use camelCase, and reserved keywords are escaped with `@`.
pub struct NamingConvention;

impl NamingConvention {
    /// Type name: `my_record` → `MyRecord`.
    pub fn class_name(name: &str) -> String {
        naming::to_upper_camel_case(name)
    }

    /// Namespace: derived from the crate name in PascalCase.
    pub fn namespace(name: &str) -> String {
        naming::to_upper_camel_case(name)
    }

    /// Method name: `do_thing` → `DoThing` (PascalCase, C# convention).
    pub fn method_name(name: &str) -> String {
        let converted = naming::to_upper_camel_case(name);
        Self::escape_keyword(&converted)
    }

    /// Property name: `my_prop` → `MyProp`.
    pub fn property_name(name: &str) -> String {
        let converted = naming::to_upper_camel_case(name);
        Self::escape_keyword(&converted)
    }

    /// Field name: `my_field` → `myField` (camelCase for private fields).
    pub fn field_name(name: &str) -> String {
        let converted = naming::snake_to_camel(name);
        Self::escape_keyword(&converted)
    }

    /// Prefix a C# keyword with `@` so it can be used as an identifier.
    pub fn escape_keyword(name: &str) -> String {
        if Self::is_csharp_keyword(name) {
            format!("@{}", name)
        } else {
            name.to_string()
        }
    }

    fn is_csharp_keyword(name: &str) -> bool {
        matches!(
            name,
            "abstract"
                | "as"
                | "base"
                | "bool"
                | "break"
                | "byte"
                | "case"
                | "catch"
                | "char"
                | "checked"
                | "class"
                | "const"
                | "continue"
                | "decimal"
                | "default"
                | "delegate"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "event"
                | "explicit"
                | "extern"
                | "false"
                | "finally"
                | "fixed"
                | "float"
                | "for"
                | "foreach"
                | "goto"
                | "if"
                | "implicit"
                | "in"
                | "int"
                | "interface"
                | "internal"
                | "is"
                | "lock"
                | "long"
                | "namespace"
                | "new"
                | "null"
                | "object"
                | "operator"
                | "out"
                | "override"
                | "params"
                | "private"
                | "protected"
                | "public"
                | "readonly"
                | "ref"
                | "return"
                | "sbyte"
                | "sealed"
                | "short"
                | "sizeof"
                | "stackalloc"
                | "static"
                | "string"
                | "struct"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "typeof"
                | "uint"
                | "ulong"
                | "unchecked"
                | "unsafe"
                | "ushort"
                | "using"
                | "virtual"
                | "void"
                | "volatile"
                | "while"
        )
    }

    /// The global FFI prefix used for all C symbols (e.g., `"boltffi"`).
    pub fn ffi_prefix() -> String {
        naming::ffi_prefix().to_string()
    }

    /// FFI prefix scoped to a class (e.g., `"boltffi_my_class"`).
    pub fn class_ffi_prefix(class_name: &str) -> String {
        naming::class_ffi_prefix(class_name).into_string()
    }
}
