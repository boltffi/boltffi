use boltffi_binding::{CanonicalName, NamePart};

pub fn lower_camel(name: &CanonicalName) -> String {
    name.parts()
        .iter()
        .enumerate()
        .map(|(index, part)| match index {
            0 => part.as_str().to_owned(),
            _ => capitalized(part.as_str()),
        })
        .collect()
}

pub fn upper_camel(name: &CanonicalName) -> String {
    name.source_spelling()
        .filter(|spelling| PascalSource::new(spelling).is_some())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| upper_camel_parts(name.parts()))
}

pub fn upper_camel_parts(parts: &[NamePart]) -> String {
    parts
        .iter()
        .map(NamePart::as_str)
        .map(capitalized)
        .collect()
}

pub fn upper_camel_from_snake(name: &str) -> String {
    name.split('_').map(capitalized).collect()
}

fn capitalized(part: &str) -> String {
    let mut characters = part.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

struct PascalSource;

impl PascalSource {
    fn new(spelling: &str) -> Option<Self> {
        let mut characters = spelling.chars();
        let starts_uppercase = characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase());
        (starts_uppercase
            && characters.all(|character| character.is_ascii_alphanumeric())
            && !spelling.contains('_'))
        .then_some(Self)
    }
}
