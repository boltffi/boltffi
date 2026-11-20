//! One foreign name per top-level declaration across the crates a contract gathers.

use std::collections::HashMap;

use boltffi_ast::{CanonicalName, SourceContract};

use super::LowerError;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum Namespace {
    Type,
    Function,
    Constant,
}

pub(super) fn require_unique(source: &SourceContract) -> Result<(), LowerError> {
    let types = source
        .records
        .iter()
        .map(|record| (&record.name, record.id.as_str()))
        .chain(
            source
                .enums
                .iter()
                .map(|item| (&item.name, item.id.as_str())),
        )
        .chain(
            source
                .classes
                .iter()
                .map(|item| (&item.name, item.id.as_str())),
        )
        .chain(
            source
                .traits
                .iter()
                .map(|item| (&item.name, item.id.as_str())),
        )
        .chain(
            source
                .customs
                .iter()
                .map(|item| (&item.name, item.id.as_str())),
        )
        .map(|(name, id)| (Namespace::Type, name, id));
    let functions = source
        .functions
        .iter()
        .map(|function| (Namespace::Function, &function.name, function.id.as_str()));
    let constants = source
        .constants
        .iter()
        .filter(|constant| constant.owner.is_none())
        .map(|constant| (Namespace::Constant, &constant.name, constant.id.as_str()));
    let mut seen = HashMap::<(Namespace, &CanonicalName), &str>::new();
    types
        .chain(functions)
        .chain(constants)
        .try_for_each(|(namespace, name, id)| {
            match seen.insert((namespace, name.canonical()), id) {
                Some(first) if crate_of(first) != crate_of(id) => Err(
                    LowerError::foreign_name_collision(name.spelling(), first, id),
                ),
                _ => Ok(()),
            }
        })
}

fn crate_of(id: &str) -> &str {
    id.split("::").next().unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, FunctionDef, PackageInfo, RecordDef, SourceContract, SourceName,
    };

    use crate::Native;
    use crate::lower::{LowerErrorKind, lower};

    fn name(spelling: &str) -> SourceName {
        SourceName::new(spelling, CanonicalName::single(spelling.to_lowercase()))
    }

    #[test]
    fn same_named_types_from_two_crates_are_refused_by_id() {
        let mut contract = SourceContract::new(PackageInfo::new("app", None));
        contract.records = vec![
            RecordDef::new("app::Point".into(), name("Point")),
            RecordDef::new("shapes::Point".into(), name("Point")),
        ];

        let error = lower::<Native>(&contract).expect_err("two records bind as Point");

        assert!(
            matches!(
                error.kind(),
                LowerErrorKind::ForeignNameCollision { first, second, .. }
                    if first == "app::Point" && second == "shapes::Point"
            ),
            "{error}"
        );
    }

    #[test]
    fn same_named_types_of_one_crate_are_left_to_the_backends() {
        let mut contract = SourceContract::new(PackageInfo::new("app", None));
        contract.records = vec![
            RecordDef::new("app::audio::Point".into(), name("Point")),
            RecordDef::new("app::video::Point".into(), name("Point")),
        ];

        lower::<Native>(&contract).expect("one crate's same-named records lower");
    }

    #[test]
    fn a_function_may_share_a_type_name() {
        let mut contract = SourceContract::new(PackageInfo::new("app", None));
        contract.records = vec![RecordDef::new("app::Point".into(), name("Point"))];
        contract.functions = vec![FunctionDef::new("shapes::point".into(), name("point"))];

        lower::<Native>(&contract).expect("a function and a record live in separate namespaces");
    }
}
