use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Result, name_case};

use super::syntax::{Constant, ConstantPath};

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

/// The default gem name for a Cargo package, such as `my-lib`.
///
/// RubyGems accepts dashes, so a Cargo package name stays as written. A
/// multipart name joins its parts with underscores.
pub fn default_gem(package: &CanonicalName) -> String {
    Name::new(package).snake()
}

/// The default Ruby module for a Cargo package, such as `MyLib` for
/// `my-lib`.
pub fn default_module(package: &CanonicalName) -> Result<ConstantPath> {
    Constant::parse(name_case::upper_camel_from_snake(&package_snake(package)))
        .map(ConstantPath::single)
}

/// The `snake_case` spelling of a Cargo package, such as `my_lib`.
///
/// The binding contract keeps the Cargo package name as one name part, dashes
/// included, so the dashes become underscores here.
pub fn package_snake(package: &CanonicalName) -> String {
    extension_stem(&default_gem(package))
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

    use super::{default_gem, default_module, extension_stem, package_snake};

    #[test]
    fn package_names_map_to_gems_modules_and_extension_stems() {
        for (package, gem) in [
            (CanonicalName::single("my-lib"), "my-lib"),
            (CanonicalName::single("my_lib"), "my_lib"),
            (
                CanonicalName::new(vec![NamePart::new("my"), NamePart::new("lib")]),
                "my_lib",
            ),
        ] {
            assert_eq!(default_gem(&package), gem);
            assert_eq!(package_snake(&package), "my_lib");
            assert_eq!(default_module(&package).unwrap().to_string(), "MyLib");
        }
        assert_eq!(extension_stem("My-Lib"), "my_lib");
    }
}
