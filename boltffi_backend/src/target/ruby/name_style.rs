use boltffi_binding::CanonicalName;

pub struct Name<'source> {
    source: &'source CanonicalName,
}

impl<'source> Name<'source> {
    pub const fn new(source: &'source CanonicalName) -> Self {
        Self { source }
    }

    pub fn function(&self) -> String {
        let mut name = self
            .source
            .parts()
            .iter()
            .map(boltffi_binding::NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_");
        if KEYWORDS.contains(&name.as_str()) {
            name.push('_');
        }
        name
    }

    pub fn class(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(|part| upper_camel(part.as_str()))
            .collect::<String>()
    }

    pub fn constant(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(|part| screaming_snake(part.as_str()))
            .collect::<Vec<_>>()
            .join("_")
    }
}

fn upper_camel(input: &str) -> String {
    input
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn screaming_snake(input: &str) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.extend(ch.to_uppercase());
    }
    out
}

const KEYWORDS: &[&str] = &[
    "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def", "defined?", "do",
    "else", "elsif", "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not",
    "or", "redo", "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless",
    "until", "when", "while", "yield",
];
