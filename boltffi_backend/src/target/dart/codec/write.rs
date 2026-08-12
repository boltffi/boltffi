use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId,
    MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::core::{RenderContext, Result};

use super::{CStyleEnumRepresentation, ValueScope, primitive_write_method, value::binder_name};

pub struct Writer<'context, 'bindings> {
    name: String,
    scope: ValueScope,
    context: &'context RenderContext<'bindings, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteStatement {
    source: String,
    value: WriteValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteValue {
    String,
    Other,
}

impl<'context, 'bindings> Writer<'context, 'bindings> {
    pub fn new(
        name: impl Into<String>,
        scope: ValueScope,
        context: &'context RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            name: name.into(),
            scope,
            context,
        }
    }

    fn write(&self, method: &str, value: &ValueRef) -> Result<WriteStatement> {
        Ok(WriteStatement::new(format!(
            "{}.{}({});",
            self.name,
            method,
            self.scope.value(value)?
        )))
    }
}

impl WriteStatement {
    pub fn into_source(self) -> String {
        self.source
    }

    fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            value: WriteValue::Other,
        }
    }

    fn string(mut self) -> Self {
        self.value = WriteValue::String;
        self
    }

    fn custom_representation(mut self) -> Self {
        self.value = WriteValue::Other;
        self
    }

    fn result_error(&self, writer: &str, binder: &str) -> String {
        match self.value {
            WriteValue::String => format!("{writer}.writeString({binder}.message);"),
            WriteValue::Other => self.source.clone(),
        }
    }
}

impl CodecWrite for Writer<'_, '_> {
    type Stmt = Result<WriteStatement>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(primitive_write_method(primitive), value)]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("writeString", value).map(WriteStatement::string)]
    }

    fn interned_string(&mut self, _: &[String], _: &ValueRef) -> Vec<Self::Stmt> {
        unreachable!("InternedString codec write reached Dart renderer without host capability")
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("writeBytes", value)]
    }

    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.scope
                .value(value)
                .map(|value| WriteStatement::new(format!("{value}._m$wireEncode({});", self.name))),
        ]
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.direct_record(id, value)
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.scope.value(value).and_then(|value| {
            let representation = CStyleEnumRepresentation::resolve(id, self.context)?;
            Ok(WriteStatement::new(format!(
                "{}.{}({value}.value);",
                self.name,
                representation.write_method()
            )))
        })]
    }

    fn data_enum(&mut self, _: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.scope
                .value(value)
                .map(|value| WriteStatement::new(format!("{value}._m$wireEncode({});", self.name))),
        ]
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![super::super::unsupported("class handle in encoded payload")]
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![super::super::unsupported(
            "callback handle in encoded payload",
        )]
    }

    fn custom<F>(&mut self, _: CustomTypeId, value: &ValueRef, representation: F) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        representation(self, value)
            .into_iter()
            .map(|statement| statement.map(WriteStatement::custom_representation))
            .collect()
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(
            match kind {
                BuiltinType::Duration => "writeDuration",
                BuiltinType::SystemTime => "writeInstant",
                BuiltinType::Uuid => "writeUUID",
                BuiltinType::Url => "writeUri",
            },
            value,
        )]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.scope.value(value).and_then(|value| {
            Ok(WriteStatement::new(format!(
                "if ({value} == null) {{\n  {}.writeU8(0);\n}} else {{\n  {}.writeU8(1);\n  final {} = {value}!;\n{}\n}}",
                self.name,
                self.name,
                binder_name(binder),
                indent(
                    inner
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(WriteStatement::into_source)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    2
                ),
            )))
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.scope.value(value).and_then(|value| {
            Ok(WriteStatement::new(format!(
                "{}.writeU32({value}.length);\nfor (final {} in {value}) {{\n{}\n}}",
                self.name,
                binder_name(binder),
                indent(
                    element
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(WriteStatement::into_source)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    2
                ),
            )))
        })]
    }

    fn tuple(&mut self, _: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        elements.into_iter().flatten().collect()
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.scope.value(value).and_then(|value| {
            let binder = binder_name(binder);
            let ok = ok
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(WriteStatement::into_source)
                .collect::<Vec<_>>()
                .join("\n");
            let err = err
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|statement| statement.result_error(&self.name, &binder))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(WriteStatement::new(format!(
                "switch ({value}) {{\n  case $$BoltResult$Ok(value: final {binder}):\n    {}.writeU8(0);\n{}\n  case $$BoltResult$Err(value: final {binder}):\n    {}.writeU8(1);\n{}\n}}",
                self.name,
                indent(ok, 4),
                self.name,
                indent(err, 4),
            )))
        })]
    }

    fn map(
        &mut self,
        _: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.scope.value(value).and_then(|value| {
            Ok(WriteStatement::new(format!(
                "{}.writeU32({value}.length);\nfor (final _l$entry in {value}.entries) {{\n  final {} = _l$entry.key;\n  final {} = _l$entry.value;\n{}\n{}\n}}",
                self.name,
                binder_name(key_binder),
                binder_name(value_binder),
                indent(
                    key.into_iter()
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(WriteStatement::into_source)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    2
                ),
                indent(
                    map_value
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .map(WriteStatement::into_source)
                        .collect::<Vec<_>>()
                        .join("\n"),
                    2
                ),
            )))
        })]
    }
}

fn indent(text: String, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
