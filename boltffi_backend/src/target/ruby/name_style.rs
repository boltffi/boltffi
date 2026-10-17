use boltffi_binding::{CanonicalName, NamePart};

/// Ruby spelling of one canonical binding name.
pub struct Name<'name> {
    source: &'name CanonicalName,
}

impl<'name> Name<'name> {
    /// Spells `source` in Ruby.
    pub fn new(source: &'name CanonicalName) -> Self {
        Self { source }
    }

    fn snake(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_")
    }
}

/// The `snake_case` spelling of a Cargo package, such as `my_lib`.
///
/// The binding contract keeps the Cargo package name as one name part, dashes
/// included, so the dashes become underscores here.
pub fn package_snake(package: &CanonicalName) -> String {
    extension_stem(&Name::new(package).snake())
}

/// The extension file stem for a gem, such as `my_lib` for `my-lib`.
///
/// The stem names the extension directory, the compiled library, and its
/// `Init_<stem>` entry point, so it keeps only ASCII lowercase letters,
/// digits, and underscores.
pub fn extension_stem(gem: &str) -> String {
    gem.chars()
        .map(|character| match character {
            'a'..='z' | '0'..='9' | '_' => character,
            'A'..='Z' => character.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use boltffi_binding::{CanonicalName, NamePart};

    use super::{extension_stem, package_snake};

    #[test]
    fn package_names_map_to_extension_stems() {
        for package in [
            CanonicalName::single("my-lib"),
            CanonicalName::single("my_lib"),
            CanonicalName::new(vec![NamePart::new("my"), NamePart::new("lib")]),
        ] {
            assert_eq!(package_snake(&package), "my_lib");
        }
        assert_eq!(extension_stem("My-Lib"), "my_lib");
    }
}
