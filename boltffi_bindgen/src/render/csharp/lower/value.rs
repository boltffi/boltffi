//! Translates an IR [`ValueExpr`] into a typed C# AST reference.
//!
//! Takes a `renames` map so callers can rebind inner `Var` references
//! without post-pass string rewriting: when a size- or write-option
//! introduces a pattern binding (`sizeOpt0`, `opt0`) or a foreach loop
//! introduces an item variable (`sizeItem0`, `item0`), the caller
//! extends the map before recursing into the inner seq and this helper
//! substitutes the bound expression at the point of use.

use std::collections::HashMap;

use crate::ir::ops::ValueExpr;

use super::super::ast::{
    CSharpExpression, CSharpIdentity, CSharpLocalName, CSharpParamName, CSharpPropertyName,
};

/// Active rebindings from IR variable names (as they appear in
/// [`ValueExpr::Var`]) to the C# expression that should stand in for
/// them at the render site. Empty at the outermost call; populated
/// each time the lowerer enters a binding scope.
pub(super) type Renames = HashMap<String, CSharpExpression>;

/// Render an IR `ValueExpr` as a typed C# expression under the given
/// [`Renames`] map.
///
/// - `Instance` → `this`.
/// - `Var(n)` → the renamed expression if `renames` binds `n`, else a
///   free identifier with the raw name.
/// - `Named(n)` → camelCase-converted free identifier (with the same
///   keyword escape the param naming applies).
/// - `Field(parent, f)` → `parent.F` where `F` is the PascalCase
///   property name (again, with keyword escape).
pub(super) fn render_value(value: &ValueExpr, renames: &Renames) -> CSharpExpression {
    match value {
        ValueExpr::Instance => CSharpExpression::Identity(CSharpIdentity::This),
        ValueExpr::Var(name) => match renames.get(name) {
            Some(expr) => expr.clone(),
            None => CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(
                name.clone(),
            ))),
        },
        ValueExpr::Named(name) => {
            // Param-typed identifier: the camelCase + `@`-keyword-escape
            // conversion lives on `CSharpParamName::from_source`.
            CSharpExpression::Identity(CSharpIdentity::Param(CSharpParamName::from_source(name)))
        }
        ValueExpr::Field(parent, field) => CSharpExpression::MemberAccess {
            receiver: Box::new(render_value(parent, renames)),
            name: CSharpPropertyName::from_source(field.as_str()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ids::FieldName;

    fn empty() -> Renames {
        Renames::new()
    }

    #[test]
    fn instance_renders_as_this_keyword() {
        assert_eq!(
            render_value(&ValueExpr::Instance, &empty()).to_string(),
            "this"
        );
    }

    #[test]
    fn var_with_no_rename_renders_name_verbatim() {
        let expr = render_value(&ValueExpr::Var("v".to_string()), &empty());
        assert_eq!(expr.to_string(), "v");
    }

    /// When a caller has introduced a binding (e.g. an option's
    /// pattern binding), the var reference is replaced by the bound
    /// expression wholesale. This is how the AST-level equivalent of
    /// the old `replace_identifier_occurrences("v", "sizeOpt0")` pass
    /// works without a post-pass string scan.
    #[test]
    fn var_with_rename_substitutes_bound_expression() {
        let mut renames = Renames::new();
        renames.insert(
            "v".to_string(),
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("sizeOpt0"))),
        );
        let expr = render_value(&ValueExpr::Var("v".to_string()), &renames);
        assert_eq!(expr.to_string(), "sizeOpt0");
    }

    #[test]
    fn named_converts_source_to_camel_case_bare_reference() {
        let expr = render_value(&ValueExpr::Named("my_field".to_string()), &empty());
        assert_eq!(expr.to_string(), "myField");
    }

    /// Field access walks through the parent and renders each segment
    /// as `PascalCase` to match the record's property names.
    #[test]
    fn field_access_on_instance_renders_as_this_dot_pascal_case() {
        let expr = render_value(
            &ValueExpr::Field(Box::new(ValueExpr::Instance), FieldName::new("radius")),
            &empty(),
        );
        assert_eq!(expr.to_string(), "this.Radius");
    }

    #[test]
    fn field_access_chains_through_nested_field() {
        let expr = render_value(
            &ValueExpr::Field(
                Box::new(ValueExpr::Field(
                    Box::new(ValueExpr::Instance),
                    FieldName::new("origin"),
                )),
                FieldName::new("x"),
            ),
            &empty(),
        );
        assert_eq!(expr.to_string(), "this.Origin.X");
    }

    #[test]
    fn field_access_on_var_respects_renames_for_receiver() {
        let mut renames = Renames::new();
        renames.insert(
            "_v".to_string(),
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("_rebound"))),
        );
        let expr = render_value(
            &ValueExpr::Field(
                Box::new(ValueExpr::Var("_v".to_string())),
                FieldName::new("radius"),
            ),
            &renames,
        );
        assert_eq!(expr.to_string(), "_rebound.Radius");
    }
}
