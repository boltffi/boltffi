//! C identifier and naming rules for the ergonomic C host layer.

use boltffi_binding::CanonicalName;

use crate::core::name_case;

/// Stable C identifier casing for one source name.
///
/// The C ABI (`CBridge`) chooses the crossing symbols; the host only spells
/// the ergonomic type and member names layered on top. Type names use
/// PascalCase, function/member/symbol names use snake_case, and enum
/// constants use UPPER_SNAKE_CASE.
#[derive(Clone, Debug)]
pub struct Name {
    source: CanonicalName,
}

impl Name {
    /// Creates a C name from a source canonical name.
    pub fn new(source: &CanonicalName) -> Self {
        Self {
            source: source.clone(),
        }
    }

    /// PascalCase type spelling (e.g. `Point`, `DemoEngine`).
    pub fn r#type(&self) -> String {
        name_case::upper_camel(&self.source)
    }

    /// snake_case member / symbol spelling joined from name parts.
    pub fn member(&self) -> String {
        self.join("_", str::to_owned)
    }

    /// Upper snake-case constant spelling (e.g. `MODE_FAST`).
    pub fn constant(&self) -> String {
        self.join("_", str::to_ascii_uppercase)
    }

    fn join(&self, separator: &str, transform: impl Fn(&str) -> String) -> String {
        self.source
            .parts()
            .iter()
            .map(|part| transform(part.as_str()))
            .collect::<Vec<_>>()
            .join(separator)
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::{CanonicalName, NamePart};

    use super::Name;

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().map(|part| NamePart::new(*part)).collect())
    }

    #[test]
    fn types_use_pascal_case() {
        assert_eq!(Name::new(&name(&["point"])).r#type(), "Point");
        assert_eq!(Name::new(&name(&["engine"])).r#type(), "Engine");
        assert_eq!(
            Name::new(&name(&["multi", "word", "type"])).r#type(),
            "MultiWordType"
        );
    }

    #[test]
    fn members_use_snake_case() {
        assert_eq!(Name::new(&name(&["distance"])).member(), "distance");
        assert_eq!(Name::new(&name(&["get", "score"])).member(), "get_score");
        assert_eq!(
            Name::new(&name(&["face", "landmark"])).member(),
            "face_landmark"
        );
    }

    #[test]
    fn constants_use_upper_snake_case() {
        assert_eq!(Name::new(&name(&["mode"])).constant(), "MODE");
        assert_eq!(Name::new(&name(&["fast"])).constant(), "FAST");
    }
}
