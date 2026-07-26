//! Kotlin naming helpers for the KMP backend.

use boltffi_binding::CanonicalName;

pub(super) fn callable_name(name: &CanonicalName) -> String {
    escape_keyword(&lower_camel_name(name))
}

pub(super) fn param_name(name: &CanonicalName) -> String {
    escape_keyword(&lower_camel_name(name))
}

pub(super) fn is_valid_identifier(name: &str) -> bool {
    if let Some(inner) = name
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
    {
        return is_kotlin_keyword(inner)
            && is_valid_unescaped_identifier(inner)
            && !is_underscore_only_identifier(inner);
    }

    is_valid_unescaped_identifier(name)
        && !is_underscore_only_identifier(name)
        && !is_kotlin_keyword(name)
}

pub(super) fn is_valid_package_segment(segment: &str) -> bool {
    is_valid_unescaped_identifier(segment) && !is_kotlin_hard_keyword(segment)
}

fn is_valid_unescaped_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|char| char == '_' || char.is_ascii_alphanumeric())
}

fn is_underscore_only_identifier(name: &str) -> bool {
    name.chars().all(|char| char == '_')
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
    is_kotlin_hard_keyword(name)
        || matches!(
            name,
            "by" | "catch"
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

fn is_kotlin_hard_keyword(name: &str) -> bool {
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
    )
}
