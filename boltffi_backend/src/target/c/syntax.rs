//! C host syntax fragment family.
//!
//! The C host renders directly over the shared C ABI (`CBridge`), so its
//! syntax fragment types are the same C fragments the bridge already emits.
//! This module re-exposes them under the host's `LanguageSyntax` contract so
//! host render models can type fragments by grammar role.
use crate::bridge::c::{ArgumentList, Expression, Identifier, Literal, Statement, TypeFragment};
use crate::core::LanguageSyntax;
use crate::core::syntax::sealed;

/// C host syntax marker implementing the host `LanguageSyntax` contract.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = crate::bridge::c::Syntax::KEYWORDS;

    type Identifier = Identifier;
    type Type = TypeFragment;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

#[cfg(test)]
mod tests {
    use crate::{
        bridge::c::{Identifier, TypeFragment},
        core::LanguageSyntax,
        target::c::Syntax,
    };

    #[test]
    fn syntax_reuses_c_keyword_set() {
        assert!(Syntax::keyword("struct"));
        assert!(Syntax::keyword("void"));
        assert!(!Syntax::keyword("point"));
    }

    #[test]
    fn reserved_words_are_escaped_with_a_stable_suffix() {
        assert_eq!(
            Identifier::escape("struct").expect("escaped").as_str(),
            "struct_"
        );
        assert_eq!(
            Identifier::escape("case").expect("escaped").as_str(),
            "case_"
        );
    }

    #[test]
    fn type_fragment_renders_pointer_types() {
        // Pointer types render as `const <inner> *` through the bridge fragment.
        let fragment = TypeFragment::anonymous(&crate::bridge::c::Type::ConstPointer(Box::new(
            crate::bridge::c::Type::Uint8,
        )))
        .expect("pointer type fragment");
        assert_eq!(fragment.to_string(), "const uint8_t *");
    }
}
