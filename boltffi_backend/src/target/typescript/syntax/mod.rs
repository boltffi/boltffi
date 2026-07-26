mod declaration;
mod expression;
mod identifier;
mod literal;
mod property;
mod type_name;

use crate::core::{LanguageSyntax, syntax::sealed};

pub use declaration::MethodDeclaration;
pub use expression::{ArgumentList, Expression, Statement};
pub use identifier::{Identifier, MemberName};
pub use literal::{IntegerLiteral, StringLiteral};
pub use property::PropertyKey;
pub use type_name::TypeName;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ];

    type Identifier = Identifier;
    type Type = TypeName;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = StringLiteral;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}
