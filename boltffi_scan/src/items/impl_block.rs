use boltffi_ast::{EnumDef, RecordDef};

use crate::declared_types::{DeclaredType, DeclaredTypes};
use crate::impl_target;
use crate::marked::Marked;
use crate::{ScanError, unsupported};

use super::impl_methods;

pub fn attach_methods(
    impls: &[Marked<'_, syn::ItemImpl>],
    declared_types: &DeclaredTypes,
    records: &mut [RecordDef],
    enums: &mut [EnumDef],
) -> Result<(), ScanError> {
    impls
        .iter()
        .try_for_each(|item| attach_impl(item, declared_types, records, enums))
}

fn attach_impl(
    marked: &Marked<'_, syn::ItemImpl>,
    declared_types: &DeclaredTypes,
    records: &mut [RecordDef],
    enums: &mut [EnumDef],
) -> Result<(), ScanError> {
    let target = impl_target::Target::scan(marked.item());
    validate(marked.item(), target.spelling())?;
    let resolved = resolve(&target, marked.scope(), declared_types)?;
    let methods =
        impl_methods::value_methods(marked.item(), resolved.id(), marked.scope(), declared_types)?;
    match resolved {
        ImplTarget::Record(id) => {
            if let Some(record) = records.iter_mut().find(|record| record.id == id) {
                record.methods.extend(methods);
            }
        }
        ImplTarget::Enum(id) => {
            if let Some(enumeration) = enums.iter_mut().find(|enumeration| enumeration.id == id) {
                enumeration.methods.extend(methods);
            }
        }
    }
    Ok(())
}

fn validate(item: &syn::ItemImpl, target: &str) -> Result<(), ScanError> {
    unsupported::generics(&item.generics, &format!("impl {target}"))?;
    if let Some((path, _)) = &item.trait_ {
        return Err(ScanError::UnsupportedMarkedImpl {
            target: target.to_owned(),
        }
        .at(path));
    }
    Ok(())
}

enum ImplTarget {
    Record(boltffi_ast::RecordId),
    Enum(boltffi_ast::EnumId),
}

impl ImplTarget {
    fn id(&self) -> &str {
        match self {
            Self::Record(id) => id.as_str(),
            Self::Enum(id) => id.as_str(),
        }
    }
}

fn resolve(
    target: &impl_target::Target<'_>,
    scope: &crate::ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ImplTarget, ScanError> {
    let Some(path) = declared_types.resolve_impl_target(scope, target)? else {
        return Err(ScanError::UnsupportedMarkedImpl {
            target: target.spelling().to_owned(),
        });
    };
    match declared_types.resolve(&path) {
        Some(DeclaredType::Record(id)) => Ok(ImplTarget::Record(id.clone())),
        Some(DeclaredType::Enum(id)) => Ok(ImplTarget::Enum(id.clone())),
        _ => Err(ScanError::UnsupportedMarkedImpl {
            target: target.spelling().to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declared_types::DeclaredTypes;
    use crate::marked::MarkedItems;
    use crate::source_tree::SourceTree;
    use boltffi_ast::{CanonicalName, EnumId, NamePart, RecordDef, RecordId, ReturnDef, TypeExpr};

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    fn source_tree(source: &str) -> SourceTree {
        SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(source)
                .expect("valid source")
                .items,
        )
        .expect("source tree")
    }

    #[test]
    fn attaches_methods_to_records_and_enums() {
        let source_tree = source_tree(
            "#[data(impl)] impl Point { pub fn origin() -> Self { todo!() } } \
             #[data(impl)] impl Mode { pub fn parse() -> Self { todo!() } }",
        );
        let marked = MarkedItems::collect(&source_tree).expect("marked");
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types.register_enum(EnumId::new("demo::Mode"));
        let mut records = vec![RecordDef::new(
            RecordId::new("demo::Point"),
            name(&["point"]),
        )];
        let mut enums = vec![boltffi_ast::EnumDef::new(
            EnumId::new("demo::Mode"),
            name(&["mode"]),
        )];

        attach_methods(marked.impls(), &declared_types, &mut records, &mut enums).expect("attach");

        assert_eq!(records[0].methods.len(), 1);
        assert_eq!(
            records[0].methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
        assert_eq!(enums[0].methods.len(), 1);
        assert_eq!(
            enums[0].methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
    }

    #[test]
    fn rejects_generic_impl_before_erasing_type_parameters() {
        let source_tree = source_tree("#[data(impl)] impl<T> Point { pub fn get(&self) {} }");
        let marked = MarkedItems::collect(&source_tree).expect("marked");
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let mut records = vec![RecordDef::new(
            RecordId::new("demo::Point"),
            name(&["point"]),
        )];
        let mut enums = Vec::new();

        let error = attach_methods(marked.impls(), &declared_types, &mut records, &mut enums)
            .expect_err("generic rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::UnsupportedGenerics {
                item: "impl Point".to_owned()
            }
        );
    }

    #[test]
    fn rejects_trait_impls_for_value_method_attachment() {
        let source_tree = source_tree("#[data(impl)] impl Display for Point {}");
        let marked = MarkedItems::collect(&source_tree).expect("marked");
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let mut records = vec![RecordDef::new(
            RecordId::new("demo::Point"),
            name(&["point"]),
        )];
        let mut enums = Vec::new();

        let error = attach_methods(marked.impls(), &declared_types, &mut records, &mut enums)
            .expect_err("trait impl rejected");

        assert_eq!(
            error.unlocated().clone(),
            ScanError::UnsupportedMarkedImpl {
                target: "Point".to_owned()
            }
        );
    }
}
