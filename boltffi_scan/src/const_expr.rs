use std::num::NonZeroUsize;

use boltffi_ast::{
    ConstExpr, FloatLiteral, GenericArgument, IntegerLiteral, Literal, NamePart, Path, PathRoot,
    PathSegment,
};
use quote::ToTokens;

use crate::type_expr;

pub(super) struct Scanner<'scanner, 'types> {
    types: &'scanner type_expr::Scanner<'types>,
}

impl<'scanner, 'types> Scanner<'scanner, 'types> {
    pub(super) fn new(types: &'scanner type_expr::Scanner<'types>) -> Self {
        Self { types }
    }

    pub(super) fn scan(&self, expr: &syn::Expr) -> ConstExpr {
        match self.unwrapped(expr) {
            syn::Expr::Lit(lit) => self.literal(&lit.lit),
            syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
                self.negative(expr, &unary.expr)
            }
            syn::Expr::Path(path) => self.path_expr(expr, path),
            syn::Expr::Array(array) => ConstExpr::Array(
                array
                    .elems
                    .iter()
                    .map(|element| self.scan(element))
                    .collect(),
            ),
            syn::Expr::Tuple(tuple) => ConstExpr::Tuple(
                tuple
                    .elems
                    .iter()
                    .map(|element| self.scan(element))
                    .collect(),
            ),
            other => self.raw(other),
        }
    }

    fn unwrapped<'expr>(&self, expr: &'expr syn::Expr) -> &'expr syn::Expr {
        match expr {
            syn::Expr::Paren(paren) => self.unwrapped(&paren.expr),
            syn::Expr::Group(group) => self.unwrapped(&group.expr),
            _ => expr,
        }
    }

    fn literal(&self, literal: &syn::Lit) -> ConstExpr {
        match literal {
            syn::Lit::Bool(literal) => ConstExpr::Literal(Literal::Bool(literal.value)),
            syn::Lit::Int(literal) => literal
                .base10_parse::<i128>()
                .map(|value| {
                    ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(
                        value,
                        literal.to_string(),
                    )))
                })
                .unwrap_or_else(|_| self.raw(literal)),
            syn::Lit::Float(literal) => {
                ConstExpr::Literal(Literal::Float(FloatLiteral::new(literal.to_string())))
            }
            syn::Lit::Str(literal) => ConstExpr::Literal(Literal::String(literal.value())),
            syn::Lit::ByteStr(literal) => ConstExpr::Literal(Literal::Bytes(literal.value())),
            other => self.raw(other),
        }
    }

    fn negative(&self, source: &syn::Expr, expr: &syn::Expr) -> ConstExpr {
        match self.unwrapped(expr) {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Int(literal),
                ..
            }) => literal
                .base10_parse::<i128>()
                .map(|value| {
                    ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(
                        -value,
                        format!("-{literal}"),
                    )))
                })
                .unwrap_or_else(|_| self.raw(source)),
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Float(literal),
                ..
            }) => ConstExpr::Literal(Literal::Float(FloatLiteral::new(format!("-{literal}")))),
            _ => self.raw(source),
        }
    }

    fn path_expr(&self, source: &syn::Expr, path: &syn::ExprPath) -> ConstExpr {
        if path.qself.is_some() {
            return self.raw(source);
        }
        self.path(&path.path)
            .map(ConstExpr::Path)
            .unwrap_or_else(|| self.raw(source))
    }

    fn path(&self, path: &syn::Path) -> Option<Path> {
        let segments = path.segments.iter().collect::<Vec<_>>();
        let (root, rest) = if path.leading_colon.is_some() {
            (PathRoot::Absolute, segments.as_slice())
        } else {
            match segments.as_slice() {
                [first, rest @ ..] if first.ident == "crate" => (PathRoot::Crate, rest),
                [first, rest @ ..] if first.ident == "self" => (PathRoot::Self_, rest),
                [first, ..] if first.ident == "super" => {
                    let levels = segments
                        .iter()
                        .take_while(|segment| segment.ident == "super")
                        .count();
                    (
                        PathRoot::Super(
                            NonZeroUsize::new(levels)
                                .expect("super path has at least one parent segment"),
                        ),
                        &segments[levels..],
                    )
                }
                _ => (PathRoot::Relative, segments.as_slice()),
            }
        };
        rest.iter()
            .map(|segment| self.segment(segment))
            .collect::<Option<Vec<_>>>()
            .map(|segments| Path::new(root, segments))
    }

    fn segment(&self, segment: &syn::PathSegment) -> Option<PathSegment> {
        match &segment.arguments {
            syn::PathArguments::None => Some(PathSegment::new(segment.ident.to_string())),
            syn::PathArguments::AngleBracketed(arguments) => arguments
                .args
                .iter()
                .map(|argument| self.generic_argument(argument))
                .collect::<Option<Vec<_>>>()
                .map(|arguments| PathSegment::with_arguments(segment.ident.to_string(), arguments)),
            syn::PathArguments::Parenthesized(_) => None,
        }
    }

    fn generic_argument(&self, argument: &syn::GenericArgument) -> Option<GenericArgument> {
        match argument {
            syn::GenericArgument::Type(ty) => self.types.scan(ty).ok().map(GenericArgument::Type),
            syn::GenericArgument::Const(expr) => Some(GenericArgument::Const(self.scan(expr))),
            syn::GenericArgument::AssocType(associated) => {
                self.types.scan(&associated.ty).ok().map(|type_expr| {
                    GenericArgument::AssociatedType {
                        name: NamePart::new(associated.ident.to_string()),
                        type_expr,
                    }
                })
            }
            _ => None,
        }
    }

    fn raw(&self, tokens: &impl ToTokens) -> ConstExpr {
        ConstExpr::Raw(tokens.to_token_stream().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{Primitive, TypeExpr};

    fn scan(source: &str) -> ConstExpr {
        let expr = syn::parse_str(source).expect("valid expression");
        let declared_types = DeclaredTypes::new();
        let types = type_expr::Scanner::new(&declared_types);
        Scanner::new(&types).scan(&expr)
    }

    #[test]
    fn scans_literal_families_without_erasing_value_spelling() {
        assert_eq!(scan("true"), ConstExpr::Literal(Literal::Bool(true)));
        assert_eq!(
            scan("0xff_u32"),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(255, "0xff_u32")))
        );
        assert_eq!(
            scan("-42"),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(-42, "-42")))
        );
        assert_eq!(
            scan("-1.5f32"),
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("-1.5f32")))
        );
        assert_eq!(
            scan("\"bolt\""),
            ConstExpr::Literal(Literal::String("bolt".to_owned()))
        );
        assert_eq!(
            scan("b\"ffi\""),
            ConstExpr::Literal(Literal::Bytes(b"ffi".to_vec()))
        );
    }

    #[test]
    fn scans_paths_with_root_and_segments() {
        assert_eq!(
            scan("crate::Mode::Fast"),
            ConstExpr::Path(Path::new(
                PathRoot::Crate,
                vec![PathSegment::new("Mode"), PathSegment::new("Fast")]
            ))
        );
        assert_eq!(
            scan("::demo::Mode::Fast"),
            ConstExpr::Path(Path::new(
                PathRoot::Absolute,
                vec![
                    PathSegment::new("demo"),
                    PathSegment::new("Mode"),
                    PathSegment::new("Fast"),
                ]
            ))
        );
    }

    #[test]
    fn scans_arrays_and_tuples_recursively() {
        assert_eq!(
            scan("[1, 2]"),
            ConstExpr::Array(vec![
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ])
        );
        assert_eq!(
            scan("(true, \"ok\")"),
            ConstExpr::Tuple(vec![
                ConstExpr::Literal(Literal::Bool(true)),
                ConstExpr::Literal(Literal::String("ok".to_owned())),
            ])
        );
    }

    #[test]
    fn preserves_supported_generic_path_arguments() {
        assert_eq!(
            scan("Factory::<i32, 4, Item = String>::VALUE"),
            ConstExpr::Path(Path::new(
                PathRoot::Relative,
                vec![
                    PathSegment::with_arguments(
                        "Factory",
                        vec![
                            GenericArgument::Type(TypeExpr::Primitive(Primitive::I32)),
                            GenericArgument::Const(ConstExpr::Literal(Literal::Integer(
                                IntegerLiteral::new(4, "4"),
                            ))),
                            GenericArgument::AssociatedType {
                                name: NamePart::new("Item"),
                                type_expr: TypeExpr::String,
                            },
                        ],
                    ),
                    PathSegment::new("VALUE"),
                ],
            ))
        );
    }

    #[test]
    fn unsupported_expression_shapes_fall_back_to_raw_tokens() {
        assert_eq!(scan("1 << 2"), ConstExpr::Raw("1 << 2".to_owned()));
        assert_eq!(scan("'x'"), ConstExpr::Raw("'x'".to_owned()));
    }
}
