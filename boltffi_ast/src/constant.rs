use serde::{Deserialize, Serialize};

use crate::{
    ClassId, ConstExpr, ConstantId, DeprecationInfo, DocComment, EnumId, RecordId, Source,
    SourceName, SourceSpan, TypeExpr, UserAttr,
};

/// An exported type that owns an associated constant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ConstantOwner<Record = RecordId, Enumeration = EnumId, Class = ClassId> {
    /// Constant declared on an exported record.
    Record(Record),
    /// Constant declared on an exported enum.
    Enum(Enumeration),
    /// Constant declared on an exported class.
    Class(Class),
}

impl ConstantOwner<RecordId, EnumId, ClassId> {
    /// Returns the stable source identity of the owning type.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Record(id) => id.as_str(),
            Self::Enum(id) => id.as_str(),
            Self::Class(id) => id.as_str(),
        }
    }
}

/// A constant exported in the source contract.
///
/// Constants keep the declared type and the expression the scanner was able to
/// preserve. This lets generated bindings expose named values without treating
/// them as zero-argument functions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ConstantDef {
    /// Stable constant identity derived from the canonical Rust path.
    pub id: ConstantId,
    /// Source constant name.
    pub name: SourceName,
    /// Exported type that owns this constant, or `None` for a top-level constant.
    #[serde(default)]
    pub owner: Option<ConstantOwner>,
    /// Declared Rust source type.
    pub type_expr: TypeExpr,
    /// Source expression used as the constant value.
    pub value: ConstExpr,
    /// User attributes preserved from the constant.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the constant.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the constant.
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl ConstantDef {
    /// Builds a constant definition.
    ///
    /// The `id` parameter is the stable constant ID. The `name` parameter is the
    /// canonical constant name. The `rust_type` and `value` parameters record
    /// the source declaration.
    ///
    /// Returns a constant with no attributes, documentation, or deprecation.
    pub fn new(
        id: ConstantId,
        name: impl Into<SourceName>,
        type_expr: TypeExpr,
        value: ConstExpr,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            owner: None,
            type_expr,
            value,
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{ConstExpr, ConstantDef, ConstantId, TypeExpr};
    use crate::{CanonicalName, IntegerLiteral, Literal, Primitive};

    #[test]
    fn missing_owner_deserializes_as_top_level_constant() {
        let constant = ConstantDef::new(
            ConstantId::new("demo::ANSWER"),
            CanonicalName::single("ANSWER"),
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(42, "42"))),
        );
        let mut serialized = serde_json::to_value(constant).expect("constant serializes");
        let Value::Object(fields) = &mut serialized else {
            panic!("constant serialization should be an object");
        };
        fields.remove("owner");

        let decoded: ConstantDef =
            serde_json::from_value(serialized).expect("legacy constant deserializes");

        assert_eq!(decoded.owner, None);
    }
}
