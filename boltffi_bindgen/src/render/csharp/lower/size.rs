//! Translates an IR [`SizeExpr`] into a typed C# AST. The structural
//! intermediate form lets downstream transforms inspect the expression
//! without reparsing source.
//!
//! Identifier rebinding (`v` → `sizeOpt0`, `item` → `sizeItem0`) is
//! handled through the [`Renames`](super::value::Renames) map passed
//! to [`super::value::render_value`].

use crate::ir::codec::VecLayout;
use crate::ir::ops::SizeExpr;

use super::super::ast::{
    CSharpArgumentList, CSharpBinaryOp, CSharpClassName, CSharpExpression, CSharpIdentity,
    CSharpLiteral, CSharpLocalName, CSharpMethodName, CSharpPropertyName, CSharpTypeReference,
};
use super::value::{Renames, render_value};

/// Counter state for the synthesized C# locals a size expression
/// introduces: the `sizeOpt{n}` pattern binding inside an option
/// ternary and the `sizeItem{n}` lambda parameter inside an encoded
/// vec's size call. Sibling size contributions summed into one
/// method body share a single instance so their counters advance
/// together and no two declarations collide in that scope.
#[derive(Debug, Default)]
pub(crate) struct SizeLocalCounters {
    option_binding_index: usize,
    loop_var_index: usize,
}

impl SizeLocalCounters {
    /// Mint the next `sizeOpt{n}` pattern-binding local.
    fn next_option_binding(&mut self) -> CSharpLocalName {
        let i = self.option_binding_index;
        self.option_binding_index += 1;
        CSharpLocalName::size_option_binding(i)
    }

    /// Mint the next `sizeItem{n}` loop-variable local.
    fn next_loop_var(&mut self) -> CSharpLocalName {
        let i = self.loop_var_index;
        self.loop_var_index += 1;
        CSharpLocalName::size_loop_var(i)
    }
}

/// Renders a size expression, threading a shared [`Renames`] map and
/// [`SizeLocalCounters`] so callers can (a) rebind inner `Var`
/// references and (b) share pattern-binding and loop counters across
/// sibling contributions summed into one method body.
pub(crate) fn lower_size_expr(
    size: &SizeExpr,
    renames: &Renames,
    locals: &mut SizeLocalCounters,
) -> CSharpExpression {
    match size {
        SizeExpr::Fixed(value) => CSharpExpression::Literal(CSharpLiteral::Int(*value as i64)),
        SizeExpr::StringLen(value) => {
            // `Encoding.UTF8.GetByteCount(value)`: static method on a
            // two-segment type path.
            CSharpExpression::MethodCall {
                receiver: Box::new(CSharpExpression::MemberAccess {
                    receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                        CSharpClassName::new("Encoding"),
                    ))),
                    name: CSharpPropertyName::from_source("UTF8"),
                }),
                method: CSharpMethodName::from_source("get_byte_count"),
                type_args: vec![],
                args: vec![render_value(value, renames)].into(),
            }
        }
        SizeExpr::BytesLen(value) => CSharpExpression::MemberAccess {
            receiver: Box::new(render_value(value, renames)),
            name: CSharpPropertyName::from_source("length"),
        },
        SizeExpr::WireSize { value, .. } => CSharpExpression::MethodCall {
            receiver: Box::new(render_value(value, renames)),
            method: CSharpMethodName::from_source("wire_encoded_size"),
            type_args: vec![],
            args: CSharpArgumentList::default(),
        },
        SizeExpr::Sum(parts) => {
            // The IR guarantees a non-empty sum in every reachable
            // shape. Reduce left-to-right with `Binary(Add)`.
            let mut rendered = parts.iter().map(|p| lower_size_expr(p, renames, locals));
            let first = rendered
                .next()
                .expect("SizeExpr::Sum must have at least one contribution");
            let folded = rendered.fold(first, |acc, next| CSharpExpression::Binary {
                op: CSharpBinaryOp::Add,
                left: Box::new(acc),
                right: Box::new(next),
            });
            CSharpExpression::Paren(Box::new(folded))
        }
        SizeExpr::OptionSize { value, inner } => {
            let binding = locals.next_option_binding();
            let mut inner_renames = renames.clone();
            // The IR's inner size references `v` as the option's
            // bound payload. Rebind that to the pattern variable so
            // nested `StringLen(Var("v"))` renders as
            // `Encoding.UTF8.GetByteCount(sizeOpt0)`.
            inner_renames.insert(
                "v".to_string(),
                CSharpExpression::Identity(CSharpIdentity::Local(binding.clone())),
            );
            let inner_expr = lower_size_expr(inner, &inner_renames, locals);
            let ternary = CSharpExpression::Paren(Box::new(CSharpExpression::Ternary {
                cond: Box::new(CSharpExpression::IsBindingPattern {
                    value: Box::new(render_value(value, renames)),
                    binding,
                }),
                then: Box::new(inner_expr),
                otherwise: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(0))),
            }));
            CSharpExpression::Paren(Box::new(CSharpExpression::Binary {
                op: CSharpBinaryOp::Add,
                left: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(1))),
                right: Box::new(ternary),
            }))
        }
        SizeExpr::VecSize {
            value,
            layout: VecLayout::Blittable { element_size },
            ..
        } => {
            // `(4 + value.Length * element_size)`: 4-byte length
            // prefix plus the raw element-count multiplier.
            let length_times_size = CSharpExpression::Binary {
                op: CSharpBinaryOp::Mul,
                left: Box::new(CSharpExpression::MemberAccess {
                    receiver: Box::new(render_value(value, renames)),
                    name: CSharpPropertyName::from_source("length"),
                }),
                right: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(
                    *element_size as i64,
                ))),
            };
            CSharpExpression::Paren(Box::new(CSharpExpression::Binary {
                op: CSharpBinaryOp::Add,
                left: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(4))),
                right: Box::new(length_times_size),
            }))
        }
        SizeExpr::VecSize {
            value,
            inner,
            layout: VecLayout::Encoded,
        } => {
            let loop_var = locals.next_loop_var();
            let mut inner_renames = renames.clone();
            // The IR's inner references `item` as the per-element
            // binding; rebind to `sizeItem{n}` so nested sizes render
            // against the lambda parameter.
            inner_renames.insert(
                "item".to_string(),
                CSharpExpression::Identity(CSharpIdentity::Local(loop_var.clone())),
            );
            let inner_expr = lower_size_expr(inner, &inner_renames, locals);
            CSharpExpression::MethodCall {
                receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                    CSharpClassName::new("WireWriter"),
                ))),
                method: CSharpMethodName::from_source("encoded_array_size"),
                type_args: vec![],
                args: vec![
                    render_value(value, renames),
                    CSharpExpression::Lambda {
                        param: loop_var,
                        body: Box::new(inner_expr),
                    },
                ]
                .into(),
            }
        }
        other => todo!(
            "C# backend has not yet implemented size expression support for {:?}",
            other
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::codec::VecLayout;
    use crate::ir::ids::{FieldName, ParamName};
    use crate::ir::ops::ValueExpr;

    fn lower_fresh(size: &SizeExpr) -> CSharpExpression {
        let mut locals = SizeLocalCounters::default();
        let renames = Renames::new();
        lower_size_expr(size, &renames, &mut locals)
    }

    fn named(name: &str) -> ValueExpr {
        ValueExpr::Named(ParamName::new(name).as_str().to_string())
    }

    fn field_of_this(field: &str) -> ValueExpr {
        ValueExpr::Field(Box::new(ValueExpr::Instance), FieldName::new(field))
    }

    #[test]
    fn fixed_renders_as_decimal_literal() {
        assert_eq!(lower_fresh(&SizeExpr::Fixed(8)).to_string(), "8");
    }

    #[test]
    fn string_len_renders_utf8_byte_count_call() {
        let size = SizeExpr::StringLen(named("name"));
        assert_eq!(
            lower_fresh(&size).to_string(),
            "Encoding.UTF8.GetByteCount(name)"
        );
    }

    #[test]
    fn bytes_len_renders_as_dot_length_access() {
        let size = SizeExpr::BytesLen(field_of_this("payload"));
        assert_eq!(lower_fresh(&size).to_string(), "this.Payload.Length");
    }

    #[test]
    fn wire_size_renders_as_wire_encoded_size_call() {
        let size = SizeExpr::WireSize {
            value: field_of_this("inner"),
            owner: None,
        };
        assert_eq!(
            lower_fresh(&size).to_string(),
            "this.Inner.WireEncodedSize()"
        );
    }

    /// `Sum` with one contribution still wraps in parens, keeping
    /// precedence sane when the sum participates in a larger expression.
    #[test]
    fn sum_with_single_contribution_still_parenthesizes() {
        let size = SizeExpr::Sum(vec![SizeExpr::Fixed(4)]);
        assert_eq!(lower_fresh(&size).to_string(), "(4)");
    }

    /// Regression: the IR wraps variable-length sizes in
    /// `Sum([Fixed(4), StringLen(v)])`; `StringLen` must render as the
    /// payload byte count alone, not `4 + byte_count`, otherwise a
    /// string field's wire size is over-counted by 4 bytes.
    #[test]
    fn sum_of_fixed_and_string_len_joins_with_plus() {
        let size = SizeExpr::Sum(vec![SizeExpr::Fixed(4), SizeExpr::StringLen(named("name"))]);
        assert_eq!(
            lower_fresh(&size).to_string(),
            "(4 + Encoding.UTF8.GetByteCount(name))"
        );
    }

    #[test]
    fn option_size_renders_pattern_binding_ternary_with_sizeopt_variable() {
        let size = SizeExpr::OptionSize {
            value: field_of_this("name"),
            inner: Box::new(SizeExpr::StringLen(ValueExpr::Var("v".to_string()))),
        };
        assert_eq!(
            lower_fresh(&size).to_string(),
            "(1 + (this.Name is { } sizeOpt0 ? Encoding.UTF8.GetByteCount(sizeOpt0) : 0))"
        );
    }

    /// Two option-size expressions sharing a [`SizeLocalCounters`] pick up
    /// distinct `sizeOpt{n}` pattern-variable names, because a shared
    /// `sizeOpt0` would redeclare the same local in one method scope.
    #[test]
    fn sibling_option_sizes_use_distinct_pattern_bindings() {
        let mut locals = SizeLocalCounters::default();
        let renames = Renames::new();
        let first = lower_size_expr(
            &SizeExpr::OptionSize {
                value: field_of_this("name"),
                inner: Box::new(SizeExpr::StringLen(ValueExpr::Var("v".to_string()))),
            },
            &renames,
            &mut locals,
        );
        let second = lower_size_expr(
            &SizeExpr::OptionSize {
                value: field_of_this("other"),
                inner: Box::new(SizeExpr::StringLen(ValueExpr::Var("v".to_string()))),
            },
            &renames,
            &mut locals,
        );
        assert!(
            first.to_string().contains("sizeOpt0"),
            "expecting first size to bind sizeOpt0, got {first}"
        );
        assert!(
            second.to_string().contains("sizeOpt1"),
            "expecting second size to bind sizeOpt1, got {second}"
        );
    }

    #[test]
    fn vec_size_blittable_renders_length_times_element_size() {
        let size = SizeExpr::VecSize {
            value: field_of_this("points"),
            inner: Box::new(SizeExpr::Fixed(0)),
            layout: VecLayout::Blittable { element_size: 16 },
        };
        assert_eq!(
            lower_fresh(&size).to_string(),
            "(4 + this.Points.Length * 16)"
        );
    }

    #[test]
    fn vec_size_encoded_renders_encoded_array_size_lambda() {
        let size = SizeExpr::VecSize {
            value: field_of_this("names"),
            inner: Box::new(SizeExpr::Sum(vec![
                SizeExpr::Fixed(4),
                SizeExpr::StringLen(ValueExpr::Var("item".to_string())),
            ])),
            layout: VecLayout::Encoded,
        };
        assert_eq!(
            lower_fresh(&size).to_string(),
            "WireWriter.EncodedArraySize(this.Names, sizeItem0 => (4 + Encoding.UTF8.GetByteCount(sizeItem0)))"
        );
    }
}
