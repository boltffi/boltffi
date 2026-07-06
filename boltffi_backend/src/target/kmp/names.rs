//! Kotlin naming helpers for the KMP backend.

use boltffi_binding::CanonicalName;

pub(super) fn callable_name(name: &CanonicalName) -> String {
    escape_keyword(&lower_camel_name(name))
}

pub(super) fn param_name(name: &CanonicalName) -> String {
    escape_keyword(&lower_camel_name(name))
}

fn lower_camel_name(name: &CanonicalName) -> String {
    let mut words = name
        .parts()
        .iter()
        .flat_map(|part| part.as_str().split('_'))
        .filter(|part| !part.is_empty());
    let Some(first) = words.next() else {
        return String::new();
    };

    let mut rendered = first.to_ascii_lowercase();
    for word in words {
        rendered.push_str(&upper_camel_word(word));
    }
    rendered
}

fn upper_camel_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut rendered = first.to_uppercase().collect::<String>();
    rendered.push_str(&chars.as_str().to_ascii_lowercase());
    rendered
}

fn escape_keyword(name: &str) -> String {
    if is_kotlin_keyword(name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

fn is_kotlin_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "class"
            | "continue"
            | "do"
            | "else"
            | "false"
            | "for"
            | "fun"
            | "if"
            | "in"
            | "interface"
            | "is"
            | "null"
            | "object"
            | "package"
            | "return"
            | "super"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typealias"
            | "typeof"
            | "val"
            | "var"
            | "when"
            | "while"
            | "by"
            | "catch"
            | "constructor"
            | "delegate"
            | "dynamic"
            | "field"
            | "file"
            | "finally"
            | "get"
            | "import"
            | "init"
            | "param"
            | "property"
            | "receiver"
            | "set"
            | "setparam"
            | "value"
            | "where"
            | "actual"
            | "abstract"
            | "annotation"
            | "companion"
            | "const"
            | "crossinline"
            | "data"
            | "enum"
            | "expect"
            | "external"
            | "final"
            | "infix"
            | "inline"
            | "inner"
            | "internal"
            | "lateinit"
            | "noinline"
            | "open"
            | "operator"
            | "out"
            | "override"
            | "private"
            | "protected"
            | "public"
            | "reified"
            | "sealed"
            | "suspend"
            | "tailrec"
            | "vararg"
    )
}
