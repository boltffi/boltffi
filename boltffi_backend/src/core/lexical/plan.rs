use std::{collections::HashMap, marker::PhantomData};

use crate::core::{Error, Result};

use super::{IdentifierKey, LexicalPolicy, NameOrdinal, NameStem, Shadowing};

pub fn with_lexical_plan<Language, Output>(
    render: impl for<'plan> FnOnce(&mut LexicalPlan<'plan, Language>) -> Result<Output>,
) -> Result<Output>
where
    Language: LexicalPolicy,
{
    let mut lexical = LexicalPlan::new();
    let output = render(&mut lexical)?;
    if lexical.locals.iter().any(|local| !local.declared) {
        return Err(Error::UnexpectedBindingShape {
            layer: "lexical planner",
            shape: "allocated local without a declaration",
        });
    }
    Ok(output)
}

pub struct LexicalPlan<'plan, Language: LexicalPolicy> {
    scopes: Vec<ScopeState>,
    domains: Vec<HashMap<IdentifierKey, usize>>,
    locals: Vec<LocalState<Language::Identifier>>,
    brand: PhantomData<&'plan mut &'plan ()>,
}

pub struct Scope<'plan, Language: LexicalPolicy> {
    index: usize,
    brand: PhantomData<&'plan mut &'plan ()>,
    language: PhantomData<fn() -> Language>,
}

pub struct LocalDeclaration<'plan, Language: LexicalPolicy> {
    index: usize,
    brand: PhantomData<&'plan mut &'plan ()>,
    language: PhantomData<fn() -> Language>,
}

pub struct LocalReference<'plan, Language: LexicalPolicy> {
    index: usize,
    brand: PhantomData<&'plan mut &'plan ()>,
    language: PhantomData<fn() -> Language>,
}

pub struct DeclaredLocal<'plan, Language: LexicalPolicy, Fragment> {
    fragment: Fragment,
    reference: LocalReference<'plan, Language>,
}

struct ScopeState {
    parent: Option<usize>,
    domain: usize,
    shadowing: Shadowing,
}

struct LocalState<Identifier> {
    identifier: Identifier,
    scope: usize,
    declared: bool,
}

impl<'plan, Language: LexicalPolicy> LexicalPlan<'plan, Language> {
    pub fn root(&self) -> Scope<'plan, Language> {
        Scope::new(0)
    }

    pub fn child(
        &mut self,
        parent: Scope<'plan, Language>,
        form: Language::ScopeForm,
    ) -> Scope<'plan, Language> {
        let index = self.scopes.len();
        self.domains.push(HashMap::new());
        self.scopes.push(ScopeState {
            parent: Some(parent.index),
            domain: index,
            shadowing: Language::shadowing(form),
        });
        Scope::new(index)
    }

    pub fn reserve_external(
        &mut self,
        scope: Scope<'plan, Language>,
        identifier: Language::Identifier,
    ) -> Option<LocalReference<'plan, Language>> {
        let key = Language::key(&identifier);
        (!self.conflicts(scope, &key)).then(|| {
            let index = self.insert(scope, key, identifier, true);
            LocalReference::new(index)
        })
    }

    pub fn allocate(
        &mut self,
        scope: Scope<'plan, Language>,
        stem: &NameStem,
    ) -> Result<LocalDeclaration<'plan, Language>> {
        let identifier =
            std::iter::successors(Some(NameOrdinal::first()), |ordinal| ordinal.next())
                .map(|ordinal| Language::generated(stem, ordinal))
                .find_map(|candidate| match candidate {
                    Ok(identifier) if !self.conflicts(scope, &Language::key(&identifier)) => {
                        Some(Ok(identifier))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .expect("lexical name ordinal space exhausted")?;
        let key = Language::key(&identifier);
        let index = self.insert(scope, key, identifier, false);
        Ok(LocalDeclaration::new(index))
    }

    pub fn shadow(
        &mut self,
        scope: Scope<'plan, Language>,
        reference: &LocalReference<'plan, Language>,
    ) -> Option<LocalDeclaration<'plan, Language>> {
        let scope_state = self.scopes.get(scope.index)?;
        let local = self.locals.get(reference.index)?;
        let key = Language::key(&local.identifier);
        let can_shadow = scope_state.shadowing == Shadowing::Allow
            && local.declared
            && self.visible(scope, local.scope)
            && !self.domains[scope_state.domain].contains_key(&key);
        let identifier = can_shadow.then(|| local.identifier.clone())?;
        let index = self.insert(scope, key, identifier, false);
        Some(LocalDeclaration::new(index))
    }

    pub fn declare<Fragment>(
        &mut self,
        declaration: LocalDeclaration<'plan, Language>,
        render: impl FnOnce(&Language::Identifier) -> Fragment,
    ) -> DeclaredLocal<'plan, Language, Fragment> {
        let local = &mut self.locals[declaration.index];
        debug_assert!(!local.declared);
        let fragment = render(&local.identifier);
        local.declared = true;
        DeclaredLocal {
            fragment,
            reference: LocalReference::new(declaration.index),
        }
    }

    pub fn resolve(
        &self,
        scope: Scope<'plan, Language>,
        reference: &LocalReference<'plan, Language>,
    ) -> Option<&Language::Identifier> {
        self.locals.get(reference.index).and_then(|local| {
            (local.declared && self.visible(scope, local.scope)).then_some(&local.identifier)
        })
    }

    fn new() -> Self {
        Self {
            scopes: vec![ScopeState {
                parent: None,
                domain: 0,
                shadowing: Shadowing::Forbid,
            }],
            domains: vec![HashMap::new()],
            locals: Vec::new(),
            brand: PhantomData,
        }
    }

    fn insert(
        &mut self,
        scope: Scope<'plan, Language>,
        key: IdentifierKey,
        identifier: Language::Identifier,
        declared: bool,
    ) -> usize {
        let index = self.locals.len();
        let domain = self.scopes[scope.index].domain;
        self.domains[domain].insert(key, index);
        self.locals.push(LocalState {
            identifier,
            scope: scope.index,
            declared,
        });
        index
    }

    fn conflicts(&self, scope: Scope<'plan, Language>, key: &IdentifierKey) -> bool {
        let scope_state = &self.scopes[scope.index];
        match scope_state.shadowing {
            Shadowing::Allow => self.domains[scope_state.domain].contains_key(key),
            Shadowing::Forbid => {
                std::iter::successors(Some(scope.index), |index| self.scopes[*index].parent)
                    .map(|index| self.scopes[index].domain)
                    .any(|domain| self.domains[domain].contains_key(key))
            }
        }
    }

    fn visible(&self, scope: Scope<'plan, Language>, declaration_scope: usize) -> bool {
        std::iter::successors(Some(scope.index), |index| self.scopes[*index].parent)
            .any(|index| index == declaration_scope)
    }
}

impl<'plan, Language: LexicalPolicy, Fragment> DeclaredLocal<'plan, Language, Fragment> {
    pub fn into_parts(self) -> (Fragment, LocalReference<'plan, Language>) {
        (self.fragment, self.reference)
    }
}

impl<'plan, Language: LexicalPolicy> Scope<'plan, Language> {
    fn new(index: usize) -> Self {
        Self {
            index,
            brand: PhantomData,
            language: PhantomData,
        }
    }
}

impl<Language: LexicalPolicy> Copy for Scope<'_, Language> {}

impl<Language: LexicalPolicy> Clone for Scope<'_, Language> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'plan, Language: LexicalPolicy> LocalDeclaration<'plan, Language> {
    fn new(index: usize) -> Self {
        Self {
            index,
            brand: PhantomData,
            language: PhantomData,
        }
    }
}

impl<'plan, Language: LexicalPolicy> LocalReference<'plan, Language> {
    fn new(index: usize) -> Self {
        Self {
            index,
            brand: PhantomData,
            language: PhantomData,
        }
    }
}

impl<Language: LexicalPolicy> Clone for LocalReference<'_, Language> {
    fn clone(&self) -> Self {
        Self::new(self.index)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use crate::core::{
        LanguageSyntax, Result,
        lexical::{IdentifierKey, LexicalPolicy, NameOrdinal, NameStem, Shadowing},
        syntax::sealed,
    };

    use super::with_lexical_plan;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ScopeForm {
        AllowShadowing,
        ForbidShadowing,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct TestSyntax;

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct Fragment(String);

    impl fmt::Display for Fragment {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(&self.0)
        }
    }

    impl sealed::SyntaxFragment for Fragment {}

    impl LanguageSyntax for TestSyntax {
        const KEYWORDS: &'static [&'static str] = &[];

        type Identifier = Fragment;
        type Type = Fragment;
        type Expr = Fragment;
        type Stmt = Fragment;
        type Literal = Fragment;
        type Arguments = Fragment;
    }

    impl sealed::LanguageSyntax for TestSyntax {}

    impl LexicalPolicy for TestSyntax {
        type ScopeForm = ScopeForm;

        fn key(identifier: &Self::Identifier) -> IdentifierKey {
            IdentifierKey::new(&identifier.0)
        }

        fn generated(stem: &NameStem, ordinal: NameOrdinal) -> Result<Self::Identifier> {
            let stem = stem.parts().collect::<Vec<_>>().join("_");
            let suffix = if ordinal.get() > 1 {
                ordinal.get().to_string()
            } else {
                String::new()
            };
            Ok(Fragment(format!("{stem}{suffix}")))
        }

        fn shadowing(form: Self::ScopeForm) -> Shadowing {
            match form {
                ScopeForm::AllowShadowing => Shadowing::Allow,
                ScopeForm::ForbidShadowing => Shadowing::Forbid,
            }
        }
    }

    #[test]
    fn generated_names_skip_reserved_and_allocated_spellings() {
        with_lexical_plan::<TestSyntax, _>(|lexical| {
            let scope = lexical.root();
            lexical
                .reserve_external(scope, Fragment("value2".to_owned()))
                .expect("external name should be unique");
            let first_declaration = lexical.allocate(scope, &NameStem::new("value"))?;
            let first = lexical
                .declare(first_declaration, |identifier| identifier.0.clone())
                .into_parts()
                .0;
            assert_eq!(first, "value");
            let second_declaration = lexical.allocate(scope, &NameStem::new("value"))?;
            let second = lexical
                .declare(second_declaration, |identifier| identifier.0.clone())
                .into_parts()
                .0;
            assert_eq!(second, "value3");
            Ok(())
        })
        .expect("lexical plan");
    }

    #[test]
    fn external_names_take_precedence_over_generated_names() {
        with_lexical_plan::<TestSyntax, _>(|lexical| {
            let scope = lexical.root();
            lexical
                .reserve_external(scope, Fragment("value".to_owned()))
                .expect("external name should be unique");
            let declaration = lexical.allocate(scope, &NameStem::new("value"))?;
            let name = lexical
                .declare(declaration, |identifier| identifier.0.clone())
                .into_parts()
                .0;
            assert_eq!(name, "value2");
            Ok(())
        })
        .expect("lexical plan");
    }

    #[test]
    fn allowed_child_shadow_preserves_the_parent_spelling() {
        with_lexical_plan::<TestSyntax, _>(|lexical| {
            let root = lexical.root();
            let declaration = lexical.allocate(root, &NameStem::new("value"))?;
            let reference = lexical
                .declare(declaration, |identifier| identifier.0.clone())
                .into_parts()
                .1;
            let child = lexical.child(root, ScopeForm::AllowShadowing);
            let shadow = lexical
                .shadow(child, &reference)
                .expect("visible parent can be shadowed");
            let (name, shadow_reference) = lexical
                .declare(shadow, |identifier| identifier.0.clone())
                .into_parts();
            assert_eq!(name, "value");
            assert!(lexical.resolve(child, &shadow_reference).is_some());
            assert!(lexical.resolve(root, &shadow_reference).is_none());
            Ok(())
        })
        .expect("lexical plan");
    }

    #[test]
    fn forbidden_child_shadow_disambiguates_visible_names() {
        with_lexical_plan::<TestSyntax, _>(|lexical| {
            let root = lexical.root();
            let declaration = lexical.allocate(root, &NameStem::new("value"))?;
            let reference = lexical
                .declare(declaration, |identifier| identifier.0.clone())
                .into_parts()
                .1;
            let child = lexical.child(root, ScopeForm::ForbidShadowing);
            assert!(lexical.shadow(child, &reference).is_none());
            let child_declaration = lexical.allocate(child, &NameStem::new("value"))?;
            let child_name = lexical
                .declare(child_declaration, |identifier| identifier.0.clone())
                .into_parts()
                .0;
            assert_eq!(child_name, "value2");
            Ok(())
        })
        .expect("lexical plan");
    }

    #[test]
    fn sibling_locals_cannot_escape_their_scope() {
        with_lexical_plan::<TestSyntax, _>(|lexical| {
            let root = lexical.root();
            let first_child = lexical.child(root, ScopeForm::AllowShadowing);
            let second_child = lexical.child(root, ScopeForm::AllowShadowing);
            let declaration = lexical.allocate(first_child, &NameStem::new("value"))?;
            let reference = lexical
                .declare(declaration, |identifier| identifier.0.clone())
                .into_parts()
                .1;
            assert!(lexical.resolve(first_child, &reference).is_some());
            assert!(lexical.resolve(second_child, &reference).is_none());
            Ok(())
        })
        .expect("lexical plan");
    }

    #[test]
    fn pending_declarations_fail_plan_finalization() {
        let result = with_lexical_plan::<TestSyntax, _>(|lexical| {
            lexical.allocate(lexical.root(), &NameStem::new("value"))?;
            Ok(())
        });
        assert!(result.is_err());
    }
}
