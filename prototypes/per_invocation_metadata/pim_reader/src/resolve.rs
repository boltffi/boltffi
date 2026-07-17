use std::collections::BTreeMap;
use std::fmt;

use pim_runtime::RawRecord;
use serde::Deserialize;

/// Everything the artifact declared, with type references resolved to canonical ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub items: Vec<Item>,
    pub functions: Vec<Function>,
    pub scaffolding: Vec<Scaffolding>,
}

/// One crate's `scaffolding!()` record: crate-level protocol symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scaffolding {
    pub module: String,
    pub free_symbol: String,
}

/// A type record whose references have been resolved to canonical ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub canonical_id: String,
    pub name: String,
    pub module: String,
    pub direct: bool,
    pub fields: Vec<Field>,
}

/// An exported function: the symbol is the link-layer name, the canonical id the readable one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub canonical_id: String,
    pub name: String,
    pub module: String,
    pub symbol: String,
    pub params: Vec<Field>,
    pub ret: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: String,
}

/// Joins each record's module path with its slot table, keyed by canonical id.
pub fn resolve(records: &[RawRecord]) -> Result<Resolved, ResolveError> {
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    let mut functions: BTreeMap<String, Function> = BTreeMap::new();
    let mut scaffolding: BTreeMap<String, Scaffolding> = BTreeMap::new();

    for record in records {
        match payload(record)? {
            Payload::Record(payload) => {
                let item = item(record, payload)?;
                if let Some(existing) = items.get(&item.canonical_id)
                    && existing != &item
                {
                    return Err(ResolveError::Conflict {
                        id: item.canonical_id,
                    });
                }
                items.insert(item.canonical_id.clone(), item);
            }
            Payload::Function(payload) => {
                let function = function(record, payload)?;
                if let Some(existing) = functions.get(&function.canonical_id)
                    && existing != &function
                {
                    return Err(ResolveError::Conflict {
                        id: function.canonical_id,
                    });
                }
                functions.insert(function.canonical_id.clone(), function);
            }
            Payload::Scaffolding(payload) => {
                let entry = Scaffolding {
                    module: record.module.clone(),
                    free_symbol: payload.free_symbol,
                };
                if let Some(existing) = scaffolding.get(&entry.module)
                    && existing != &entry
                {
                    return Err(ResolveError::Conflict { id: entry.module });
                }
                scaffolding.insert(entry.module.clone(), entry);
            }
        }
    }

    Ok(Resolved {
        items: items.into_values().collect(),
        functions: functions.into_values().collect(),
        scaffolding: scaffolding.into_values().collect(),
    })
}

fn payload(record: &RawRecord) -> Result<Payload, ResolveError> {
    serde_json::from_slice(&record.json).map_err(|error| ResolveError::Json(error.to_string()))
}

fn item(record: &RawRecord, payload: RecordPayload) -> Result<Item, ResolveError> {
    Ok(Item {
        canonical_id: format!("{}::{}", record.module, payload.name),
        fields: fields(&payload.fields, &record.slots)?,
        name: payload.name,
        module: record.module.clone(),
        direct: payload.direct,
    })
}

fn function(record: &RawRecord, payload: FunctionPayload) -> Result<Function, ResolveError> {
    Ok(Function {
        canonical_id: format!("{}::{}", record.module, payload.name),
        params: fields(&payload.params, &record.slots)?,
        ret: payload
            .ret
            .as_ref()
            .map(|node| render(node, &record.slots))
            .transpose()?,
        name: payload.name,
        module: record.module.clone(),
        symbol: payload.symbol,
    })
}

fn fields(payload: &[PayloadField], slots: &[String]) -> Result<Vec<Field>, ResolveError> {
    payload
        .iter()
        .map(|field| {
            Ok(Field {
                name: field.name.clone(),
                ty: render(&field.ty, slots)?,
            })
        })
        .collect()
}

fn render(node: &TypeNode, slots: &[String]) -> Result<String, ResolveError> {
    match node {
        TypeNode::Prim { prim } => Ok(prim.clone()),
        TypeNode::Slot { slot } => {
            let module = slots.get(slot * 2);
            let name = slots.get(slot * 2 + 1);
            match (module, name) {
                (Some(module), Some(name)) => Ok(format!("{module}::{name}")),
                _ => Err(ResolveError::MissingSlot {
                    index: *slot,
                    available: slots.len() / 2,
                }),
            }
        }
        TypeNode::Shape { shape, args } => {
            let args = args
                .iter()
                .map(|arg| render(arg, slots))
                .collect::<Result<Vec<_>, ResolveError>>()?
                .join(", ");
            Ok(format!("{shape}<{args}>"))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Payload {
    Record(RecordPayload),
    Function(FunctionPayload),
    Scaffolding(ScaffoldingPayload),
}

#[derive(Debug, Deserialize)]
struct ScaffoldingPayload {
    free_symbol: String,
}

#[derive(Debug, Deserialize)]
struct RecordPayload {
    name: String,
    #[serde(default)]
    direct: bool,
    fields: Vec<PayloadField>,
}

#[derive(Debug, Deserialize)]
struct FunctionPayload {
    name: String,
    symbol: String,
    params: Vec<PayloadField>,
    ret: Option<TypeNode>,
}

#[derive(Debug, Deserialize)]
struct PayloadField {
    name: String,
    #[serde(rename = "type")]
    ty: TypeNode,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TypeNode {
    Prim { prim: String },
    Slot { slot: usize },
    Shape { shape: String, args: Vec<TypeNode> },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    Json(String),
    MissingSlot { index: usize, available: usize },
    Conflict { id: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "record payload is not valid json: {error}"),
            Self::MissingSlot { index, available } => write!(
                formatter,
                "record references slot {index} but declares {available}"
            ),
            Self::Conflict { id } => {
                write!(formatter, "two different records both claim `{id}`")
            }
        }
    }
}

impl std::error::Error for ResolveError {}
