//! Converts between a Rust surface type (`TypeRef`) and the Dart-side
//! `dart:js_interop` value that crosses to/from the `target::typescript`
//! module this target wraps.
//!
//! This target never re-derives wire encoding, memory layout, or async
//! protocol decisions — `target::typescript` already made those and
//! already ships a tested JS module that does the marshalling. This file
//! only answers "what Dart type does this become, and how do I convert it
//! at the JS boundary" for a given `TypeRef` — the same boundary-level
//! question `target::typescript`'s own declared parameter/return types
//! answer, just phrased in Dart instead of TS.

use boltffi_binding::{BuiltinType, EnumDecl, Primitive, TypeRef};

use crate::core::{Error, RenderContext};
use boltffi_binding::Wasm32;

use super::name_style::Name;

type Result<T> = crate::core::Result<T>;

fn unsupported(shape: &'static str) -> Error {
    Error::UnsupportedTarget {
        target: "dart_web",
        shape,
    }
}

/// The Dart-side type a `TypeRef` value has once it has already crossed
/// the JS boundary (i.e. after `from_js`, or what a Dart-side
/// implementation of a callback method sees/returns directly).
pub fn dart_type(ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => "bool".to_owned(),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32,
        ) => "int".to_owned(),
        TypeRef::Primitive(
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize,
        ) => "int".to_owned(),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => "double".to_owned(),
        TypeRef::String | TypeRef::InternedString { .. } => "String".to_owned(),
        TypeRef::Bytes => "Uint8List".to_owned(),
        TypeRef::Record(id) => context
            .record(*id)
            .map(|decl| Name::new(decl.name()).dart_type_name())
            .ok_or_else(|| unsupported("record reference"))?,
        TypeRef::Enum(id) => context
            .enumeration(*id)
            .map(|decl| Name::new(decl.name()).dart_type_name())
            .ok_or_else(|| unsupported("enum reference"))?,
        TypeRef::Class(id) => context
            .class(*id)
            .map(|decl| Name::new(decl.name()).dart_type_name())
            .ok_or_else(|| unsupported("class reference"))?,
        TypeRef::Callback(id) => context
            .callback(*id)
            .map(|decl| Name::new(decl.name()).dart_type_name())
            .ok_or_else(|| unsupported("callback reference"))?,
        TypeRef::Custom(id) => context
            .custom_type(*id)
            .map(|decl| Name::new(decl.name()).dart_type_name())
            .ok_or_else(|| unsupported("custom type reference"))?,
        TypeRef::Builtin(BuiltinType::Duration) => "Duration".to_owned(),
        TypeRef::Builtin(BuiltinType::SystemTime) => "DateTime".to_owned(),
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => "String".to_owned(),
        TypeRef::Optional(inner) => format!("{}?", dart_type(inner, context)?),
        TypeRef::Sequence(inner) => format!("List<{}>", dart_type(inner, context)?),
        TypeRef::Map { key, value } => {
            format!(
                "Map<{}, {}>",
                dart_type(key, context)?,
                dart_type(value, context)?
            )
        }
        TypeRef::Tuple(elements) => {
            let parts = elements
                .iter()
                .map(|element| dart_type(element, context))
                .collect::<Result<Vec<_>>>()?;
            format!("({})", parts.join(", "))
        }
        _ => return Err(unsupported("dart_web type")),
    })
}

/// Converts a Dart-valued expression into the JS-valued expression the
/// wrapped `target::typescript` module expects as an argument.
pub fn to_js(expr: &str, ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => format!("({expr}).toJS"),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32,
        ) => format!("({expr}).toJS"),
        TypeRef::Primitive(
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize,
        ) => format!("BigInt({expr}).toJS"),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => format!("({expr}).toJS"),
        TypeRef::String | TypeRef::InternedString { .. } => format!("({expr}).toJS"),
        TypeRef::Bytes => format!("({expr}).toJS"),
        // Records and enums generate their own `JSObject/JSAny toJS()`
        // instance method (see `render::Record`/`render::Enumeration`).
        TypeRef::Record(_) | TypeRef::Enum(_) => format!("({expr}).toJS()"),
        // The `Class` wrapper (see `render::Class`) has no conversion
        // method — it just holds the underlying JS instance in `.js`.
        TypeRef::Class(_) => format!("({expr}).js"),
        TypeRef::Callback(id) => {
            let callback = context
                .callback(*id)
                .ok_or_else(|| unsupported("callback"))?;
            let name = Name::new(callback.name()).dart_type_name();
            // Matches the per-callback free function `render::Callback`
            // emits alongside the interface/adapter/wrapper — see its
            // doc comment for why this isn't a single generic helper.
            format!("boltffiCallbackToJS{name}({expr})")
        }
        // A custom type is a bare `typedef` over its representation
        // (see `render::CustomType`) — it has no conversion method of
        // its own, so convert through the representation type instead.
        TypeRef::Custom(id) => {
            let custom = context
                .custom_type(*id)
                .ok_or_else(|| unsupported("custom type"))?;
            to_js(expr, custom.representation(), context)?
        }
        TypeRef::Builtin(BuiltinType::Duration) => format!("boltffiDurationToJS({expr})"),
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => format!("({expr}).toJS"),
        TypeRef::Optional(inner) => {
            let converted = to_js("__boltffiValue", inner, context)?;
            format!(
                "(({expr}) == null ? null : (() {{ final __boltffiValue = ({expr})!; return {converted}; }})())"
            )
        }
        TypeRef::Sequence(inner) => {
            let converted = to_js("__boltffiElement", inner, context)?;
            format!("({expr}).map((__boltffiElement) => {converted}).toList().toJS")
        }
        _ => return Err(unsupported("dart_web to_js type")),
    })
}

/// Converts a JS-valued expression (already cast to `JSAny`/`JSObject` as
/// appropriate) into a Dart value.
pub fn from_js(expr: &str, ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
    Ok(match ty {
        TypeRef::Primitive(Primitive::Bool) => format!("({expr} as JSBoolean).toDart"),
        TypeRef::Primitive(
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32,
        ) => format!("({expr} as JSNumber).toDartInt"),
        TypeRef::Primitive(
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize,
        ) => format!("({expr} as JSBigInt).toDartInt"),
        TypeRef::Primitive(Primitive::F32 | Primitive::F64) => {
            format!("({expr} as JSNumber).toDartDouble")
        }
        TypeRef::String | TypeRef::InternedString { .. } => {
            format!("({expr} as JSString).toDart")
        }
        TypeRef::Bytes => format!("({expr} as JSUint8Array).toDart"),
        TypeRef::Record(id) => {
            let record = context.record(*id).ok_or_else(|| unsupported("record"))?;
            let name = Name::new(record.name()).dart_type_name();
            format!("{name}.fromJS({expr} as JSObject)")
        }
        TypeRef::Enum(id) => {
            let enumeration = context
                .enumeration(*id)
                .ok_or_else(|| unsupported("enum"))?;
            let name = Name::new(enumeration.name()).dart_type_name();
            // C-style enums cross as a bare number; data enums cross as a
            // tagged object.
            let cast = match enumeration {
                EnumDecl::CStyle(_) => "JSAny",
                _ => "JSObject",
            };
            format!("{name}.fromJS({expr} as {cast})")
        }
        TypeRef::Class(id) => {
            let class = context.class(*id).ok_or_else(|| unsupported("class"))?;
            let name = Name::new(class.name()).dart_type_name();
            format!("{name}.fromJS({expr} as JSObject)")
        }
        TypeRef::Custom(id) => {
            let custom = context
                .custom_type(*id)
                .ok_or_else(|| unsupported("custom type"))?;
            from_js(expr, custom.representation(), context)?
        }
        TypeRef::Builtin(BuiltinType::Duration) => {
            format!("boltffiDurationFromJS({expr} as JSObject)")
        }
        TypeRef::Builtin(BuiltinType::Uuid | BuiltinType::Url) => {
            format!("({expr} as JSString).toDart")
        }
        TypeRef::Optional(inner) => {
            let converted = from_js("__boltffiValue", inner, context)?;
            format!(
                "(({expr}) == null ? null : (() {{ final __boltffiValue = ({expr})!; return {converted}; }})())"
            )
        }
        TypeRef::Sequence(inner) => {
            let converted = from_js("__boltffiElement", inner, context)?;
            format!("({expr} as JSArray).toDart.map((__boltffiElement) => {converted}).toList()")
        }
        _ => return Err(unsupported("dart_web from_js type")),
    })
}
