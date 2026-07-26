use std::fmt;

use boltffi_ast::{
    ConstExpr, FnSig, GenericArgument, Path, Primitive as AstPrimitive, ReturnDef, TypeExpr,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[non_exhaustive]
/// A stable signature name for an inline closure crossing the boundary.
///
/// The value is derived from the closure argument and return types during
/// lowering, then stored in the IR so backends do not rebuild names from source
/// types locally.
pub struct ClosureSignature {
    name: String,
}

impl ClosureSignature {
    pub(crate) fn from_fn_signature(signature: &FnSig) -> Self {
        Self {
            name: SignatureName::from_fn_signature(signature).to_string(),
        }
    }

    /// Returns the signature name without a backend-specific prefix.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the signature segment used in generated native symbols.
    pub fn symbol_part(&self) -> String {
        symbol_case(&format!("__Closure_{}", self.name))
    }
}

struct SignatureName<'signature> {
    params: &'signature [TypeExpr],
    returns: &'signature ReturnDef,
}

impl<'signature> SignatureName<'signature> {
    fn from_fn_signature(signature: &'signature FnSig) -> Self {
        Self {
            params: &signature.parameters,
            returns: &signature.returns,
        }
    }
}

impl fmt::Display for SignatureName<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (
            self.params.is_empty(),
            ReturnSignature::new(self.returns).is_void(),
        ) {
            (true, true) => formatter.write_str("Void"),
            (false, true) => TypeList(self.params).fmt(formatter),
            (true, false) => {
                formatter.write_str("To")?;
                ReturnSignature::new(self.returns).fmt(formatter)
            }
            (false, false) => {
                TypeList(self.params).fmt(formatter)?;
                formatter.write_str("To")?;
                ReturnSignature::new(self.returns).fmt(formatter)
            }
        }
    }
}

struct ReturnSignature<'signature> {
    returns: &'signature ReturnDef,
}

impl<'signature> ReturnSignature<'signature> {
    fn new(returns: &'signature ReturnDef) -> Self {
        Self { returns }
    }

    fn is_void(&self) -> bool {
        matches!(self.returns, ReturnDef::Void)
            || matches!(self.returns, ReturnDef::Value(type_expr) if matches!(type_expr, TypeExpr::Unit))
    }
}

impl fmt::Display for ReturnSignature<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.returns {
            ReturnDef::Void => formatter.write_str("Void"),
            ReturnDef::Value(type_expr) => TypeSignature(type_expr).fmt(formatter),
        }
    }
}

struct TypeList<'signature>(&'signature [TypeExpr]);

impl fmt::Display for TypeList<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .iter()
            .enumerate()
            .try_for_each(|(index, type_expr)| {
                if index > 0 {
                    formatter.write_str("_")?;
                }
                TypeSignature(type_expr).fmt(formatter)
            })
    }
}

struct TypeSignature<'signature>(&'signature TypeExpr);

impl fmt::Display for TypeSignature<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            TypeExpr::Primitive(primitive) => formatter.write_str(&primitive_signature(*primitive)),
            TypeExpr::Unit => formatter.write_str("Void"),
            TypeExpr::Record { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::Enum { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::Class { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::Custom { id, .. } => formatter.write_str(&source_type_signature(id.as_str())),
            TypeExpr::InternedString { pool_id, .. } => {
                write!(
                    formatter,
                    "InternedString{}",
                    source_type_signature(pool_id)
                )
            }
            TypeExpr::ImplTrait(bounds) | TypeExpr::Dyn(bounds) => match &bounds.base {
                boltffi_ast::BaseTrait::Named { id, .. } => {
                    formatter.write_str(&source_type_signature(id.as_str()))
                }
                boltffi_ast::BaseTrait::Function(_) => formatter.write_str("Closure"),
            },
            TypeExpr::FnPtr(_) => formatter.write_str("Closure"),
            TypeExpr::SelfType => formatter.write_str("Self"),
            TypeExpr::Vec(inner) => write!(formatter, "Vec{}", TypeSignature(inner)),
            TypeExpr::Slice(inner) => write!(formatter, "Slice{}", TypeSignature(inner)),
            TypeExpr::Boxed(inner) => write!(formatter, "Box{}", TypeSignature(inner)),
            TypeExpr::Arc(inner) => write!(formatter, "Arc{}", TypeSignature(inner)),
            TypeExpr::Option(inner) => write!(formatter, "Opt{}", TypeSignature(inner)),
            TypeExpr::Result { ok, err } => {
                write!(
                    formatter,
                    "Result{}Err{}",
                    TypeSignature(ok),
                    TypeSignature(err)
                )
            }
            TypeExpr::Tuple(elements) => {
                formatter.write_str("Tuple")?;
                TypeList(elements).fmt(formatter)
            }
            TypeExpr::Map { key, value, .. } => {
                write!(
                    formatter,
                    "Map{}To{}",
                    TypeSignature(key),
                    TypeSignature(value)
                )
            }
            TypeExpr::String => formatter.write_str("String"),
            TypeExpr::Str => formatter.write_str("Str"),
            TypeExpr::Builtin(kind) => formatter.write_str(kind.type_id()),
            TypeExpr::Parameter(parameter) => formatter.write_str(&parameter.name),
        }
    }
}

fn source_type_signature(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(capitalize)
        .collect()
}

fn path_signature(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| {
            let mut signature = capitalize(segment.name.as_str());
            segment
                .arguments
                .iter()
                .for_each(|argument| signature.push_str(&generic_argument_signature(argument)));
            signature
        })
        .collect()
}

fn generic_argument_signature(argument: &GenericArgument) -> String {
    match argument {
        GenericArgument::Type(type_expr) => TypeSignature(type_expr).to_string(),
        GenericArgument::Const(expr) => const_expr_signature(expr),
        GenericArgument::AssociatedType { name, type_expr } => {
            format!("{}{}", capitalize(name.as_str()), TypeSignature(type_expr))
        }
    }
}

fn const_expr_signature(expr: &ConstExpr) -> String {
    match expr {
        ConstExpr::Literal(literal) => source_type_signature(&format!("{:?}", literal)),
        ConstExpr::Path(path) => path_signature(path),
        ConstExpr::Array(elements) | ConstExpr::Tuple(elements) => {
            elements.iter().map(const_expr_signature).collect()
        }
        ConstExpr::Raw(text) => source_type_signature(text),
    }
}

fn primitive_signature(primitive: AstPrimitive) -> String {
    let rust_name = primitive.rust_name();
    rust_name
        .strip_suffix("size")
        .and_then(|prefix| {
            (prefix.len() == 1).then(|| format!("{}Size", prefix.to_ascii_uppercase()))
        })
        .unwrap_or_else(|| capitalize(rust_name))
}

fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(characters).collect(),
    }
}

fn symbol_case(name: &str) -> String {
    name.chars().enumerate().fold(
        String::with_capacity(name.len() + 4),
        |mut result, (index, character)| {
            if character.is_uppercase() {
                if index > 0 {
                    result.push('_');
                }
                result.push(character.to_ascii_lowercase());
            } else {
                result.push(character);
            }
            result
        },
    )
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{Path, Primitive, RecordId};

    use super::*;

    fn closure(parameters: Vec<TypeExpr>, returns: ReturnDef) -> FnSig {
        FnSig::new(parameters, returns)
    }

    fn record(id: &str, path: &str) -> TypeExpr {
        TypeExpr::record(RecordId::new(id), Path::single(path))
    }

    fn interned_string_in_module(module: &str, pool: &str) -> TypeExpr {
        let pool_id = format!("demo::{module}::{pool}");
        TypeExpr::interned_string(
            Path::single("InternedString"),
            &pool_id,
            Path::single(pool),
            vec!["Chrome".to_owned()],
        )
    }

    #[test]
    fn signature_keeps_nested_source_shape() {
        let closure = closure(
            vec![TypeExpr::option(record("demo::Point", "Point"))],
            ReturnDef::value(TypeExpr::result(
                TypeExpr::Primitive(Primitive::I32),
                record("demo::MathError", "MathError"),
            )),
        );

        assert_eq!(
            ClosureSignature::from_fn_signature(&closure).symbol_part(),
            "___closure__opt_demo_point_to_result_i32_err_demo_math_error"
        );
    }

    #[test]
    fn signature_includes_named_type_namespace() {
        let first = closure(vec![record("a::Point", "Point")], ReturnDef::Void);
        let second = closure(vec![record("b::Point", "Point")], ReturnDef::Void);

        assert_eq!(
            ClosureSignature::from_fn_signature(&first).symbol_part(),
            "___closure__a_point"
        );
        assert_eq!(
            ClosureSignature::from_fn_signature(&second).symbol_part(),
            "___closure__b_point"
        );
    }

    #[test]
    fn signature_includes_canonical_interned_string_pool_identity() {
        let first = closure(
            vec![interned_string_in_module("a", "BrowserName")],
            ReturnDef::Void,
        );
        let second = closure(
            vec![interned_string_in_module("b", "BrowserName")],
            ReturnDef::Void,
        );

        assert_eq!(
            ClosureSignature::from_fn_signature(&first).symbol_part(),
            "___closure__interned_string_demo_a_browser_name"
        );
        assert_eq!(
            ClosureSignature::from_fn_signature(&second).symbol_part(),
            "___closure__interned_string_demo_b_browser_name"
        );
    }

    #[test]
    fn symbol_case_preserves_callback_signature_casing() {
        assert_eq!(
            symbol_case("__Closure_I32_StringToBool"),
            "___closure__i32__string_to_bool"
        );
    }
}
