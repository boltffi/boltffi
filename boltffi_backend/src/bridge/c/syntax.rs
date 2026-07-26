use std::{
    borrow::Borrow,
    fmt::{self, Write},
};

use crate::core::{LanguageSyntax, Result, syntax::sealed};

use super::{Function, Identifier, Parameter, Type};

/// C syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// C type syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeFragment(String);

/// C expression syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Expression(String);

/// C statement syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Statement(String);

/// C literal syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Literal(String);

/// C argument list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArgumentList(Vec<Expression>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
        "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
        "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
        "union", "unsigned", "void", "volatile", "while",
    ];

    type Identifier = Identifier;
    type Type = TypeFragment;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for TypeFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for TypeFragment {}

impl TypeFragment {
    /// Creates C type syntax.
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Expression {}

impl Expression {
    pub(crate) fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }

    pub(crate) fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub(crate) fn literal(literal: Literal) -> Self {
        Self(literal.to_string())
    }

    pub(crate) fn call(function: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{function}({arguments})"))
    }

    pub(crate) fn address_of(expression: Self) -> Self {
        Self(format!("&{expression}"))
    }

    pub(crate) fn cast(ty: TypeFragment, expression: Self) -> Self {
        Self(format!("({ty}){expression}"))
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Statement {}

impl Statement {
    /// Creates C statement syntax.
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Literal {}

impl Literal {
    pub(crate) fn integer_zero() -> Self {
        Self("0".to_owned())
    }

    pub(crate) fn bool_false() -> Self {
        Self("false".to_owned())
    }

    pub(crate) fn f32_zero() -> Self {
        Self("0.0f".to_owned())
    }

    pub(crate) fn f64_zero() -> Self {
        Self("0.0".to_owned())
    }

    pub(crate) fn compound_zero() -> Self {
        Self("{0}".to_owned())
    }

    pub(crate) fn null_pointer() -> Self {
        Self("NULL".to_owned())
    }

    pub(crate) fn status_failure() -> Self {
        Self("{.code = 1}".to_owned())
    }

    pub(crate) fn string(value: &str) -> Self {
        Self(format!("{value:?}"))
    }

    pub(crate) fn byte_string(bytes: &[u8]) -> Self {
        let contents =
            bytes
                .iter()
                .fold(String::with_capacity(bytes.len()), |mut literal, byte| {
                    match byte {
                        b'"' => literal.push_str("\\\""),
                        b'\\' => literal.push_str("\\\\"),
                        b'?' => literal.push_str("\\077"),
                        0x20..=0x7e => literal.push(char::from(*byte)),
                        _ => write!(&mut literal, "\\{byte:03o}")
                            .expect("writing a byte escape to a string"),
                    }
                    literal
                });
        Self(format!("\"{contents}\""))
    }
}

impl fmt::Display for ArgumentList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            &self
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl sealed::SyntaxFragment for ArgumentList {}

impl ArgumentList {
    pub(crate) fn from_iter(arguments: impl IntoIterator<Item = Expression>) -> Self {
        Self(arguments.into_iter().collect())
    }
}

impl TypeFragment {
    pub(crate) fn anonymous(ty: &Type) -> Result<TypeFragment> {
        Ok(TypeFragment::new(match ty {
            Type::Void => "void".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Int8 => "int8_t".to_owned(),
            Type::Uint8 => "uint8_t".to_owned(),
            Type::Int16 => "int16_t".to_owned(),
            Type::Uint16 => "uint16_t".to_owned(),
            Type::Int32 => "int32_t".to_owned(),
            Type::Uint32 => "uint32_t".to_owned(),
            Type::Int64 => "int64_t".to_owned(),
            Type::Uint64 => "uint64_t".to_owned(),
            Type::Float32 => "float".to_owned(),
            Type::Float64 => "double".to_owned(),
            Type::SignedPointerWidth => "intptr_t".to_owned(),
            Type::PointerWidth => "uintptr_t".to_owned(),
            Type::Status => "FfiStatus".to_owned(),
            Type::Buffer => "FfiBuf_u8".to_owned(),
            Type::String => "FfiString".to_owned(),
            Type::Span => "FfiSpan".to_owned(),
            Type::FutureHandle => "RustFutureHandle".to_owned(),
            Type::StreamPollResult => "StreamPollResult".to_owned(),
            Type::WaitResult => "WaitResult".to_owned(),
            Type::CallbackHandle(_) => "BoltFFICallbackHandle".to_owned(),
            Type::Named(name) | Type::DirectRecord(name) => name.to_string(),
            Type::CStyleEnum { name, .. } => name.to_string(),
            Type::ConstPointer(inner) => format!("const {} *", Self::anonymous(inner)?),
            Type::MutPointer(inner) => format!("{} *", Self::anonymous(inner)?),
            Type::FunctionPointer { returns, params } => {
                Self::function_pointer_declaration("", returns, params.iter())?
                    .to_string()
                    .trim()
                    .to_owned()
            }
        }))
    }

    pub(crate) fn declaration(ty: &Type, name: &str) -> Result<Statement> {
        let name = Identifier::escape(name)?;
        Ok(Statement::new(match ty {
            Type::FunctionPointer { returns, params } => {
                Self::function_pointer_declaration(name.as_str(), returns, params)?.to_string()
            }
            Type::ConstPointer(inner) => {
                format!("const {} *{}", Self::anonymous(inner)?, name)
            }
            Type::MutPointer(inner) => format!("{} *{}", Self::anonymous(inner)?, name),
            _ => format!("{} {}", Self::anonymous(ty)?, name),
        }))
    }

    pub(crate) fn function(ty: &Type, name: &str, params: &str) -> Result<Statement> {
        Ok(Statement::new(format!(
            "{} {name}({params})",
            Self::anonymous(ty)?
        )))
    }

    pub(crate) fn function_pointer_declaration<P>(
        name: &str,
        returns: &Type,
        params: impl IntoIterator<Item = P>,
    ) -> Result<Statement>
    where
        P: Borrow<Type>,
    {
        let params = params
            .into_iter()
            .map(|param| Self::anonymous(param.borrow()))
            .collect::<Result<Vec<_>>>()?;
        let params = match params.is_empty() {
            true => "void".to_owned(),
            false => params
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        };
        Ok(Statement::new(format!(
            "{} (*{name})({params})",
            Self::anonymous(returns)?
        )))
    }
}

impl Statement {
    pub(crate) fn function_declaration(function: &Function) -> Result<Self> {
        let name = Identifier::parse(function.name())?;
        TypeFragment::function(
            function.returns(),
            name.as_str(),
            &Self::named_params(function)?,
        )
    }

    pub(crate) fn function_pointer_typedef(function: &Function, name: &str) -> Result<Self> {
        let name = Identifier::parse(name)?;
        Ok(Statement::new(format!(
            "typedef {}",
            TypeFragment::function_pointer_declaration(
                name.as_str(),
                function.returns(),
                function.params().iter().map(Parameter::ty)
            )?
        )))
    }

    fn named_params(function: &Function) -> Result<String> {
        match function.params().is_empty() {
            true => Ok("void".to_owned()),
            false => function
                .params()
                .iter()
                .map(|parameter| TypeFragment::declaration(parameter.ty(), parameter.name()))
                .collect::<Result<Vec<_>>>()
                .map(|params| {
                    params
                        .into_iter()
                        .map(|param| param.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
        }
    }
}
