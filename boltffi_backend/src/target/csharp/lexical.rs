use crate::core::{
    Result,
    lexical::{IdentifierKey, LexicalPolicy, NameOrdinal, NameStem, Shadowing},
};

use super::syntax::{Identifier, Syntax};

impl LexicalPolicy for Syntax {
    type ScopeForm = ();

    fn key(identifier: &Identifier) -> IdentifierKey {
        IdentifierKey::new(identifier.as_str().trim_start_matches('@'))
    }

    fn generated(stem: &NameStem, ordinal: NameOrdinal) -> Result<Identifier> {
        let stem = stem.parts().collect::<String>();
        let suffix = if ordinal.get() == 1 {
            String::new()
        } else {
            ordinal.get().to_string()
        };
        Identifier::escape(format!("{stem}{suffix}"))
    }

    fn shadowing(_: Self::ScopeForm) -> Shadowing {
        Shadowing::Forbid
    }
}

#[cfg(test)]
mod tests {
    use crate::core::lexical::{IdentifierKey, NameStem, with_lexical_plan};

    use super::{Identifier, LexicalPolicy, Syntax};

    #[test]
    fn verbatim_keyword_uses_the_unescaped_collision_key() {
        let keyword = Identifier::escape("class").expect("escaped C# keyword");
        assert_eq!(keyword.as_str(), "@class");
        assert_eq!(Syntax::key(&keyword), IdentifierKey::new("class"));

        with_lexical_plan::<Syntax, _>(|lexical| {
            let scope = lexical.root();
            lexical
                .reserve_external(scope, keyword)
                .expect("external name should be unique");
            let declaration = lexical.allocate(scope, &NameStem::new("class"))?;
            let name = lexical.declare(declaration, Clone::clone).into_parts().0;
            assert_eq!(name.as_str(), "class2");
            Ok(())
        })
        .expect("C# lexical plan");
    }

    #[test]
    fn generated_names_preserve_casing_and_skip_reserved_suffixes() {
        with_lexical_plan::<Syntax, _>(|lexical| {
            let scope = lexical.root();
            lexical
                .reserve_external(scope, Identifier::parse("boltffiStatus")?)
                .expect("external name should be unique");
            lexical
                .reserve_external(scope, Identifier::parse("boltffiStatus2")?)
                .expect("external name should be unique");
            let declaration =
                lexical.allocate(scope, &NameStem::new("boltffi").suffixed("Status"))?;
            let name = lexical.declare(declaration, Clone::clone).into_parts().0;
            assert_eq!(name.as_str(), "boltffiStatus3");
            Ok(())
        })
        .expect("C# lexical plan");
    }

    #[test]
    fn child_scope_avoids_visible_parent_name() {
        with_lexical_plan::<Syntax, _>(|lexical| {
            let root = lexical.root();
            lexical
                .reserve_external(root, Identifier::parse("boltffiFuture")?)
                .expect("external name should be unique");
            let child = lexical.child(root, ());
            let declaration = lexical.allocate(child, &NameStem::new("boltffiFuture"))?;
            let name = lexical.declare(declaration, Clone::clone).into_parts().0;
            assert_eq!(name.as_str(), "boltffiFuture2");
            Ok(())
        })
        .expect("C# lexical plan");
    }
}
