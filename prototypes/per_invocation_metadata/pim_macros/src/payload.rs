use serde::Serialize;

use crate::parse::{Record, TypeNode};

/// Host-side serialization: slots stand in for every type the macro cannot name.
pub fn json(record: &Record) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&Payload {
        kind: "record",
        name: &record.name,
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
    fields: Vec<Field<'a>>,
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
