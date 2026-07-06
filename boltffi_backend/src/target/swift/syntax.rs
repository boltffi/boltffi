use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// Swift syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier {
    spelling: String,
    escaped: bool,
}

/// Swift type syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(String);

/// Swift expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// Swift statement syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// Swift literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ParameterList(Vec<String>);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArgumentLabel(String);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
struct CommaList(Vec<String>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "associatedtype",
        "borrowing",
        "class",
        "consuming",
        "deinit",
        "enum",
        "extension",
        "fileprivate",
        "func",
        "import",
        "init",
        "inout",
        "internal",
        "let",
        "nonisolated",
        "open",
        "operator",
        "precedencegroup",
        "private",
        "protocol",
        "public",
        "rethrows",
        "static",
        "struct",
        "subscript",
        "typealias",
        "var",
        "break",
        "case",
        "catch",
        "continue",
        "default",
        "defer",
        "do",
        "else",
        "fallthrough",
        "for",
        "guard",
        "if",
        "in",
        "repeat",
        "return",
        "switch",
        "throw",
        "where",
        "while",
        "Any",
        "as",
        "await",
        "false",
        "is",
        "nil",
        "self",
        "Self",
        "super",
        "throws",
        "true",
        "try",
        "_",
    ];

    type Identifier = Identifier;
    type Type = TypeName;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.escaped {
            true => write!(formatter, "`{}`", self.spelling),
            false => formatter.write_str(&self.spelling),
        }
    }
}

impl Identifier {
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            Ok(Self {
                spelling: identifier,
                escaped: false,
            })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier })
        }
    }

    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) {
            Ok(Self {
                escaped: Syntax::keyword(&identifier),
                spelling: identifier,
            })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.spelling
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl fmt::Display for ArgumentLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ArgumentLabel {
    fn from(label: &str) -> Self {
        Self(label.to_owned())
    }
}

impl From<String> for ArgumentLabel {
    fn from(label: String) -> Self {
        Self(label)
    }
}

impl From<&String> for ArgumentLabel {
    fn from(label: &String) -> Self {
        Self(label.clone())
    }
}

impl From<Identifier> for ArgumentLabel {
    fn from(identifier: Identifier) -> Self {
        Self(identifier.spelling)
    }
}

impl From<&Identifier> for ArgumentLabel {
    fn from(identifier: &Identifier) -> Self {
        Self(identifier.spelling.clone())
    }
}

impl sealed::SyntaxFragment for TypeName {}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TypeName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn void() -> Self {
        Self::new("Void")
    }

    pub fn bool() -> Self {
        Self::new("Bool")
    }

    pub fn string() -> Self {
        Self::new("String")
    }

    pub fn data() -> Self {
        Self::new("Data")
    }

    pub fn int8() -> Self {
        Self::new("Int8")
    }

    pub fn uint8() -> Self {
        Self::new("UInt8")
    }

    pub fn int16() -> Self {
        Self::new("Int16")
    }

    pub fn uint16() -> Self {
        Self::new("UInt16")
    }

    pub fn int32() -> Self {
        Self::new("Int32")
    }

    pub fn uint32() -> Self {
        Self::new("UInt32")
    }

    pub fn int64() -> Self {
        Self::new("Int64")
    }

    pub fn uint64() -> Self {
        Self::new("UInt64")
    }

    pub fn int() -> Self {
        Self::new("Int")
    }

    pub fn uint() -> Self {
        Self::new("UInt")
    }

    pub fn float() -> Self {
        Self::new("Float")
    }

    pub fn double() -> Self {
        Self::new("Double")
    }

    pub fn array(element: Self) -> Self {
        Self::new(format!("[{element}]"))
    }

    pub fn optional(self) -> Self {
        Self::new(format!("{self}?"))
    }

    pub fn optional_function_pointer(self) -> Self {
        Self::new(format!("({self})?"))
    }

    pub fn escaping(self) -> Self {
        Self::new(format!("@escaping {self}"))
    }

    pub fn result(ok: Self, err: Self) -> Self {
        Self::new(format!("Swift.Result<{ok}, {err}>"))
    }

    pub fn tuple(elements: impl IntoIterator<Item = Self>) -> Self {
        let elements = elements
            .into_iter()
            .map(|element| element.to_string())
            .collect::<Vec<_>>();
        match elements.as_slice() {
            [element] => Self::new(element.clone()),
            _ => Self::new(format!("({})", elements.join(", "))),
        }
    }

    pub fn dictionary(key: Self, value: Self) -> Self {
        Self::new(format!("[{key}: {value}]"))
    }

    pub fn closure(parameters: impl IntoIterator<Item = Self>, returns: Option<Self>) -> Self {
        Self::closure_effect(parameters, returns, false)
    }

    pub fn throwing_closure(
        parameters: impl IntoIterator<Item = Self>,
        returns: Option<Self>,
    ) -> Self {
        Self::closure_effect(parameters, returns, true)
    }

    pub fn mutable_pointer(self) -> Self {
        Self::new(format!("UnsafeMutablePointer<{self}>?"))
    }

    pub fn pointer(self) -> Self {
        Self::new(format!("UnsafePointer<{self}>?"))
    }

    fn closure_effect(
        parameters: impl IntoIterator<Item = Self>,
        returns: Option<Self>,
        throwing: bool,
    ) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let returns = returns.unwrap_or_else(Self::void);
        let effect = if throwing { " throws" } else { "" };
        Self::new(format!("({parameters}){effect} -> {returns}"))
    }

    pub fn metatype(self) -> Self {
        Self::new(format!("{self}.self"))
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    pub fn new(expression: impl Into<String>) -> Self {
        Self(expression.into())
    }

    pub fn identifier(identifier: Identifier) -> Self {
        Self::new(identifier.to_string())
    }

    pub fn literal(literal: Literal) -> Self {
        Self::new(literal.to_string())
    }

    pub fn member(base: impl fmt::Display, member: impl fmt::Display) -> Self {
        Self::new(format!("{base}.{member}"))
    }

    pub fn optional_member(base: impl fmt::Display, member: impl fmt::Display) -> Self {
        Self::new(format!("{base}.{member}?"))
    }

    pub fn optional_chain_member(base: impl fmt::Display, member: impl fmt::Display) -> Self {
        Self::new(format!("{base}?.{member}"))
    }

    pub fn call(callee: impl fmt::Display, arguments: ArgumentList) -> Self {
        Self::new(format!("{callee}({arguments})"))
    }

    pub fn call_with_layout(
        callee: impl fmt::Display,
        arguments: ArgumentList,
        argument_indent: &str,
        closing_indent: &str,
    ) -> Self {
        Self::new(format!(
            "{callee}({})",
            arguments.render(argument_indent, closing_indent)
        ))
    }

    pub fn optional_call(callee: impl fmt::Display, arguments: ArgumentList) -> Self {
        Self::new(format!("{callee}?({arguments})"))
    }

    pub fn labeled(label: impl Into<ArgumentLabel>, value: impl fmt::Display) -> Self {
        let label = label.into();
        Self::new(format!("{label}: {value}"))
    }

    pub fn forced(expression: impl fmt::Display) -> Self {
        Self::new(format!("{expression}!"))
    }

    pub fn trying(expression: impl fmt::Display) -> Self {
        Self::new(format!("try {expression}"))
    }

    pub fn awaiting(expression: impl fmt::Display) -> Self {
        Self::new(format!("await {expression}"))
    }

    pub fn address(expression: impl fmt::Display) -> Self {
        Self::new(format!("&{expression}"))
    }

    pub fn nil() -> Self {
        Self::new("nil")
    }

    pub fn not_equal(left: impl fmt::Display, right: impl fmt::Display) -> Self {
        Self::new(format!("{left} != {right}"))
    }

    pub fn equal(left: impl fmt::Display, right: impl fmt::Display) -> Self {
        Self::new(format!("{left} == {right}"))
    }

    pub fn or(left: impl fmt::Display, right: impl fmt::Display) -> Self {
        Self::new(format!("{left} || {right}"))
    }

    pub fn nil_coalescing(left: impl fmt::Display, right: impl fmt::Display) -> Self {
        Self::new(format!("{left} ?? {right}"))
    }

    pub fn conditional(
        condition: impl fmt::Display,
        success: impl fmt::Display,
        failure: impl fmt::Display,
    ) -> Self {
        Self::new(format!("{condition} ? {success} : {failure}"))
    }

    pub fn tuple(elements: impl IntoIterator<Item = Self>) -> Self {
        let elements = elements
            .into_iter()
            .map(|element| element.to_string())
            .collect::<Vec<_>>();
        match elements.as_slice() {
            [element] => Self::new(element.clone()),
            _ => Self::new(format!("({})", elements.join(", "))),
        }
    }

    pub fn range_until(end: impl fmt::Display) -> Self {
        Self::new(format!("(0..<{end})"))
    }

    pub fn subscript(base: impl fmt::Display, index: impl fmt::Display) -> Self {
        Self::new(format!("{base}[{index}]"))
    }

    pub fn trailing_closure(
        callee: impl fmt::Display,
        arguments: ArgumentList,
        parameter: impl fmt::Display,
        expression: impl fmt::Display,
    ) -> Self {
        match arguments.is_empty() {
            true => Self::new(format!("{callee} {{ {parameter} in {expression} }}")),
            false => Self::new(format!(
                "{callee}({arguments}) {{ {parameter} in {expression} }}"
            )),
        }
    }

    pub fn trailing_closure_parameters(
        callee: impl fmt::Display,
        arguments: ArgumentList,
        parameters: impl IntoIterator<Item = Identifier>,
        expression: impl fmt::Display,
    ) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        match arguments.is_empty() {
            true => Self::new(format!("{callee} {{ {parameters} in {expression} }}")),
            false => Self::new(format!(
                "{callee}({arguments}) {{ {parameters} in {expression} }}"
            )),
        }
    }

    pub fn trailing_closure_statements(
        callee: impl fmt::Display,
        arguments: ArgumentList,
        parameter: impl fmt::Display,
        statements: impl IntoIterator<Item = Statement>,
    ) -> Self {
        let statements = statements
            .into_iter()
            .map(|statement| statement.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        Self::trailing_closure(callee, arguments, parameter, statements)
    }

    pub fn closure(
        parameters: impl IntoIterator<Item = Identifier>,
        expression: impl fmt::Display,
    ) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self::new(format!("{{ {parameters} in {expression} }}"))
    }

    pub fn map(
        collection: impl fmt::Display,
        parameter: Identifier,
        expression: impl fmt::Display,
    ) -> Self {
        Self::new(format!(
            "{collection}.map {{ {parameter} in {expression} }}"
        ))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    pub fn new(statement: impl Into<String>) -> Self {
        Self(statement.into())
    }

    pub fn expression(expression: impl fmt::Display) -> Self {
        Self::new(expression.to_string())
    }

    pub fn let_value(identifier: impl fmt::Display, expression: impl fmt::Display) -> Self {
        Self::new(format!("let {identifier} = {expression}"))
    }

    pub fn try_let_value(identifier: impl fmt::Display, expression: impl fmt::Display) -> Self {
        Self::new(format!("let {identifier} = try {expression}"))
    }

    pub fn var_value(
        identifier: impl fmt::Display,
        ty: impl fmt::Display,
        expression: impl fmt::Display,
    ) -> Self {
        Self::new(format!("var {identifier}: {ty} = {expression}"))
    }

    pub fn var_nil(identifier: impl fmt::Display, ty: impl fmt::Display) -> Self {
        Self::new(format!("var {identifier}: {ty} = nil"))
    }

    pub fn stored_var(identifier: impl fmt::Display, ty: impl fmt::Display) -> Self {
        Self::new(format!("var {identifier}: {ty}"))
    }

    pub fn assign(target: impl fmt::Display, expression: impl fmt::Display) -> Self {
        Self::new(format!("{target} = {expression}"))
    }

    pub fn discard(expression: impl fmt::Display) -> Self {
        Self::new(format!("_ = {expression}"))
    }

    pub fn returns(expression: impl fmt::Display) -> Self {
        Self::new(format!("return {expression}"))
    }

    pub fn try_returns(expression: impl fmt::Display) -> Self {
        Self::new(format!("return try {expression}"))
    }

    pub fn discarding_unsafe_buffer_scope(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        throwing: bool,
    ) -> String {
        let prefix = if throwing { "try " } else { "" };
        Self::unsafe_buffer_scope_with_effect(bytes, buffer, body, indent, prefix)
    }

    pub fn unsafe_buffer_scope(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
    ) -> String {
        Self::unsafe_buffer_scope_with_prefix(bytes, buffer, body, indent, "")
    }

    pub fn returning_unsafe_buffer_scope(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        throwing: bool,
    ) -> String {
        let prefix = if throwing { "return try " } else { "return " };
        Self::unsafe_buffer_scope_with_prefix(bytes, buffer, body, indent, prefix)
    }

    pub fn binding_unsafe_buffer_scope(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        binding: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        throwing: bool,
    ) -> String {
        let effect = if throwing { "try " } else { "" };
        Self::unsafe_buffer_scope_with_prefix(
            bytes,
            buffer,
            body,
            indent,
            &format!("let {binding} = {effect}"),
        )
    }

    pub fn discarding_trailing_closure_scope(
        callee: impl fmt::Display,
        parameter: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
    ) -> String {
        Self::trailing_closure_scope_with_prefix(callee, parameter, body, indent, "_ = ")
    }

    pub fn trailing_closure_scope(
        callee: impl fmt::Display,
        parameter: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
    ) -> String {
        Self::trailing_closure_scope_with_prefix(callee, parameter, body, indent, "")
    }

    pub fn returning_trailing_closure_scope(
        callee: impl fmt::Display,
        parameter: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        throwing: bool,
    ) -> String {
        let prefix = if throwing { "return try " } else { "return " };
        Self::trailing_closure_scope_with_prefix(callee, parameter, body, indent, prefix)
    }

    pub fn binding_trailing_closure_scope(
        callee: impl fmt::Display,
        parameter: impl fmt::Display,
        binding: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        throwing: bool,
    ) -> String {
        let effect = if throwing { "try " } else { "" };
        Self::trailing_closure_scope_with_prefix(
            callee,
            parameter,
            body,
            indent,
            &format!("let {binding} = {effect}"),
        )
    }

    pub fn try_await_returning_trailing_closure(
        callee: impl fmt::Display,
        arguments: ArgumentList,
        parameters: impl IntoIterator<Item = Identifier>,
        body: impl fmt::Display,
        indent: &str,
    ) -> String {
        let argument_indent = format!("{indent}    ");
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{indent}return try await {callee}(\n{}\n{indent}) {{ {parameters} in\n{body}\n{indent}}}",
            arguments.indented(&argument_indent)
        )
    }

    pub fn structure(name: impl fmt::Display, members: impl IntoIterator<Item = Self>) -> Self {
        Self::block(format!("struct {name}"), members)
    }

    pub fn final_class(name: impl fmt::Display, members: impl IntoIterator<Item = Self>) -> Self {
        Self::block(format!("final class {name}"), members)
    }

    pub fn initializer(
        parameter: impl fmt::Display,
        ty: impl fmt::Display,
        body: impl IntoIterator<Item = Self>,
    ) -> Self {
        Self::block(format!("init(_ {parameter}: {ty})"), body)
    }

    pub fn deinitializer(body: impl IntoIterator<Item = Self>) -> Self {
        Self::block("deinit", body)
    }

    pub fn guard_else(condition: impl fmt::Display, body: impl IntoIterator<Item = Self>) -> Self {
        Self::block(format!("guard {condition} else"), body)
    }

    pub fn guard_let(
        binding: impl fmt::Display,
        expression: impl fmt::Display,
        body: impl IntoIterator<Item = Self>,
    ) -> Self {
        Self::block(format!("guard let {binding} = {expression} else"), body)
    }

    pub fn throwing(expression: impl fmt::Display) -> Self {
        Self::new(format!("throw {expression}"))
    }

    pub fn fatal_error(message: Literal) -> Self {
        Self::expression(Expression::call(
            "fatalError",
            [Expression::literal(message)].into_iter().collect(),
        ))
    }

    pub fn defer(expression: impl fmt::Display) -> Self {
        Self::new(format!("defer {{ {expression} }}"))
    }

    pub fn if_then(condition: impl fmt::Display, body: impl IntoIterator<Item = Self>) -> Self {
        Self::block(format!("if {condition}"), body)
    }

    pub fn returning_closure(
        parameters: impl IntoIterator<Item = Identifier>,
        body: impl fmt::Display,
        indent: &str,
    ) -> String {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let header = match parameters.is_empty() {
            true => format!("{indent}return {{"),
            false => format!("{indent}return {{ {parameters} in"),
        };
        format!("{header}\n{body}\n{indent}}}")
    }

    pub fn indented(&self, indent: &str) -> String {
        self.0
            .lines()
            .map(|line| format!("{indent}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn unsafe_buffer_scope_with_prefix(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        prefix: &str,
    ) -> String {
        Self::trailing_closure_scope_with_prefix(
            Expression::member(bytes, "withUnsafeBufferPointer"),
            buffer,
            body,
            indent,
            prefix,
        )
    }

    fn unsafe_buffer_scope_with_effect(
        bytes: impl fmt::Display,
        buffer: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        prefix: &str,
    ) -> String {
        format!(
            "{indent}{prefix}{} {{ {buffer} -> Void in\n{body}\n{indent}}}",
            Expression::member(bytes, "withUnsafeBufferPointer")
        )
    }

    fn trailing_closure_scope_with_prefix(
        callee: impl fmt::Display,
        parameter: impl fmt::Display,
        body: impl fmt::Display,
        indent: &str,
        prefix: &str,
    ) -> String {
        format!("{indent}{prefix}{callee} {{ {parameter} in\n{body}\n{indent}}}")
    }

    fn block(header: impl fmt::Display, body: impl IntoIterator<Item = Self>) -> Self {
        let body = body
            .into_iter()
            .map(|statement| statement.indented("    "))
            .collect::<Vec<_>>()
            .join("\n");
        Self::new(format!("{header} {{\n{body}\n}}"))
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Literal {
    pub fn new(literal: impl Into<String>) -> Self {
        Self(literal.into())
    }

    pub fn bool(value: bool) -> Self {
        Self::new(match value {
            true => "true",
            false => "false",
        })
    }

    pub fn integer(value: i128) -> Self {
        Self::new(value.to_string())
    }

    pub fn float32(value: f32) -> Self {
        Self::new(value.to_string())
    }

    pub fn float64(value: f64) -> Self {
        Self::new(value.to_string())
    }

    pub fn string(value: &str) -> Self {
        let escaped = Self::escaped_string_content(value);
        Self::new(format!("\"{escaped}\""))
    }

    pub fn interpolated(prefix: &str, expression: impl fmt::Display, suffix: &str) -> Self {
        let escaped_prefix = Self::escaped_string_content(prefix);
        let escaped_suffix = Self::escaped_string_content(suffix);
        Self::new(format!(
            "\"{escaped_prefix}\\({expression}){escaped_suffix}\""
        ))
    }

    pub fn nil() -> Self {
        Self::new("nil")
    }

    fn escaped_string_content(value: &str) -> String {
        value
            .chars()
            .flat_map(|character| match character {
                '"' => "\\\"".chars().collect::<Vec<_>>(),
                '\\' => "\\\\".chars().collect(),
                '\n' => "\\n".chars().collect(),
                '\r' => "\\r".chars().collect(),
                '\t' => "\\t".chars().collect(),
                character if character.is_control() => {
                    format!("\\u{{{:X}}}", character as u32).chars().collect()
                }
                character => vec![character],
            })
            .collect()
    }
}

impl sealed::SyntaxFragment for ArgumentList {}

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

impl ArgumentList {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn render(&self, argument_indent: &str, closing_indent: &str) -> String {
        CommaList::new(self.0.iter()).render(argument_indent, closing_indent)
    }

    pub fn indented(&self, indent: &str) -> String {
        let last = self.0.len().saturating_sub(1);
        self.0
            .iter()
            .enumerate()
            .map(|(index, argument)| match index == last {
                true => format!("{indent}{argument}"),
                false => format!("{indent}{argument},"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl FromIterator<Expression> for ArgumentList {
    fn from_iter<T: IntoIterator<Item = Expression>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl ParameterList {
    pub fn new(parameters: impl IntoIterator<Item = impl fmt::Display>) -> Self {
        Self(
            parameters
                .into_iter()
                .map(|parameter| parameter.to_string())
                .collect(),
        )
    }

    pub fn render(&self, parameter_indent: &str, closing_indent: &str) -> String {
        CommaList::new(self.0.iter()).render(parameter_indent, closing_indent)
    }
}

impl CommaList {
    fn new(items: impl IntoIterator<Item = impl fmt::Display>) -> Self {
        Self(items.into_iter().map(|item| item.to_string()).collect())
    }

    fn render(&self, item_indent: &str, closing_indent: &str) -> String {
        match self.0.len() > 3 {
            true => format!("\n{}\n{closing_indent}", self.render_multiline(item_indent)),
            false => self.0.join(", "),
        }
    }

    fn render_multiline(&self, item_indent: &str) -> String {
        self.0
            .iter()
            .enumerate()
            .map(|(index, item)| match index + 1 == self.0.len() {
                true => format!("{item_indent}{item}"),
                false => format!("{item_indent}{item},"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
