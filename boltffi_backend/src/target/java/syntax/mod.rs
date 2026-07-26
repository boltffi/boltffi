mod expression;
mod identifier;
mod literal;
mod type_name;
mod unicode;

use crate::core::{LanguageSyntax, syntax::sealed};

pub use expression::{ArgumentList, Expression, Statement};
pub use identifier::{Identifier, TypeIdentifier};
pub use literal::{Javadoc, StringLiteral};
pub use type_name::TypeName;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "abstract",
        "assert",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "int",
        "interface",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "strictfp",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "void",
        "volatile",
        "while",
    ];

    type Identifier = Identifier;
    type Type = TypeName;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = StringLiteral;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}
