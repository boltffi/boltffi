use serde::Serialize;

use crate::callback::CallbackFn;
use crate::class::MethodFn;
use crate::export::ExportFn;
use crate::parse::{Record, TypeNode};
use crate::stream::StreamExport;

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

pub fn scaffolding_json(free_symbol: &str) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&ScaffoldingPayload {
        kind: "scaffolding",
        free_symbol,
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

pub fn class_json(
    name: &str,
    constructors: &[&MethodFn],
    methods: &[&MethodFn],
    free_symbol: &str,
) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&ClassPayload {
        kind: "class",
        name,
        constructors: constructors.iter().copied().map(method_payload).collect(),
        methods: methods.iter().copied().map(method_payload).collect(),
        free_symbol,
    })
}

fn method_payload(meta: &MethodFn) -> MethodPayload<'_> {
    MethodPayload {
        name: &meta.name,
        symbol: &meta.symbol,
        params: meta
            .params
            .iter()
            .map(|param| Field {
                name: &param.name,
                ty: node(&param.ty),
            })
            .collect(),
        ret: meta.ret.as_ref().map(node),
    }
}

pub fn callback_json(name: &str, methods: &[&CallbackFn]) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&CallbackPayload {
        kind: "callback",
        name,
        methods: methods
            .iter()
            .map(|meta| CallbackMethodPayload {
                name: &meta.name,
                params: meta
                    .params
                    .iter()
                    .map(|param| Field {
                        name: &param.name,
                        ty: node(&param.ty),
                    })
                    .collect(),
                ret: meta.ret.as_ref().map(node),
            })
            .collect(),
    })
}

pub fn stream_json(stream: &StreamExport) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&StreamPayload {
        kind: "stream",
        name: &stream.name,
        subscribe_symbol: &stream.subscribe_symbol,
        pop_symbol: &stream.pop_symbol,
        free_symbol: &stream.free_symbol,
        params: stream
            .params
            .iter()
            .map(|param| Field {
                name: &param.name,
                ty: node(&param.ty),
            })
            .collect(),
        item: node(&stream.item),
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
struct ScaffoldingPayload<'a> {
    kind: &'a str,
    free_symbol: &'a str,
}

#[derive(Serialize)]
struct ClassPayload<'a> {
    kind: &'a str,
    name: &'a str,
    constructors: Vec<MethodPayload<'a>>,
    methods: Vec<MethodPayload<'a>>,
    free_symbol: &'a str,
}

#[derive(Serialize)]
struct MethodPayload<'a> {
    name: &'a str,
    symbol: &'a str,
    params: Vec<Field<'a>>,
    ret: Option<Node<'a>>,
}

#[derive(Serialize)]
struct CallbackPayload<'a> {
    kind: &'a str,
    name: &'a str,
    methods: Vec<CallbackMethodPayload<'a>>,
}

#[derive(Serialize)]
struct CallbackMethodPayload<'a> {
    name: &'a str,
    params: Vec<Field<'a>>,
    ret: Option<Node<'a>>,
}

#[derive(Serialize)]
struct StreamPayload<'a> {
    kind: &'a str,
    name: &'a str,
    subscribe_symbol: &'a str,
    pop_symbol: &'a str,
    free_symbol: &'a str,
    params: Vec<Field<'a>>,
    item: Node<'a>,
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
