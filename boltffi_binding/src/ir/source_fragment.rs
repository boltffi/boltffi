//! Interpreting per-invocation source records into one aggregated [`SourceContract`].
//!
//! A record's JSON payload is one [`SourceFragment`]: the `boltffi_ast` declaration the
//! macro invocation saw, with two kinds of placeholder the macro cannot fill in itself.
//! Ids are written `$self::Name` and minted here against the record's `module_path!()`. Named type references are provisional [`TypeExpr::Record`] leaves
//! whose id is `$slot:N`, replaced here by the type node the referenced type's own
//! definition site resolved through rustc.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use boltffi_ast::{
    ClassDef, ClassId, ConstantDef, ConstantId, CustomTypeDef, EnumDef, EnumId, FieldDef,
    FunctionDef, FunctionId, MethodDef, NamePart, PackageInfo, Path, PathSegment, Primitive,
    RecordDef, RecordId, ReturnDef, SourceContract, StreamDef, StreamId, TraitDef, TraitId,
    TypeExpr, VariantPayload,
};
use serde::{Deserialize, Serialize};

use crate::ir::source_record::RawSourceRecord;

/// Id prefix marking a declaration's own identity, minted from the record's module path.
pub const SELF_ID: &str = "$self";

/// Id prefix marking a slot-deferred type reference inside a fragment.
pub const SLOT_ID_PREFIX: &str = "$slot:";

/// One declaration captured by one macro invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFragment {
    /// A `#[data]` struct.
    Record(RecordDef),
    /// A `#[data]` enum.
    Enum(EnumDef),
    /// An `#[export]` free function.
    Function(FunctionDef),
    /// An `#[export]`ed class-style object.
    Class(ClassDef),
    /// An `#[export]`ed callback trait.
    Trait(TraitDef),
    /// A stream export.
    Stream(StreamDef),
    /// An `#[export]`ed constant.
    Constant(ConstantDef),
    /// A `custom_type!` registration.
    Custom(CustomTypeDef),
    /// An invocation the per-invocation capture cannot describe yet.
    ///
    /// Its presence means the crate's source records are incomplete; aggregation refuses
    /// the whole set so consumers fall back to the legacy path instead of shipping a
    /// silently partial contract.
    Unsupported {
        /// Item name at the invocation site.
        name: String,
        /// Why the capture could not describe the item.
        reason: String,
    },
}

/// One resolved type node from a slot descriptor.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TypeNode {
    /// A named declaration, by canonical id.
    Id {
        /// Canonical id of the referenced declaration.
        id: String,
    },
    /// A primitive or builtin leaf.
    Prim {
        /// Rust spelling of the leaf type.
        prim: String,
    },
    /// A container shape with type arguments.
    Shape {
        /// Shape name, such as `Vec` or `HashMap`.
        shape: String,
        /// Type arguments in declaration order.
        args: Vec<TypeNode>,
    },
}

/// Aggregates decoded source records into one source contract.
///
/// The contract's declarations are sorted by id within each family, so the result does
/// not depend on link order.
pub fn aggregate_records(
    records: &[RawSourceRecord],
    package: PackageInfo,
) -> Result<SourceContract, SourceFragmentError> {
    let mut fragments = Vec::new();
    for record in records {
        let mut fragment: SourceFragment =
            serde_json::from_slice(&record.json).map_err(|error| SourceFragmentError::Decode {
                module: record.module.clone(),
                message: error.to_string(),
            })?;
        if let SourceFragment::Unsupported { name, reason } = &fragment {
            return Err(SourceFragmentError::UnsupportedCapture {
                module: record.module.clone(),
                name: name.clone(),
                reason: reason.clone(),
            });
        }
        let slots = record
            .slots
            .iter()
            .map(|slot| {
                serde_json::from_str::<TypeNode>(slot).map_err(|error| {
                    SourceFragmentError::SlotDescriptor {
                        module: record.module.clone(),
                        message: error.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        mint_self_ids(&mut fragment, &record.module);
        fragments.push((fragment, slots));
    }

    let mut kinds = HashMap::new();
    let mut unique: HashMap<String, (&SourceFragment, &[TypeNode])> = HashMap::new();
    for (fragment, slots) in &fragments {
        let id = fragment_id(fragment).to_owned();
        match unique.entry(id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert((fragment, slots));
            }
            Entry::Occupied(entry) if *entry.get() == (fragment, slots.as_slice()) => {}
            Entry::Occupied(_) => {
                return Err(SourceFragmentError::DuplicateDeclaration { id });
            }
        }
        if let Some(kind) = declared_kind(fragment) {
            kinds.insert(id, kind);
        }
    }

    let mut contract = SourceContract::new(package);
    let mut seen = HashMap::new();
    for (mut fragment, slots) in fragments {
        let id = fragment_id(&fragment).to_owned();
        if seen.insert(id, ()).is_some() {
            continue;
        }
        resolve_fragment(&mut fragment, &slots, &kinds)?;
        match fragment {
            SourceFragment::Record(def) => contract.records.push(def),
            SourceFragment::Enum(def) => contract.enums.push(def),
            SourceFragment::Function(def) => contract.functions.push(def),
            SourceFragment::Class(def) => contract.classes.push(def),
            SourceFragment::Trait(def) => contract.traits.push(def),
            SourceFragment::Stream(def) => contract.streams.push(def),
            SourceFragment::Constant(def) => contract.constants.push(def),
            SourceFragment::Custom(def) => contract.customs.push(def),
            SourceFragment::Unsupported { .. } => {}
        }
    }

    contract.records.sort_by(|a, b| a.id.cmp(&b.id));
    contract.enums.sort_by(|a, b| a.id.cmp(&b.id));
    contract.functions.sort_by(|a, b| a.id.cmp(&b.id));
    contract.classes.sort_by(|a, b| a.id.cmp(&b.id));
    contract.traits.sort_by(|a, b| a.id.cmp(&b.id));
    contract.streams.sort_by(|a, b| a.id.cmp(&b.id));
    contract.constants.sort_by(|a, b| a.id.cmp(&b.id));
    contract.customs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(contract)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeclaredKind {
    Record,
    Enum,
    Class,
    Custom,
}

fn declared_kind(fragment: &SourceFragment) -> Option<DeclaredKind> {
    match fragment {
        SourceFragment::Record(_) => Some(DeclaredKind::Record),
        SourceFragment::Enum(_) => Some(DeclaredKind::Enum),
        SourceFragment::Class(_) => Some(DeclaredKind::Class),
        SourceFragment::Custom(_) => Some(DeclaredKind::Custom),
        SourceFragment::Function(_)
        | SourceFragment::Trait(_)
        | SourceFragment::Stream(_)
        | SourceFragment::Constant(_)
        | SourceFragment::Unsupported { .. } => None,
    }
}

fn fragment_id(fragment: &SourceFragment) -> &str {
    match fragment {
        SourceFragment::Record(def) => def.id.as_str(),
        SourceFragment::Enum(def) => def.id.as_str(),
        SourceFragment::Function(def) => def.id.as_str(),
        SourceFragment::Class(def) => def.id.as_str(),
        SourceFragment::Trait(def) => def.id.as_str(),
        SourceFragment::Stream(def) => def.id.as_str(),
        SourceFragment::Constant(def) => def.id.as_str(),
        SourceFragment::Custom(def) => def.id.as_str(),
        SourceFragment::Unsupported { name, .. } => name,
    }
}

fn minted(id: &str, module: &str) -> Option<String> {
    id.strip_prefix(SELF_ID)
        .filter(|rest| rest.is_empty() || rest.starts_with("::"))
        .map(|rest| format!("{module}{rest}"))
}

fn mint_self_ids(fragment: &mut SourceFragment, module: &str) {
    match fragment {
        SourceFragment::Record(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = RecordId::new(value);
            }
            mint_methods(&mut def.methods, module);
        }
        SourceFragment::Enum(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = EnumId::new(value);
            }
            mint_methods(&mut def.methods, module);
        }
        SourceFragment::Function(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = FunctionId::new(value);
            }
        }
        SourceFragment::Class(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = ClassId::new(value);
            }
            mint_methods(&mut def.methods, module);
        }
        SourceFragment::Trait(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = TraitId::new(value);
            }
            mint_methods(&mut def.methods, module);
        }
        SourceFragment::Stream(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = StreamId::new(value);
            }
            if let Some(owner) = &mut def.owner
                && let Some(value) = minted(owner.as_str(), module)
            {
                *owner = ClassId::new(value);
            }
        }
        SourceFragment::Constant(def) => {
            if let Some(value) = minted(def.id.as_str(), module) {
                def.id = ConstantId::new(value);
            }
        }
        SourceFragment::Custom(_) | SourceFragment::Unsupported { .. } => {}
    }
}

fn mint_methods(methods: &mut [MethodDef], prefix: &str) {
    for method in methods {
        if let Some(value) = minted(method.id.as_str(), prefix) {
            method.id = boltffi_ast::MethodId::new(value);
        }
    }
}

fn resolve_fragment(
    fragment: &mut SourceFragment,
    slots: &[TypeNode],
    kinds: &HashMap<String, DeclaredKind>,
) -> Result<(), SourceFragmentError> {
    let mut resolve = |expr: &mut TypeExpr| resolve_expr(expr, slots, kinds);
    match fragment {
        SourceFragment::Record(def) => {
            resolve_fields(&mut def.fields, &mut resolve)?;
            resolve_methods(&mut def.methods, &mut resolve)
        }
        SourceFragment::Enum(def) => {
            for variant in &mut def.variants {
                match &mut variant.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(types) => {
                        for expr in types {
                            resolve(expr)?;
                        }
                    }
                    VariantPayload::Struct(fields) => resolve_fields(fields, &mut resolve)?,
                }
            }
            resolve_methods(&mut def.methods, &mut resolve)
        }
        SourceFragment::Function(def) => {
            resolve_callable(&mut def.parameters, &mut def.returns, &mut resolve)
        }
        SourceFragment::Class(def) => resolve_methods(&mut def.methods, &mut resolve),
        SourceFragment::Trait(def) => resolve_methods(&mut def.methods, &mut resolve),
        SourceFragment::Stream(def) => resolve(&mut def.item_type),
        SourceFragment::Constant(def) => resolve(&mut def.type_expr),
        SourceFragment::Custom(def) => resolve(&mut def.repr),
        SourceFragment::Unsupported { .. } => Ok(()),
    }
}

fn resolve_fields(
    fields: &mut [FieldDef],
    resolve: &mut impl FnMut(&mut TypeExpr) -> Result<(), SourceFragmentError>,
) -> Result<(), SourceFragmentError> {
    for field in fields {
        resolve(&mut field.type_expr)?;
    }
    Ok(())
}

fn resolve_methods(
    methods: &mut [MethodDef],
    resolve: &mut impl FnMut(&mut TypeExpr) -> Result<(), SourceFragmentError>,
) -> Result<(), SourceFragmentError> {
    for method in methods {
        resolve_callable(&mut method.parameters, &mut method.returns, resolve)?;
    }
    Ok(())
}

fn resolve_callable(
    parameters: &mut [boltffi_ast::ParameterDef],
    returns: &mut ReturnDef,
    resolve: &mut impl FnMut(&mut TypeExpr) -> Result<(), SourceFragmentError>,
) -> Result<(), SourceFragmentError> {
    for parameter in parameters {
        resolve(&mut parameter.type_expr)?;
    }
    if let ReturnDef::Value(expr) = returns {
        resolve(expr)?;
    }
    Ok(())
}

fn resolve_expr(
    expr: &mut TypeExpr,
    slots: &[TypeNode],
    kinds: &HashMap<String, DeclaredKind>,
) -> Result<(), SourceFragmentError> {
    if let TypeExpr::Record { id, path } = expr
        && let Some(index) = id.as_str().strip_prefix(SLOT_ID_PREFIX)
    {
        let index: usize = index
            .parse()
            .map_err(|_| SourceFragmentError::UnknownSlot {
                slot: index.to_owned(),
            })?;
        let node = slots.get(index).ok_or(SourceFragmentError::MissingSlot {
            index,
            available: slots.len(),
        })?;
        *expr = node_expr(node, Some(path.clone()), kinds)?;
        return Ok(());
    }

    match expr {
        TypeExpr::Boxed(inner)
        | TypeExpr::Arc(inner)
        | TypeExpr::Vec(inner)
        | TypeExpr::Slice(inner)
        | TypeExpr::Option(inner) => resolve_expr(inner, slots, kinds),
        TypeExpr::Result { ok, err } => {
            resolve_expr(ok, slots, kinds)?;
            resolve_expr(err, slots, kinds)
        }
        TypeExpr::Map { key, value, .. } => {
            resolve_expr(key, slots, kinds)?;
            resolve_expr(value, slots, kinds)
        }
        TypeExpr::Tuple(elements) => {
            for element in elements {
                resolve_expr(element, slots, kinds)?;
            }
            Ok(())
        }
        TypeExpr::FnPtr(signature) => resolve_fn_sig(signature, slots, kinds),
        TypeExpr::Dyn(bounds) | TypeExpr::ImplTrait(bounds) => {
            if let boltffi_ast::BaseTrait::Function(function_trait) = &mut bounds.base {
                resolve_fn_sig(&mut function_trait.signature, slots, kinds)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn resolve_fn_sig(
    signature: &mut boltffi_ast::FnSig,
    slots: &[TypeNode],
    kinds: &HashMap<String, DeclaredKind>,
) -> Result<(), SourceFragmentError> {
    for parameter in &mut signature.parameters {
        resolve_expr(parameter, slots, kinds)?;
    }
    if let ReturnDef::Value(expr) = &mut signature.returns {
        resolve_expr(expr, slots, kinds)?;
    }
    Ok(())
}

fn node_expr(
    node: &TypeNode,
    path: Option<Path>,
    kinds: &HashMap<String, DeclaredKind>,
) -> Result<TypeExpr, SourceFragmentError> {
    match node {
        TypeNode::Id { id } => {
            let kind = kinds
                .get(id)
                .ok_or_else(|| SourceFragmentError::UnresolvedReference { id: id.clone() })?;
            let path = path.unwrap_or_else(|| synthesized_path(id));
            Ok(match kind {
                DeclaredKind::Record => TypeExpr::record(RecordId::new(id.clone()), path),
                DeclaredKind::Enum => TypeExpr::enumeration(EnumId::new(id.clone()), path),
                DeclaredKind::Class => TypeExpr::class(ClassId::new(id.clone()), path),
                DeclaredKind::Custom => {
                    TypeExpr::custom(boltffi_ast::CustomTypeId::new(id.clone()), path)
                }
            })
        }
        TypeNode::Prim { prim } => leaf_expr(prim),
        TypeNode::Shape { shape, args } => {
            let mut resolved = args
                .iter()
                .map(|arg| node_expr(arg, None, kinds))
                .collect::<Result<Vec<_>, _>>()?;
            shape_expr(shape, &mut resolved)
        }
    }
}

fn synthesized_path(id: &str) -> Path {
    let name = id.rsplit("::").next().unwrap_or(id);
    Path::new(
        boltffi_ast::PathRoot::Relative,
        vec![PathSegment::new(NamePart::new(name))],
    )
}

fn leaf_expr(name: &str) -> Result<TypeExpr, SourceFragmentError> {
    let primitive = match name {
        "bool" => Some(Primitive::Bool),
        "i8" => Some(Primitive::I8),
        "u8" => Some(Primitive::U8),
        "i16" => Some(Primitive::I16),
        "u16" => Some(Primitive::U16),
        "i32" => Some(Primitive::I32),
        "u32" => Some(Primitive::U32),
        "i64" => Some(Primitive::I64),
        "u64" => Some(Primitive::U64),
        "isize" => Some(Primitive::ISize),
        "usize" => Some(Primitive::USize),
        "f32" => Some(Primitive::F32),
        "f64" => Some(Primitive::F64),
        _ => None,
    };
    if let Some(primitive) = primitive {
        return Ok(TypeExpr::Primitive(primitive));
    }
    match name {
        "String" => Ok(TypeExpr::String),
        "Duration" => Ok(TypeExpr::builtin(boltffi_ast::BuiltinType::Duration)),
        "SystemTime" => Ok(TypeExpr::builtin(boltffi_ast::BuiltinType::SystemTime)),
        "Uuid" => Ok(TypeExpr::builtin(boltffi_ast::BuiltinType::Uuid)),
        "Url" => Ok(TypeExpr::builtin(boltffi_ast::BuiltinType::Url)),
        _ => Err(SourceFragmentError::UnknownLeaf {
            name: name.to_owned(),
        }),
    }
}

fn shape_expr(shape: &str, args: &mut Vec<TypeExpr>) -> Result<TypeExpr, SourceFragmentError> {
    let arity_error = |expected: usize, args: &Vec<TypeExpr>| SourceFragmentError::ShapeArity {
        shape: shape.to_owned(),
        expected,
        actual: args.len(),
    };
    match shape {
        "Vec" | "Option" | "Box" | "Arc" => {
            if args.len() != 1 {
                return Err(arity_error(1, args));
            }
            let inner = args.remove(0);
            Ok(match shape {
                "Vec" => TypeExpr::vec(inner),
                "Option" => TypeExpr::option(inner),
                "Box" => TypeExpr::boxed(inner),
                _ => TypeExpr::arc(inner),
            })
        }
        "HashMap" | "BTreeMap" | "Result" => {
            if args.len() != 2 {
                return Err(arity_error(2, args));
            }
            let second = args.remove(1);
            let first = args.remove(0);
            Ok(match shape {
                "HashMap" => TypeExpr::hash_map(first, second),
                "BTreeMap" => TypeExpr::btree_map(first, second),
                _ => TypeExpr::result(first, second),
            })
        }
        _ => Err(SourceFragmentError::UnknownShape {
            shape: shape.to_owned(),
        }),
    }
}

/// Failure while interpreting source records into a contract.
#[derive(Debug)]
pub enum SourceFragmentError {
    /// A record payload did not deserialize as a fragment.
    Decode {
        /// Module path of the emitting invocation.
        module: String,
        /// Deserialization error text.
        message: String,
    },
    /// A slot descriptor did not parse as a type node.
    SlotDescriptor {
        /// Module path of the emitting invocation.
        module: String,
        /// Parse error text.
        message: String,
    },
    /// A slot placeholder index is not a number.
    UnknownSlot {
        /// The malformed index text.
        slot: String,
    },
    /// A slot placeholder points past the record's slot table.
    MissingSlot {
        /// Placeholder index.
        index: usize,
        /// Number of slots the record carries.
        available: usize,
    },
    /// A referenced declaration is missing from the aggregated records.
    UnresolvedReference {
        /// Canonical id of the missing declaration.
        id: String,
    },
    /// A slot leaf names an unknown primitive or builtin.
    UnknownLeaf {
        /// The unknown leaf name.
        name: String,
    },
    /// A slot shape is not a known container.
    UnknownShape {
        /// The unknown shape name.
        shape: String,
    },
    /// A slot shape carries the wrong number of arguments.
    ShapeArity {
        /// Shape name.
        shape: String,
        /// Expected argument count.
        expected: usize,
        /// Actual argument count.
        actual: usize,
    },
    /// Two records declare the same id with different content.
    DuplicateDeclaration {
        /// The conflicting canonical id.
        id: String,
    },
    /// A record marks an invocation the capture cannot describe yet.
    UnsupportedCapture {
        /// Module path of the emitting invocation.
        module: String,
        /// Item name at the invocation site.
        name: String,
        /// Why the capture could not describe the item.
        reason: String,
    },
}

impl std::fmt::Display for SourceFragmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode { module, message } => {
                write!(
                    formatter,
                    "source record in `{module}` failed to decode: {message}"
                )
            }
            Self::SlotDescriptor { module, message } => write!(
                formatter,
                "source record in `{module}` has an unreadable slot descriptor: {message}"
            ),
            Self::UnknownSlot { slot } => {
                write!(formatter, "slot placeholder `{slot}` is not an index")
            }
            Self::MissingSlot { index, available } => write!(
                formatter,
                "slot {index} is out of range for a record with {available} slots"
            ),
            Self::UnresolvedReference { id } => write!(
                formatter,
                "`{id}` is referenced but no aggregated record declares it"
            ),
            Self::UnknownLeaf { name } => {
                write!(formatter, "slot leaf `{name}` is not a known FFI type")
            }
            Self::UnknownShape { shape } => {
                write!(formatter, "slot shape `{shape}` is not a known container")
            }
            Self::ShapeArity {
                shape,
                expected,
                actual,
            } => write!(
                formatter,
                "slot shape `{shape}` expects {expected} arguments, found {actual}"
            ),
            Self::DuplicateDeclaration { id } => write!(
                formatter,
                "two records declare `{id}` with different content"
            ),
            Self::UnsupportedCapture {
                module,
                name,
                reason,
            } => write!(
                formatter,
                "`{name}` in `{module}` is not captured per-invocation yet: {reason}"
            ),
        }
    }
}

impl std::error::Error for SourceFragmentError {}

#[cfg(test)]
mod tests {
    use boltffi_ast::CanonicalName;

    use super::*;

    fn name(spelling: &str) -> boltffi_ast::SourceName {
        boltffi_ast::SourceName::new(spelling, CanonicalName::single(spelling.to_lowercase()))
    }

    fn slot_leaf(index: usize, written: &str) -> TypeExpr {
        TypeExpr::record(
            RecordId::new(format!("{SLOT_ID_PREFIX}{index}")),
            Path::single(written),
        )
    }

    fn record_fragment(fields: Vec<FieldDef>) -> Vec<u8> {
        let mut def = RecordDef::new(RecordId::new(format!("{SELF_ID}::Route")), name("Route"));
        def.fields = fields;
        serde_json::to_vec(&SourceFragment::Record(def)).expect("fragment serializes")
    }

    fn raw(module: &str, slots: &[&str], json: Vec<u8>) -> RawSourceRecord {
        RawSourceRecord {
            package: PackageInfo::new("demo", None),
            module: module.to_owned(),
            slots: slots.iter().map(|slot| (*slot).to_owned()).collect(),
            json,
        }
    }

    fn point_record() -> RawSourceRecord {
        let mut def = RecordDef::new(RecordId::new(format!("{SELF_ID}::Point")), name("Point"));
        def.fields = vec![FieldDef::new(
            name("x"),
            TypeExpr::Primitive(Primitive::F64),
        )];
        raw(
            "demo::geometry",
            &[],
            serde_json::to_vec(&SourceFragment::Record(def)).expect("fragment serializes"),
        )
    }

    #[test]
    fn mints_self_ids_and_resolves_slot_references() {
        let route = raw(
            "demo",
            &[r#"{"id":"demo::geometry::Point"}"#],
            record_fragment(vec![FieldDef::new(name("start"), slot_leaf(0, "Point"))]),
        );

        let contract = aggregate_records(&[route, point_record()], PackageInfo::new("demo", None))
            .expect("records aggregate");

        assert_eq!(
            contract.records.len(),
            2,
            "both records land in the contract"
        );
        assert_eq!(
            contract.records[0].id,
            RecordId::new("demo::Route"),
            "self id is minted from the record's module path"
        );
        let field = &contract.records[0].fields[0];
        match &field.type_expr {
            TypeExpr::Record { id, path } => {
                assert_eq!(id, &RecordId::new("demo::geometry::Point"));
                assert_eq!(
                    path.last().expect("written path kept").name.as_str(),
                    "Point",
                    "the spelling at the use site survives resolution"
                );
            }
            other => panic!("slot leaf did not resolve to a record: {other:?}"),
        }
    }

    #[test]
    fn resolves_a_container_alias_slot_to_its_structure() {
        let route = raw(
            "demo",
            &[r#"{"shape":"Vec","args":[{"id":"demo::geometry::Point"}]}"#],
            record_fragment(vec![FieldDef::new(name("points"), slot_leaf(0, "Points"))]),
        );

        let contract = aggregate_records(&[route, point_record()], PackageInfo::new("demo", None))
            .expect("records aggregate");

        match &contract.records[0].fields[0].type_expr {
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Record { id, .. } => {
                    assert_eq!(id, &RecordId::new("demo::geometry::Point"));
                }
                other => panic!("vec element did not resolve: {other:?}"),
            },
            other => panic!("alias slot did not resolve to a container: {other:?}"),
        }
    }

    #[test]
    fn classifies_references_by_the_defining_fragment() {
        let mut status = EnumDef::new(EnumId::new(format!("{SELF_ID}::Status")), name("Status"));
        status.variants = vec![];
        let status = raw(
            "demo",
            &[],
            serde_json::to_vec(&SourceFragment::Enum(status)).expect("fragment serializes"),
        );
        let holder = raw(
            "demo",
            &[r#"{"id":"demo::Status"}"#],
            record_fragment(vec![FieldDef::new(name("status"), slot_leaf(0, "Status"))]),
        );

        let contract = aggregate_records(&[holder, status], PackageInfo::new("demo", None))
            .expect("records aggregate");

        assert!(
            matches!(
                &contract.records[0].fields[0].type_expr,
                TypeExpr::Enum { id, .. } if id == &EnumId::new("demo::Status")
            ),
            "the reference takes the kind of its defining fragment"
        );
    }

    #[test]
    fn rejects_a_reference_no_record_declares() {
        let route = raw(
            "demo",
            &[r#"{"id":"demo::Missing"}"#],
            record_fragment(vec![FieldDef::new(name("gone"), slot_leaf(0, "Missing"))]),
        );

        let error = aggregate_records(&[route], PackageInfo::new("demo", None))
            .expect_err("missing declaration fails");
        assert!(
            matches!(error, SourceFragmentError::UnresolvedReference { id } if id == "demo::Missing"),
            "the missing id is named"
        );
    }

    #[test]
    fn rejects_conflicting_duplicate_declarations() {
        let first = point_record();
        let mut def = RecordDef::new(RecordId::new(format!("{SELF_ID}::Point")), name("Point"));
        def.fields = vec![FieldDef::new(
            name("x"),
            TypeExpr::Primitive(Primitive::F32),
        )];
        let second = raw(
            "demo::geometry",
            &[],
            serde_json::to_vec(&SourceFragment::Record(def)).expect("fragment serializes"),
        );

        let error = aggregate_records(&[first, second], PackageInfo::new("demo", None))
            .expect_err("conflicting duplicates fail");
        assert!(
            matches!(error, SourceFragmentError::DuplicateDeclaration { id } if id == "demo::geometry::Point")
        );
    }

    #[test]
    fn rejects_duplicate_declarations_whose_slots_differ() {
        let json = record_fragment(vec![FieldDef::new(name("start"), slot_leaf(0, "Alias"))]);
        let first = raw("demo", &[r#"{"prim":"f32"}"#], json.clone());
        let second = raw("demo", &[r#"{"prim":"f64"}"#], json);

        let error = aggregate_records(&[first, second], PackageInfo::new("demo", None))
            .expect_err("identical fragments with differing slots fail");
        assert!(
            matches!(error, SourceFragmentError::DuplicateDeclaration { id } if id == "demo::Route")
        );
    }

    #[test]
    fn refuses_a_set_with_an_unsupported_capture() {
        let unsupported = raw(
            "demo",
            &[],
            serde_json::to_vec(&SourceFragment::Unsupported {
                name: "Listener".to_owned(),
                reason: "callback traits are not captured yet".to_owned(),
            })
            .expect("fragment serializes"),
        );

        let error = aggregate_records(
            &[point_record(), unsupported],
            PackageInfo::new("demo", None),
        )
        .expect_err("incomplete capture refuses the whole set");
        assert!(
            matches!(error, SourceFragmentError::UnsupportedCapture { name, .. } if name == "Listener")
        );
    }

    #[test]
    fn collapses_identical_duplicate_declarations() {
        let contract = aggregate_records(
            &[point_record(), point_record()],
            PackageInfo::new("demo", None),
        )
        .expect("identical duplicates collapse");
        assert_eq!(contract.records.len(), 1, "one declaration survives");
    }
}
