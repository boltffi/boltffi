use serde::Serialize;

use crate::export::ExportFn;
use crate::parse::{Record, TypeNode};

/// Host-side serialization: slots stand in for every type the macro cannot name.
pub fn json(record: &Record) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&Payload {
        kind: "record",
        name: &record.name,
        direct: record.direct,
        fields: record
            .fields
            .iter()
            .map(|field| Field {
                name: &field.name,
                ty: node(&field.ty),
            })
            .collect(),
    })
}

pub fn function_json(function: &ExportFn) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&FunctionPayload {
        kind: "function",
        name: &function.name,
        symbol: &function.symbol,
        params: function
            .params
            .iter()
            .map(|param| Field {
                name: &param.name,
                ty: node(&param.ty),
            })
            .collect(),
        ret: function.ret.as_ref().map(node),
    })
}

fn node(ty: &TypeNode) -> Node<'_> {
    match ty {
        TypeNode::Prim(name) => Node::Prim { prim: name },
        TypeNode::Slot(index) => Node::Slot { slot: *index },
        TypeNode::Shape { name, args } => Node::Shape {
            shape: name,
            args: args.iter().map(node).collect(),
        },
    }
}

#[derive(Serialize)]
struct Payload<'a> {
    kind: &'a str,
    name: &'a str,
    direct: bool,
    fields: Vec<Field<'a>>,
}

#[derive(Serialize)]
struct FunctionPayload<'a> {
    kind: &'a str,
    name: &'a str,
    symbol: &'a str,
    params: Vec<Field<'a>>,
    ret: Option<Node<'a>>,
}

#[derive(Serialize)]
struct Field<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    ty: Node<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Node<'a> {
    Prim { prim: &'a str },
    Slot { slot: usize },
    Shape { shape: &'a str, args: Vec<Node<'a>> },
}
