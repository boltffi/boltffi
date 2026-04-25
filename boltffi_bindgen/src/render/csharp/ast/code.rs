//! Structured C# expression and statement AST. Each variant carries
//! just enough structure to render its target C# form via
//! [`fmt::Display`]; the lowerer builds these trees directly so
//! templates no longer receive pre-rendered snippets.
//!
//! Migration note: `CSharpExpression::Raw` and `CSharpStatement::Raw`
//! are shim variants that passthrough a pre-rendered source string.
//! They bridge the period where some emit helpers (read / write / size)
//! still produce strings; they will be deleted once those helpers are
//! ported to build AST directly.
//!
//! Control-flow statement variants (`If`, `ForEach`) will join
//! [`CSharpStatement`] when the write-expression lowering substep adds
//! the first AST-level call sites that need them. Leaving them out of
//! the initial shape keeps 10a's diff focused.

use std::fmt;

use super::{
    CSharpLocalName, CSharpMethodName, CSharpParamName, CSharpPropertyName, CSharpType,
    CSharpTypeReference,
};

/// A bare identifier reference: the leaf of every expression tree
/// that doesn't eventually resolve through a call or member access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpIdent {
    /// C# `this` keyword, used as a receiver in record wire-encode
    /// statements (`this.X.WireEncodeTo(wire)`).
    This,
    /// A local variable: synthesized by the lowerer (`_v`, `item0`,
    /// `opt0`, `r0`) or a fixed-vocabulary local from the surrounding
    /// generated method (`reader`, `wire`).
    Local(CSharpLocalName),
    /// A user parameter (`v`, `count`, `@class`).
    Param(CSharpParamName),
}

impl fmt::Display for CSharpIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::This => f.write_str("this"),
            Self::Local(name) => name.fmt(f),
            Self::Param(name) => name.fmt(f),
        }
    }
}

impl From<CSharpLocalName> for CSharpIdent {
    fn from(name: CSharpLocalName) -> Self {
        Self::Local(name)
    }
}

impl From<CSharpParamName> for CSharpIdent {
    fn from(name: CSharpParamName) -> Self {
        Self::Param(name)
    }
}

/// A C# literal value. Only the forms the backend actually emits are
/// modeled; grow the enum as new forms arrive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpLiteral {
    Int(i64),
    Null,
    Bool(bool),
}

impl fmt::Display for CSharpLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Null => f.write_str("null"),
            Self::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// Binary operators used by the backend's generated expressions. The
/// enum grows as new forms arrive; today only the three the size /
/// option / tag paths emit are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpBinaryOp {
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

/// A C# expression. Evaluates to a value, no trailing semicolon.
///
/// The structure captures enough of C#'s expression grammar to serve
/// every shape the backend emits today (identifier leaves, member
/// access, method calls, casts, binary ops, grouping parens, ternary,
/// lambdas, and the `is {{ }} x` pattern). Precedence is not modeled;
/// call sites add [`Self::Paren`] explicitly when grouping matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpExpression {
    /// A bare identifier leaf: `this`, `reader`, `v`, `item0`,
    /// `Encoding`.
    Ident(CSharpIdent),
    /// A reference to a named type, used as a static-call receiver or
    /// type anchor: `Point`, `StatusWire`, `global::Demo.Point`.
    TypeRef(CSharpTypeReference),
    /// `16`, `null`, `true`.
    Literal(CSharpLiteral),
    /// `{receiver}.{name}`: property, field, or nested-type access.
    /// Used for `this.X`, `v.Length`, `Encoding.UTF8`, `_v.Origin.X`.
    MemberAccess {
        receiver: Box<CSharpExpression>,
        name: CSharpPropertyName,
    },
    /// `{receiver}.{method}<T, U>({arg0}, {arg1})`. `type_args` and
    /// `args` may both be empty.
    MethodCall {
        receiver: Box<CSharpExpression>,
        method: CSharpMethodName,
        type_args: Vec<CSharpType>,
        args: Vec<CSharpExpression>,
    },
    /// `({target}){inner}`: C-style cast, used for `(int?)null` in
    /// option decode paths and `(byte)0` / `(byte)1` in option tag
    /// writes.
    Cast {
        target: CSharpType,
        inner: Box<CSharpExpression>,
    },
    /// `{left} {op} {right}` with a single space on each side of the
    /// operator.
    Binary {
        op: CSharpBinaryOp,
        left: Box<CSharpExpression>,
        right: Box<CSharpExpression>,
    },
    /// `({inner})`: explicit grouping parens. Precedence is not
    /// otherwise modeled, so ambiguous groupings (`Sum` contributions
    /// threaded into an outer multiply-by-element-count) must wrap
    /// themselves at construction.
    Paren(Box<CSharpExpression>),
    /// `{cond} ? {then} : {otherwise}`.
    Ternary {
        cond: Box<CSharpExpression>,
        then: Box<CSharpExpression>,
        otherwise: Box<CSharpExpression>,
    },
    /// `{param} => {body}`: single-parameter lambda. All call sites
    /// today (loop body, encode closure, decode closure) fit the
    /// single-parameter shape.
    Lambda {
        param: CSharpLocalName,
        body: Box<CSharpExpression>,
    },
    /// `{value} is {{ }} {binding}`: C# property pattern that tests
    /// for not-null and captures into `binding`. Evaluates to `bool`
    /// and is used as the condition of a size-expression ternary and
    /// of a write `if` statement.
    IsBindingPattern {
        value: Box<CSharpExpression>,
        binding: CSharpLocalName,
    },
    /// Migration shim: a pre-rendered source string passed through
    /// verbatim. Used by the read / write / size emitters while they
    /// still produce strings; deleted once all of them build AST.
    Raw(String),
}

impl fmt::Display for CSharpExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ident(ident) => ident.fmt(f),
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
                f.write_str("(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
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
            Self::Raw(s) => f.write_str(s),
        }
    }
}

/// A C# local variable declaration with initializer, rendered as
/// `{declared_type} {name} = {rhs};`. Structural: each piece is
/// typed. The trailing semicolon is part of the Display, matching the
/// template sites that interpolate a declaration expecting no extra
/// punctuation.
#[derive(Debug, Clone)]
pub struct CSharpLocalDecl {
    pub declared_type: CSharpType,
    pub name: CSharpLocalName,
    pub rhs: CSharpExpression,
}

impl fmt::Display for CSharpLocalDecl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} = {};", self.declared_type, self.name, self.rhs)
    }
}

/// A C# statement. An effect produced by executing its contents.
///
/// The Display convention is that a statement renders its text
/// *without* a trailing semicolon on the leaf paths (`Raw`,
/// `Expression`), because today's templates interpolate these fields
/// and add their own `;`. [`CSharpLocalDecl`] keeps its existing
/// self-punctuating form because its templates rely on it. Control-
/// flow variants (`If`, `ForEach`) terminate each inner statement
/// inside their body braces with `; ` because the closing `}` must
/// follow a completed statement, not a partial expression.
#[derive(Debug, Clone)]
pub enum CSharpStatement {
    /// Migration shim matching [`CSharpExpression::Raw`].
    Raw(String),
    /// An expression used as a statement (`wire.WriteF64(this.X)`).
    /// Displayed without a trailing semicolon; the consuming template
    /// adds it.
    Expression(CSharpExpression),
    /// A typed local declaration (`byte[] _vBytes = Encoding.UTF8.GetBytes(v);`).
    LocalDecl(CSharpLocalDecl),
    /// `if ({cond}) {{ {then}; }} else {{ {otherwise}; }}`. The
    /// `otherwise` branch is optional: when absent, no `else` block
    /// is rendered.
    If {
        cond: CSharpExpression,
        then: Vec<CSharpStatement>,
        otherwise: Option<Vec<CSharpStatement>>,
    },
    /// `foreach ({elem_type} {var} in {collection}) {{ {body}; }}`.
    /// Used by the write path's encoded-vec branch where each element
    /// is serialized by a nested write sequence.
    ForEach {
        elem_type: CSharpType,
        var: CSharpLocalName,
        collection: CSharpExpression,
        body: Vec<CSharpStatement>,
    },
    /// Multiple statements joined by `; ` when one write op expands
    /// to several top-level statements (today only the encoded-vec
    /// shape: `WriteI32(length)` followed by `foreach`). Rendered
    /// without a trailing `;` so the consuming template still adds
    /// its own, same as the other leaf variants.
    Sequence(Vec<CSharpStatement>),
}

impl fmt::Display for CSharpStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(s) => f.write_str(s),
            Self::Expression(expr) => expr.fmt(f),
            Self::LocalDecl(decl) => decl.fmt(f),
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
            Self::Sequence(stmts) => {
                for (i, stmt) in stmts.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "{stmt}")?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{CSharpClassName, CSharpNamespace};
    use rstest::rstest;

    fn local_for(name: &str) -> CSharpLocalName {
        CSharpLocalName::for_bytes(&CSharpParamName::from_source(name))
    }

    fn int(v: i64) -> CSharpExpression {
        CSharpExpression::Literal(CSharpLiteral::Int(v))
    }

    fn local_ident(name: &str) -> CSharpExpression {
        CSharpExpression::Ident(CSharpIdent::Local(CSharpLocalName::new(name)))
    }

    fn type_ref(name: &str) -> CSharpExpression {
        CSharpExpression::TypeRef(CSharpTypeReference::Plain(CSharpClassName::new(name)))
    }

    mod ident {
        use super::*;

        #[test]
        fn this_renders_as_keyword() {
            assert_eq!(CSharpIdent::This.to_string(), "this");
        }

        #[test]
        fn local_renders_via_wrapped_type() {
            assert_eq!(
                CSharpIdent::Local(local_for("person")).to_string(),
                "_personBytes"
            );
        }

        #[test]
        fn param_renders_via_wrapped_type() {
            assert_eq!(
                CSharpIdent::Param(CSharpParamName::from_source("my_param")).to_string(),
                "myParam"
            );
        }

        /// Every param whose transformed form collides with a C# keyword
        /// picks up the `@` escape at `CSharpParamName` construction. The
        /// ident wrapper must pass that escape through without
        /// re-escaping or stripping it.
        #[rstest]
        #[case::class("class", "@class")]
        #[case::new("new", "@new")]
        #[case::string("string", "@string")]
        #[case::interface("interface", "@interface")]
        #[case::foreach("foreach", "@foreach")]
        fn param_preserves_keyword_escape(#[case] source: &str, #[case] expected: &str) {
            let ident = CSharpIdent::Param(CSharpParamName::from_source(source));
            assert_eq!(ident.to_string(), expected);
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

        #[rstest]
        #[case(true, "true")]
        #[case(false, "false")]
        fn bool_literal_renders_lowercase(#[case] value: bool, #[case] expected: &str) {
            assert_eq!(CSharpLiteral::Bool(value).to_string(), expected);
        }
    }

    mod binary_op {
        use super::*;

        #[rstest]
        #[case(CSharpBinaryOp::Eq, "==")]
        #[case(CSharpBinaryOp::Add, "+")]
        #[case(CSharpBinaryOp::Mul, "*")]
        fn operator_renders_as_source_token(
            #[case] op: CSharpBinaryOp,
            #[case] expected: &str,
        ) {
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
                receiver: Box::new(CSharpExpression::Ident(CSharpIdent::This)),
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
                args: vec![],
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
                    receiver: Box::new(CSharpExpression::Ident(CSharpIdent::This)),
                    name: CSharpPropertyName::from_source("x"),
                }],
            };
            assert_eq!(expr.to_string(), "wire.WriteF64(this.X)");
        }

        #[test]
        fn method_call_with_type_args_renders_angle_brackets() {
            let expr = CSharpExpression::MethodCall {
                receiver: Box::new(reader()),
                method: CSharpMethodName::from_source("read_blittable_array"),
                type_args: vec![CSharpType::Int],
                args: vec![],
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
                args: vec![],
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
                    args: vec![],
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
                    args: vec![],
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
                args: vec![],
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
                    receiver: Box::new(CSharpExpression::Ident(CSharpIdent::Local(r0))),
                    method: CSharpMethodName::from_source("read_i32"),
                    type_args: vec![],
                    args: vec![],
                }),
            };
            assert_eq!(expr.to_string(), "_r0Bytes => _r0Bytes.ReadI32()");
        }

        #[test]
        fn is_binding_pattern_renders_captured_binding() {
            let expr = CSharpExpression::IsBindingPattern {
                value: Box::new(CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::Ident(CSharpIdent::This)),
                    name: CSharpPropertyName::from_source("name"),
                }),
                binding: local_for("opt"),
            };
            assert_eq!(expr.to_string(), "this.Name is { } _optBytes");
        }

        #[test]
        fn raw_passes_through_literal_source() {
            let expr = CSharpExpression::Raw("Point.Decode(reader)".to_string());
            assert_eq!(expr.to_string(), "Point.Decode(reader)");
        }

    }

    mod statement {
        use super::*;

        #[test]
        fn raw_passes_through_without_trailing_semicolon() {
            let stmt = CSharpStatement::Raw("wire.WriteF64(this.X)".to_string());
            assert_eq!(stmt.to_string(), "wire.WriteF64(this.X)");
        }

        /// Expression-as-statement keeps the expression's Display
        /// verbatim; the template adds the trailing `;`. Symmetric with
        /// `Raw`'s passthrough so the 10a shim swap is neutral.
        #[test]
        fn expression_statement_renders_expression_alone() {
            let stmt = CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(local_ident("wire")),
                method: CSharpMethodName::from_source("write_f64"),
                type_args: vec![],
                args: vec![CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::Ident(CSharpIdent::This)),
                    name: CSharpPropertyName::from_source("x"),
                }],
            });
            assert_eq!(stmt.to_string(), "wire.WriteF64(this.X)");
        }

        #[test]
        fn local_decl_includes_trailing_semicolon() {
            let decl = CSharpLocalDecl {
                declared_type: CSharpType::Array(Box::new(CSharpType::Byte)),
                name: local_for("v"),
                rhs: CSharpExpression::Raw("Encoding.UTF8.GetBytes(v)".to_string()),
            };
            let stmt = CSharpStatement::LocalDecl(decl);
            assert_eq!(stmt.to_string(), "byte[] _vBytes = Encoding.UTF8.GetBytes(v);");
        }

        fn raw(source: &str) -> CSharpStatement {
            CSharpStatement::Raw(source.to_string())
        }

        /// The `If` body renders each inner statement followed by
        /// `"; "` and ends with a bare `}`. Matches the old write-
        /// expression emitter's output byte-for-byte.
        #[test]
        fn if_with_two_then_stmts_and_single_else_stmt_matches_brace_spacing() {
            let stmt = CSharpStatement::If {
                cond: CSharpExpression::Raw("this.Name is { } opt0".to_string()),
                then: vec![
                    raw("wire.WriteU8((byte)1)"),
                    raw("wire.WriteString(opt0)"),
                ],
                otherwise: Some(vec![raw("wire.WriteU8((byte)0)")]),
            };
            assert_eq!(
                stmt.to_string(),
                "if (this.Name is { } opt0) { wire.WriteU8((byte)1); wire.WriteString(opt0); } else { wire.WriteU8((byte)0); }"
            );
        }

        #[test]
        fn if_without_else_omits_else_clause() {
            let stmt = CSharpStatement::If {
                cond: CSharpExpression::Raw("guard".to_string()),
                then: vec![raw("body")],
                otherwise: None,
            };
            assert_eq!(stmt.to_string(), "if (guard) { body; }");
        }

        #[test]
        fn foreach_renders_header_and_body_brace_block() {
            let stmt = CSharpStatement::ForEach {
                elem_type: CSharpType::String,
                var: local_for("name"),
                collection: CSharpExpression::Raw("_v.Names".to_string()),
                body: vec![raw("wire.WriteString(_nameBytes)")],
            };
            assert_eq!(
                stmt.to_string(),
                "foreach (string _nameBytes in _v.Names) { wire.WriteString(_nameBytes); }"
            );
        }

        /// Sequence joins with `"; "` and leaves the trailing `;`
        /// to the consuming template, matching the write-encoded-vec
        /// shape where `WriteI32(length)` precedes a `foreach`.
        #[test]
        fn sequence_joins_with_semicolon_space_and_no_trailing_semi() {
            let stmt = CSharpStatement::Sequence(vec![
                raw("wire.WriteI32(_v.Names.Length)"),
                CSharpStatement::ForEach {
                    elem_type: CSharpType::String,
                    var: local_for("item"),
                    collection: CSharpExpression::Raw("_v.Names".to_string()),
                    body: vec![raw("wire.WriteString(_itemBytes)")],
                },
            ]);
            assert_eq!(
                stmt.to_string(),
                "wire.WriteI32(_v.Names.Length); foreach (string _itemBytes in _v.Names) { wire.WriteString(_itemBytes); }"
            );
        }
    }
}
