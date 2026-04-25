//! C# expression and statement AST. Most code is rendered directly in the templates, but when the
//! plan needs to generate more complicated snippets, it uses this module.

use std::fmt;

use super::{
    CSharpArgumentList, CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpPropertyName,
    CSharpType, CSharpTypeReference,
};

/// A bare C# identifier reference: the `this` keyword, an in-scope
/// local-body name, or a method parameter.
///
/// Examples:
/// ```csharp
/// return this;
/// //     ^^^^
///
/// var x = reader.ReadF64();
/// //      ^^^^^^
///
/// void Use(int myParam) { }
/// //           ^^^^^^^
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CSharpIdentity {
    This,
    Local(CSharpLocalName),
    Param(CSharpParamName),
}

impl fmt::Display for CSharpIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::This => f.write_str("this"),
            Self::Local(name) => name.fmt(f),
            Self::Param(name) => name.fmt(f),
        }
    }
}

impl From<CSharpLocalName> for CSharpIdentity {
    fn from(name: CSharpLocalName) -> Self {
        Self::Local(name)
    }
}

impl From<CSharpParamName> for CSharpIdentity {
    fn from(name: CSharpParamName) -> Self {
        Self::Param(name)
    }
}

/// A C# literal value: a numeric constant or the `null` keyword.
///
/// Examples:
/// ```csharp
/// var count = 16;
/// //          ^^
///
/// string name = null;
/// //            ^^^^
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CSharpLiteral {
    Int(i64),
    Null,
}

impl fmt::Display for CSharpLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Null => f.write_str("null"),
        }
    }
}

/// A C# binary operator token.
///
/// Examples:
/// ```csharp
/// a == b
/// a + b
/// a * b
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CSharpBinaryOp {
    Eq,
    Add,
    Mul,
}

impl fmt::Display for CSharpBinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq => f.write_str("=="),
            Self::Add => f.write_str("+"),
            Self::Mul => f.write_str("*"),
        }
    }
}

/// A C# expression: a fragment of code that evaluates to a value
/// and carries no trailing semicolon.
///
/// Examples:
/// ```csharp
/// this.X
/// reader.ReadF64()
/// (int?)null
/// reader.ReadU8() == 0 ? (int?)null : reader.ReadI32()
/// item => item.Decode()
/// this.Name is { } opt0
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CSharpExpression {
    /// A bare identifier reference, like `this`, a local, or a parameter.
    Identity(CSharpIdentity),
    /// A reference to a named type used as a value (typically as the
    /// receiver of a static call, e.g. `Encoding.UTF8`).
    TypeRef(CSharpTypeReference),
    /// A literal value, like `16` or `null`.
    Literal(CSharpLiteral),
    /// `{receiver}.{name}`: property or field access.
    MemberAccess {
        receiver: Box<CSharpExpression>,
        name: CSharpPropertyName,
    },
    /// `{receiver}.{method}<T, U>({arg0}, {arg1})`: a method invocation,
    /// optionally with type arguments.
    MethodCall {
        receiver: Box<CSharpExpression>,
        method: CSharpMethodName,
        type_args: Vec<CSharpType>,
        args: CSharpArgumentList,
    },
    /// `({target}){inner}`: a C-style cast.
    Cast {
        target: CSharpType,
        inner: Box<CSharpExpression>,
    },
    /// `{left} {op} {right}`: a binary expression.
    Binary {
        op: CSharpBinaryOp,
        left: Box<CSharpExpression>,
        right: Box<CSharpExpression>,
    },
    /// `({inner})`: explicit grouping parentheses. Operator precedence
    /// is not modeled; call sites wrap subtrees here when grouping
    /// matters.
    Paren(Box<CSharpExpression>),
    /// `{cond} ? {then} : {otherwise}`: the conditional expression.
    Ternary {
        cond: Box<CSharpExpression>,
        then: Box<CSharpExpression>,
        otherwise: Box<CSharpExpression>,
    },
    /// `{param} => {body}`: a single-parameter lambda expression.
    Lambda {
        param: CSharpLocalName,
        body: Box<CSharpExpression>,
    },
    /// `{value} is {{ }} {binding}`: the property pattern that tests
    /// for not-null and binds the captured value to a pattern variable.
    IsBindingPattern {
        value: Box<CSharpExpression>,
        binding: CSharpLocalName,
    },
}

impl fmt::Display for CSharpExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity(ident) => ident.fmt(f),
            Self::TypeRef(ty) => ty.fmt(f),
            Self::Literal(lit) => lit.fmt(f),
            Self::MemberAccess { receiver, name } => write!(f, "{receiver}.{name}"),
            Self::MethodCall {
                receiver,
                method,
                type_args,
                args,
            } => {
                write!(f, "{receiver}.{method}")?;
                if !type_args.is_empty() {
                    f.write_str("<")?;
                    for (i, t) in type_args.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{t}")?;
                    }
                    f.write_str(">")?;
                }
                write!(f, "({args})")
            }
            Self::Cast { target, inner } => write!(f, "({target}){inner}"),
            Self::Binary { op, left, right } => write!(f, "{left} {op} {right}"),
            Self::Paren(inner) => write!(f, "({inner})"),
            Self::Ternary {
                cond,
                then,
                otherwise,
            } => write!(f, "{cond} ? {then} : {otherwise}"),
            Self::Lambda { param, body } => write!(f, "{param} => {body}"),
            Self::IsBindingPattern { value, binding } => write!(f, "{value} is {{ }} {binding}"),
        }
    }
}

/// A C# local-variable declaration with an initializer, terminated
/// with a semicolon.
///
/// Examples:
/// ```csharp
/// string name = "Jack";
/// byte[] _vBytes = Encoding.UTF8.GetBytes(v);
/// ```
#[derive(Debug, Clone)]
pub(crate) struct CSharpLocalDecl {
    pub(crate) declared_type: CSharpType,
    pub(crate) name: CSharpLocalName,
    pub(crate) rhs: CSharpExpression,
}

impl fmt::Display for CSharpLocalDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} = {};", self.declared_type, self.name, self.rhs)
    }
}

/// A C# statement: an action that the program executes.
///
/// The leaf [`Self::Expression`] variant renders without a trailing
/// semicolon (the consuming template adds it). Control-flow variants
/// (`If`, `ForEach`) put `;` between statements inside their braces
/// because the closing `}` must follow a completed statement.
///
/// Examples:
/// ```csharp
/// // Expression as statement (no trailing ;)
/// wire.WriteF64(this.X)
///
/// // If
/// if (cond) { body; } else { other; }
///
/// // foreach
/// foreach (string item in items) { wire.WriteString(item); }
/// ```
#[derive(Debug, Clone)]
pub(crate) enum CSharpStatement {
    /// An expression used as a statement (e.g. `wire.WriteF64(this.X)`).
    Expression(CSharpExpression),
    /// `if ({cond}) {{ {then}; }} else {{ {otherwise}; }}`. The
    /// `otherwise` branch is optional.
    If {
        cond: CSharpExpression,
        then: Vec<CSharpStatement>,
        otherwise: Option<Vec<CSharpStatement>>,
    },
    /// `foreach ({elem_type} {var} in {collection}) {{ {body}; }}`.
    ForEach {
        elem_type: CSharpType,
        var: CSharpLocalName,
        collection: CSharpExpression,
        body: Vec<CSharpStatement>,
    },
}

impl fmt::Display for CSharpStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression(expr) => expr.fmt(f),
            Self::If {
                cond,
                then,
                otherwise,
            } => {
                write!(f, "if ({cond}) {{ ")?;
                for stmt in then {
                    write!(f, "{stmt}; ")?;
                }
                f.write_str("}")?;
                if let Some(else_body) = otherwise {
                    f.write_str(" else { ")?;
                    for stmt in else_body {
                        write!(f, "{stmt}; ")?;
                    }
                    f.write_str("}")?;
                }
                Ok(())
            }
            Self::ForEach {
                elem_type,
                var,
                collection,
                body,
            } => {
                write!(f, "foreach ({elem_type} {var} in {collection}) {{ ")?;
                for stmt in body {
                    write!(f, "{stmt}; ")?;
                }
                f.write_str("}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CSharpClassName, CSharpNamespace};
    use super::*;
    use rstest::rstest;

    fn local_for(name: &str) -> CSharpLocalName {
        CSharpLocalName::for_bytes(&CSharpParamName::from_source(name))
    }

    fn int(v: i64) -> CSharpExpression {
        CSharpExpression::Literal(CSharpLiteral::Int(v))
    }

    fn local_ident(name: &str) -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(name)))
    }

    fn type_ref(name: &str) -> CSharpExpression {
        CSharpExpression::TypeRef(CSharpTypeReference::Plain(CSharpClassName::new(name)))
    }

    mod identity {
        use super::*;

        #[test]
        fn this_renders_as_keyword() {
            assert_eq!(CSharpIdentity::This.to_string(), "this");
        }

        #[test]
        fn local_renders_via_wrapped_type() {
            assert_eq!(
                CSharpIdentity::Local(local_for("person")).to_string(),
                "_personBytes"
            );
        }

        #[test]
        fn param_renders_via_wrapped_type() {
            assert_eq!(
                CSharpIdentity::Param(CSharpParamName::from_source("my_param")).to_string(),
                "myParam"
            );
        }

        /// Every param whose transformed form collides with a C# keyword
        /// picks up the `@` escape at `CSharpParamName` construction. The
        /// identity wrapper must pass that escape through without
        /// re-escaping or stripping it.
        #[rstest]
        #[case::class("class", "@class")]
        #[case::new("new", "@new")]
        #[case::string("string", "@string")]
        #[case::interface("interface", "@interface")]
        #[case::foreach("foreach", "@foreach")]
        fn param_preserves_keyword_escape(#[case] source: &str, #[case] expected: &str) {
            let identity = CSharpIdentity::Param(CSharpParamName::from_source(source));
            assert_eq!(identity.to_string(), expected);
        }
    }

    mod literal {
        use super::*;

        #[rstest]
        #[case::zero(0, "0")]
        #[case::positive(16, "16")]
        #[case::negative(-1, "-1")]
        fn int_literal_renders_as_decimal(#[case] value: i64, #[case] expected: &str) {
            assert_eq!(CSharpLiteral::Int(value).to_string(), expected);
        }

        #[test]
        fn null_literal_renders_as_keyword() {
            assert_eq!(CSharpLiteral::Null.to_string(), "null");
        }
    }

    mod binary_op {
        use super::*;

        #[rstest]
        #[case(CSharpBinaryOp::Eq, "==")]
        #[case(CSharpBinaryOp::Add, "+")]
        #[case(CSharpBinaryOp::Mul, "*")]
        fn operator_renders_as_source_token(#[case] op: CSharpBinaryOp, #[case] expected: &str) {
            assert_eq!(op.to_string(), expected);
        }
    }

    mod expression {
        use super::*;

        fn reader() -> CSharpExpression {
            local_ident("reader")
        }

        #[test]
        fn ident_renders_via_ident_display() {
            assert_eq!(reader().to_string(), "reader");
        }

        #[test]
        fn type_ref_renders_plain_class_name() {
            let ty = CSharpTypeReference::Plain(CSharpClassName::from_source("point"));
            let expr = CSharpExpression::TypeRef(ty);
            assert_eq!(expr.to_string(), "Point");
        }

        #[test]
        fn type_ref_renders_qualified_with_global_prefix() {
            let ty = CSharpTypeReference::Qualified {
                namespace: CSharpNamespace::from_source("demo"),
                name: CSharpClassName::from_source("point"),
            };
            assert_eq!(
                CSharpExpression::TypeRef(ty).to_string(),
                "global::Demo.Point"
            );
        }

        #[test]
        fn literal_renders_via_literal_display() {
            assert_eq!(int(16).to_string(), "16");
        }

        #[test]
        fn member_access_renders_dotted() {
            let expr = CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                name: CSharpPropertyName::from_source("x"),
            };
            assert_eq!(expr.to_string(), "this.X");
        }

        /// Member access nests: `Encoding.UTF8` is a `MemberAccess` on
        /// a `TypeRef(Encoding)` receiver, and further members stack on
        /// top.
        #[test]
        fn member_access_chains_through_nested_access() {
            let encoding = CSharpExpression::MemberAccess {
                receiver: Box::new(type_ref("Encoding")),
                name: CSharpPropertyName::from_source("UTF8"),
            };
            assert_eq!(encoding.to_string(), "Encoding.UTF8");
        }

        #[test]
        fn method_call_with_no_type_args_no_args_renders_empty_parens() {
            let expr = CSharpExpression::MethodCall {
                receiver: Box::new(reader()),
                method: CSharpMethodName::from_source("read_f64"),
                type_args: vec![],
                args: CSharpArgumentList::default(),
            };
            assert_eq!(expr.to_string(), "reader.ReadF64()");
        }

        #[test]
        fn method_call_with_args_renders_comma_separated() {
            let expr = CSharpExpression::MethodCall {
                receiver: Box::new(local_ident("wire")),
                method: CSharpMethodName::from_source("write_f64"),
                type_args: vec![],
                args: vec![CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                    name: CSharpPropertyName::from_source("x"),
                }]
                .into(),
            };
            assert_eq!(expr.to_string(), "wire.WriteF64(this.X)");
        }

        #[test]
        fn method_call_with_type_args_renders_angle_brackets() {
            let expr = CSharpExpression::MethodCall {
                receiver: Box::new(reader()),
                method: CSharpMethodName::from_source("read_blittable_array"),
                type_args: vec![CSharpType::Int],
                args: CSharpArgumentList::default(),
            };
            assert_eq!(expr.to_string(), "reader.ReadBlittableArray<int>()");
        }

        /// Two type arguments confirm the comma-separated rendering; the
        /// backend doesn't emit this shape today but the Display is
        /// symmetric with the args case.
        #[test]
        fn method_call_with_multiple_type_args_joins_with_comma_space() {
            let expr = CSharpExpression::MethodCall {
                receiver: Box::new(reader()),
                method: CSharpMethodName::from_source("pair"),
                type_args: vec![CSharpType::Int, CSharpType::Double],
                args: CSharpArgumentList::default(),
            };
            assert_eq!(expr.to_string(), "reader.Pair<int, double>()");
        }

        #[test]
        fn cast_renders_paren_target_then_inner() {
            let expr = CSharpExpression::Cast {
                target: CSharpType::Nullable(Box::new(CSharpType::Int)),
                inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Null)),
            };
            assert_eq!(expr.to_string(), "(int?)null");
        }

        #[test]
        fn binary_renders_with_spaces_around_operator() {
            let expr = CSharpExpression::Binary {
                op: CSharpBinaryOp::Eq,
                left: Box::new(CSharpExpression::MethodCall {
                    receiver: Box::new(reader()),
                    method: CSharpMethodName::from_source("read_u8"),
                    type_args: vec![],
                    args: CSharpArgumentList::default(),
                }),
                right: Box::new(int(0)),
            };
            assert_eq!(expr.to_string(), "reader.ReadU8() == 0");
        }

        #[test]
        fn paren_wraps_inner_in_round_brackets() {
            let expr = CSharpExpression::Paren(Box::new(CSharpExpression::Binary {
                op: CSharpBinaryOp::Add,
                left: Box::new(int(4)),
                right: Box::new(int(8)),
            }));
            assert_eq!(expr.to_string(), "(4 + 8)");
        }

        /// The option-decode ternary composes binary, cast, and method
        /// call. Pinning the full shape here guards the whole subtree
        /// against accidental Display drift.
        #[test]
        fn ternary_option_decode_composes_with_nested_variants() {
            let tag_eq_zero = CSharpExpression::Binary {
                op: CSharpBinaryOp::Eq,
                left: Box::new(CSharpExpression::MethodCall {
                    receiver: Box::new(reader()),
                    method: CSharpMethodName::from_source("read_u8"),
                    type_args: vec![],
                    args: CSharpArgumentList::default(),
                }),
                right: Box::new(int(0)),
            };
            let null_int = CSharpExpression::Cast {
                target: CSharpType::Nullable(Box::new(CSharpType::Int)),
                inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Null)),
            };
            let read_i32 = CSharpExpression::MethodCall {
                receiver: Box::new(reader()),
                method: CSharpMethodName::from_source("read_i32"),
                type_args: vec![],
                args: CSharpArgumentList::default(),
            };
            let expr = CSharpExpression::Ternary {
                cond: Box::new(tag_eq_zero),
                then: Box::new(null_int),
                otherwise: Box::new(read_i32),
            };
            assert_eq!(
                expr.to_string(),
                "reader.ReadU8() == 0 ? (int?)null : reader.ReadI32()"
            );
        }

        #[test]
        fn lambda_renders_fat_arrow_between_param_and_body() {
            let r0 = CSharpLocalName::for_bytes(&CSharpParamName::from_source("r0"));
            let expr = CSharpExpression::Lambda {
                param: r0.clone(),
                body: Box::new(CSharpExpression::MethodCall {
                    receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::Local(r0))),
                    method: CSharpMethodName::from_source("read_i32"),
                    type_args: vec![],
                    args: CSharpArgumentList::default(),
                }),
            };
            assert_eq!(expr.to_string(), "_r0Bytes => _r0Bytes.ReadI32()");
        }

        #[test]
        fn is_binding_pattern_renders_captured_binding() {
            let expr = CSharpExpression::IsBindingPattern {
                value: Box::new(CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                    name: CSharpPropertyName::from_source("name"),
                }),
                binding: local_for("opt"),
            };
            assert_eq!(expr.to_string(), "this.Name is { } _optBytes");
        }
    }

    mod statement {
        use super::*;

        /// `wire.{method}({arg})` as a statement: a small builder so the
        /// `If` / `ForEach` / `Sequence` tests below can describe their
        /// bodies without spelling out the full `MethodCall` shape every
        /// time.
        fn wire_call_stmt(method: &str, arg: CSharpExpression) -> CSharpStatement {
            CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(local_ident("wire")),
                method: CSharpMethodName::from_source(method),
                type_args: vec![],
                args: vec![arg].into(),
            })
        }

        fn this_member(name: &str) -> CSharpExpression {
            CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                name: CSharpPropertyName::from_source(name),
            }
        }

        fn cast_byte(n: i64) -> CSharpExpression {
            CSharpExpression::Cast {
                target: CSharpType::Byte,
                inner: Box::new(int(n)),
            }
        }

        /// Expression-as-statement keeps the expression's Display
        /// verbatim; the template adds the trailing `;`.
        #[test]
        fn expression_statement_renders_expression_alone() {
            let stmt = wire_call_stmt("write_f64", this_member("x"));
            assert_eq!(stmt.to_string(), "wire.WriteF64(this.X)");
        }

        #[test]
        fn local_decl_includes_trailing_semicolon() {
            let utf8 = CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                    CSharpClassName::new("Encoding"),
                ))),
                name: CSharpPropertyName::from_source("UTF8"),
            };
            let get_bytes = CSharpExpression::MethodCall {
                receiver: Box::new(utf8),
                method: CSharpMethodName::new("GetBytes"),
                type_args: vec![],
                args: vec![CSharpExpression::Identity(CSharpIdentity::Param(
                    CSharpParamName::from_source("v"),
                ))]
                .into(),
            };
            let decl = CSharpLocalDecl {
                declared_type: CSharpType::Array(Box::new(CSharpType::Byte)),
                name: local_for("v"),
                rhs: get_bytes,
            };
            assert_eq!(
                decl.to_string(),
                "byte[] _vBytes = Encoding.UTF8.GetBytes(v);"
            );
        }

        /// The `If` body renders each inner statement followed by
        /// `"; "` and ends with a bare `}`.
        #[test]
        fn if_with_two_then_stmts_and_single_else_stmt_matches_brace_spacing() {
            let opt0 = CSharpLocalName::new("opt0");
            let stmt = CSharpStatement::If {
                cond: CSharpExpression::IsBindingPattern {
                    value: Box::new(this_member("name")),
                    binding: opt0.clone(),
                },
                then: vec![
                    wire_call_stmt("write_u8", cast_byte(1)),
                    wire_call_stmt(
                        "write_string",
                        CSharpExpression::Identity(CSharpIdentity::Local(opt0)),
                    ),
                ],
                otherwise: Some(vec![wire_call_stmt("write_u8", cast_byte(0))]),
            };
            assert_eq!(
                stmt.to_string(),
                "if (this.Name is { } opt0) { wire.WriteU8((byte)1); wire.WriteString(opt0); } else { wire.WriteU8((byte)0); }"
            );
        }

        #[test]
        fn if_without_else_omits_else_clause() {
            let stmt = CSharpStatement::If {
                cond: local_ident("guard"),
                then: vec![CSharpStatement::Expression(local_ident("body"))],
                otherwise: None,
            };
            assert_eq!(stmt.to_string(), "if (guard) { body; }");
        }

        #[test]
        fn foreach_renders_header_and_body_brace_block() {
            let v_names = CSharpExpression::MemberAccess {
                receiver: Box::new(local_ident("_v")),
                name: CSharpPropertyName::from_source("names"),
            };
            let name_bytes = CSharpExpression::Identity(CSharpIdentity::Local(local_for("name")));
            let stmt = CSharpStatement::ForEach {
                elem_type: CSharpType::String,
                var: local_for("name"),
                collection: v_names,
                body: vec![wire_call_stmt("write_string", name_bytes)],
            };
            assert_eq!(
                stmt.to_string(),
                "foreach (string _nameBytes in _v.Names) { wire.WriteString(_nameBytes); }"
            );
        }
    }
}
