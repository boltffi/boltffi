use std::collections::BTreeMap;
use std::fmt;

use pim_runtime::RawRecord;
use serde::Deserialize;

/// A record whose type references have been resolved to canonical ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub canonical_id: String,
    pub name: String,
    pub module: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: String,
}

/// Joins each record's module path with its slot table, keyed by canonical id.
pub fn resolve(records: &[RawRecord]) -> Result<Vec<Item>, ResolveError> {
    let mut items: BTreeMap<String, Item> = BTreeMap::new();

    for record in records {
        let item = item(record)?;
        if let Some(existing) = items.get(&item.canonical_id)
            && existing != &item
        {
            return Err(ResolveError::Conflict {
                id: item.canonical_id,
            });
        }
        items.insert(item.canonical_id.clone(), item);
    }

    Ok(items.into_values().collect())
}

fn item(record: &RawRecord) -> Result<Item, ResolveError> {
    let payload: Payload = serde_json::from_slice(&record.json)
        .map_err(|error| ResolveError::Json(error.to_string()))?;

    let fields = payload
        .fields
        .iter()
        .map(|field| {
            Ok(Field {
                name: field.name.clone(),
                ty: render(&field.ty, &record.slots)?,
            })
        })
        .collect::<Result<Vec<_>, ResolveError>>()?;

    Ok(Item {
        canonical_id: format!("{}::{}", record.module, payload.name),
        name: payload.name,
        module: record.module.clone(),
        fields,
    })
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
struct Payload {
    name: String,
    fields: Vec<PayloadField>,
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
