//! Probes the composable descriptor: no container vocabulary, and aliases resolve.

use std::collections::HashMap;

use pim_runtime::{Meta, TypeMeta};

pub struct Point {
    pub x: f64,
}

impl<Tag> TypeMeta<Tag> for Point {
    const META: Meta = Meta::new()
        .push(module_path!().as_bytes())
        .push(b"::")
        .push(b"Point");
}

pub type Points = Vec<Point>;
pub type Lookup = HashMap<String, Points>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PimTag;

    #[test]
    fn composes_a_descriptor_through_an_alias() {
        const DESCRIPTOR: &str = <Points as TypeMeta<PimTag>>::META.as_str();

        assert_eq!(
            DESCRIPTOR, "Vec<pim_toy::composed::Point>",
            "the compiler sees through the alias to the blanket impl"
        );
    }

    #[test]
    fn composes_a_nested_descriptor() {
        const DESCRIPTOR: &str = <Lookup as TypeMeta<PimTag>>::META.as_str();

        assert_eq!(
            DESCRIPTOR, "HashMap<String, Vec<pim_toy::composed::Point>>",
            "descriptors nest without the macro knowing any container names"
        );
    }

    #[test]
    fn composes_a_descriptor_for_a_type_the_macro_never_saw() {
        const DESCRIPTOR: &str = <Option<Box<Vec<u64>>> as TypeMeta<PimTag>>::META.as_str();

        assert_eq!(
            DESCRIPTOR, "Option<Box<Vec<u64>>>",
            "any composition of known containers resolves"
        );
    }
}
