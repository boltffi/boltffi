use boltffi_ast::{ClassDef, ClassId, ClassThreadSafety};
use syn::spanned::Spanned;

use crate::attributes::Attributes;
use crate::declared_types::DeclaredTypes;
use crate::impl_target;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::{ModuleScope, ScanError, attributes, name};

use super::impl_methods;

pub fn scan(
    marked: &[Marked<'_, syn::ItemImpl>],
    declared_types: &DeclaredTypes,
) -> Result<Vec<ClassDef>, ScanError> {
    marked
        .iter()
        .try_fold(Vec::<ClassDef>::new(), |mut classes, marked| {
            let class = build(
                marked.item(),
                marked.scope(),
                declared_types,
                marked
                    .marker()
                    .export()
                    .expect("class scanner only receives export markers")
                    .class_thread_safety(),
            )?;
            match classes
                .iter_mut()
                .find(|candidate| candidate.id == class.id)
            {
                Some(existing) => {
                    existing.thread_safety = existing.thread_safety.merge(class.thread_safety);
                    existing.methods.extend(class.methods);
                }
                None => classes.push(class),
            }
            Ok(classes)
        })
}

pub(crate) fn scan_item(
    item: &syn::ItemImpl,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
    thread_safety: ClassThreadSafety,
) -> Result<ClassDef, ScanError> {
    build(item, scope, declared_types, thread_safety)
}

fn build(
    item: &syn::ItemImpl,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
    thread_safety: ClassThreadSafety,
) -> Result<ClassDef, ScanError> {
    let target = impl_target::Target::class(item)?;
    let id = resolve_target_id(&target, scope, declared_types)?;
    let path = id.as_str();
    let name = name::source_segment(path.rsplit("::").next().unwrap_or(path));
    let mut class = ClassDef::new(id, name);
    let scanner = Scanner::new(declared_types, scope);
    let attrs = Attributes::new(&item.attrs, &scanner);
    class.source = attributes::public_source(scope, item.span());
    class.source_span = class.source.span.clone();
    class.doc = attrs.doc();
    class.deprecated = attrs.deprecated()?;
    class.user_attrs = attrs.user_attrs();
    class.methods = impl_methods::class_methods(item, class.id.as_str(), scope, declared_types)?;
    class.thread_safety = thread_safety;
    Ok(class)
}

pub(crate) fn resolve_id(
    item: &syn::ItemImpl,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ClassId, ScanError> {
    let target = impl_target::Target::class(item)?;
    resolve_target_id(&target, scope, declared_types)
}

fn resolve_target_id(
    target: &impl_target::Target<'_>,
    scope: &ModuleScope,
    declared_types: &DeclaredTypes,
) -> Result<ClassId, ScanError> {
    if declared_types.is_deferred() {
        let bare = target
            .path()
            .filter(|path| path.leading_colon.is_none() && path.segments.len() == 1);
        let Some(path) = bare else {
            return Err(ScanError::UnsupportedCapturedClassImpl {
                target: target.spelling().to_owned(),
            });
        };
        let leaf = path.segments[0].ident.to_string();
        return Ok(ClassId::new(scope.path().qualified(&leaf)));
    }
    let path = declared_types
        .resolve_impl_target(scope, target)?
        .ok_or_else(|| ScanError::UnsupportedClassImpl {
            target: target.spelling().to_owned(),
        })?;
    Ok(ClassId::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declared_types::DeclaredTypes;
    use crate::marker::Marker;
    use boltffi_ast::{
        CanonicalName, ClassId, MethodId, NamePart, Primitive, Receiver, ReturnDef, TypeExpr,
    };

    fn parse(source: &str) -> syn::ItemImpl {
        syn::parse_str(source).expect("valid impl block")
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    fn scan(source: &str) -> Result<ClassDef, ScanError> {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_class(ClassId::new("demo::Engine"));
        let item = parse(source);
        let thread_safety = Marker::detect(&item.attrs)?
            .and_then(Marker::export)
            .map(|export| export.class_thread_safety())
            .unwrap_or_default();
        build(
            &item,
            &ModuleScope::root("demo"),
            &declared_types,
            thread_safety,
        )
    }

    #[test]
    fn scans_exported_class_methods_from_inherent_impl() {
        let class = scan(
            "impl Engine { \
                pub fn new(seed: u64) -> Self { todo!() } \
                pub fn start(&mut self) {} \
                pub fn version(&self) -> u32 { 1 } \
            }",
        )
        .expect("scan");

        assert_eq!(class.id, ClassId::new("demo::Engine"));
        assert_eq!(class.name.canonical(), &name(&["engine"]));
        assert_eq!(class.methods.len(), 3);
        assert_eq!(class.methods[0].id, MethodId::new("demo::Engine::new"));
        assert_eq!(class.methods[0].receiver, Receiver::None);
        assert_eq!(
            class.methods[0].parameters[0].type_expr,
            TypeExpr::Primitive(Primitive::U64)
        );
        assert_eq!(
            class.methods[0].returns,
            ReturnDef::value(TypeExpr::SelfType)
        );
        assert_eq!(class.methods[1].receiver, Receiver::Mutable);
        assert_eq!(class.methods[2].receiver, Receiver::Shared);
        assert_eq!(
            class.methods[2].returns,
            ReturnDef::value(TypeExpr::Primitive(Primitive::U32))
        );
    }

    #[test]
    fn scans_qualified_class_impl_target() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_class(ClassId::new("demo::runtime::Engine"));
        let class = build(
            &parse("impl crate::runtime::Engine { pub fn start(&self) {} }"),
            &ModuleScope::root("demo"),
            &declared_types,
            ClassThreadSafety::default(),
        )
        .expect("scan");

        assert_eq!(class.id, ClassId::new("demo::runtime::Engine"));
        assert_eq!(class.name.canonical(), &name(&["engine"]));
    }

    #[test]
    fn scans_single_threaded_class_export_policy() {
        let class = scan("#[export(single_threaded)] impl Engine { pub fn start(&mut self) {} }")
            .expect("scan");

        assert_eq!(class.thread_safety, ClassThreadSafety::UnsafeSingleThreaded);
    }

    #[test]
    fn default_impl_keeps_class_thread_safety_assertion() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_class(ClassId::new("demo::Engine"));
        let scope = ModuleScope::root("demo");
        let mut mutable = build(
            &parse("#[export(single_threaded)] impl Engine { pub fn start(&mut self) {} }"),
            &scope,
            &declared_types,
            ClassThreadSafety::UnsafeSingleThreaded,
        )
        .expect("scan mutable impl");
        let static_methods = build(
            &parse("#[export] impl Engine { pub fn version() -> u32 { 1 } }"),
            &scope,
            &declared_types,
            ClassThreadSafety::RequireSendSync,
        )
        .expect("scan static impl");

        mutable.thread_safety = mutable.thread_safety.merge(static_methods.thread_safety);
        mutable.methods.extend(static_methods.methods);

        assert_eq!(mutable.thread_safety, ClassThreadSafety::RequireSendSync);
        assert_eq!(mutable.methods.len(), 2);
    }

    #[test]
    fn rejects_class_impl_shapes_erased_by_ast() {
        let generic =
            scan("impl<T> Engine { pub fn start(&self) {} }").expect_err("generic rejected");
        let trait_impl = scan("impl Display for Engine {}").expect_err("trait impl rejected");
        let generic_target = scan("impl Engine<u32> { pub fn start(&self) {} }")
            .expect_err("generic target rejected");

        assert_eq!(
            generic,
            ScanError::UnsupportedGenerics {
                item: "class Engine".to_owned()
            }
        );
        assert_eq!(
            trait_impl,
            ScanError::UnsupportedClassImplShape {
                target: "Engine".to_owned()
            }
        );
        assert_eq!(
            generic_target,
            ScanError::UnsupportedClassImpl {
                target: "Engine<u32>".to_owned()
            }
        );
    }

    #[test]
    fn merges_multiple_exported_impl_blocks_for_the_same_class() {
        let source_tree = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>(
                "pub struct Engine; \
                 #[export] impl Engine { pub fn new() -> Self { todo!() } } \
                 #[export] impl Engine { pub fn start(&self) {} }",
            )
            .expect("valid source")
            .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source_tree).expect("marked");
        let declared_types =
            DeclaredTypes::index(&source_tree, &marked).expect("declared type index");
        let classes = super::scan(marked.classes(), &declared_types).expect("scan");

        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].id, ClassId::new("demo::Engine"));
        assert_eq!(classes[0].methods.len(), 2);
        assert_eq!(classes[0].methods[0].id, MethodId::new("demo::Engine::new"));
        assert_eq!(
            classes[0].methods[1].id,
            MethodId::new("demo::Engine::start")
        );
    }
}
