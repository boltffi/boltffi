use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumDecl,
    EnumId, MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::core::{CustomTypeConversion, Error, RenderContext, Result};

use super::super::syntax::{Expression, Identifier, Statement};
use super::value::{ValueExpression, ValueScope};

pub(in crate::target::csharp) struct Writer<'context, 'bindings> {
    name: Identifier,
    scope: ValueScope,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'context, 'bindings> Writer<'context, 'bindings> {
    pub(in crate::target::csharp) fn new(
        name: Identifier,
        scope: impl Into<ValueScope>,
        context: &'context RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            name,
            scope: scope.into(),
            context,
        }
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value, self.scope.clone()).render()
    }

    fn write(&self, method: &str, value: &ValueRef) -> Result<Statement> {
        Ok(Statement::new(format!(
            "{}.{}({});",
            self.name,
            method,
            self.value(value)?
        )))
    }

    fn encodable(&self, value: &ValueRef) -> Result<Statement> {
        Ok(Statement::new(format!(
            "{}.Encode({});",
            self.value(value)?,
            self.name
        )))
    }

    fn with_scope(
        &mut self,
        scope: ValueScope,
        render: impl FnOnce(&mut Self, &ValueRef) -> Vec<Result<Statement>>,
    ) -> Vec<Result<Statement>> {
        let previous = std::mem::replace(&mut self.scope, scope);
        let statements = render(self, &ValueRef::self_value());
        self.scope = previous;
        statements
    }

    fn c_style_enum_repr(&self, id: EnumId) -> Result<Primitive> {
        match self.context.enumeration(id) {
            Some(EnumDecl::CStyle(enumeration)) => Ok(enumeration.repr().primitive()),
            Some(_) => super::super::unsupported("data enum where C-style enum was expected"),
            None => Err(Error::BrokenBridgeContract {
                bridge: "csharp",
                invariant: "missing enum type in C# codec writer",
            }),
        }
    }
}

impl CodecWrite for Writer<'_, '_> {
    type Stmt = Result<Statement>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(primitive_write_method(primitive), value)]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("WriteString", value)]
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        unreachable!(
            "InternedString codec write reached C# renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("WriteBytes", value)]
    }

    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.encodable(value)]
    }

    fn encoded_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.encodable(value)]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.c_style_enum_repr(id).and_then(|repr| {
            Ok(Statement::new(format!(
                "{}.{}(({}){});",
                self.name,
                primitive_write_method(repr),
                super::super::render::primitive_type(repr),
                self.value(value)?
            )))
        })]
    }

    fn data_enum(&mut self, _: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.encodable(value)]
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![super::super::unsupported("class handle codec write")]
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![super::super::unsupported("callback handle codec write")]
    }

    fn custom<F>(
        &mut self,
        id: CustomTypeId,
        value: &ValueRef,
        representation: F,
    ) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        match self.context.custom_type_mapping(id) {
            Some(mapping) => match self.value(value) {
                Ok(value) => {
                    let representation_value = match mapping.conversion() {
                        CustomTypeConversion::UuidString | CustomTypeConversion::UrlString => {
                            Expression::new(format!("{value}.ToString()"))
                        }
                    };
                    self.with_scope(representation_value.into(), representation)
                }
                Err(error) => vec![Err(error)],
            },
            None => representation(self, value),
        }
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write(
            match kind {
                BuiltinType::Duration => "WriteDuration",
                BuiltinType::SystemTime => "WriteDateTime",
                BuiltinType::Uuid => "WriteGuid",
                BuiltinType::Url => "WriteUri",
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
        vec![self.value(value).and_then(|value| {
            let binder = ValueExpression::binder(binder)?;
            let inner = collect_statements(inner)?;
            Ok(Statement::new(format!(
                "if ({value} is {{ }} {binder})\n{{\n{}\n}}\nelse\n{{\n    {}.WriteU8(0);\n}}",
                indent(&format!("{}.WriteU8(1);\n{inner}", self.name), 4),
                self.name,
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
        vec![self.value(value).and_then(|value| {
            let binder = ValueExpression::binder(binder)?;
            let element = collect_statements(element)?;
            Ok(Statement::new(format!(
                "{}.WriteU32(checked((uint){value}.Length));\nforeach (var {binder} in {value})\n{{\n{}\n}}",
                self.name,
                indent(&element, 4),
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
        vec![self.value(value).and_then(|value| {
            let binder = ValueExpression::binder(binder)?;
            let ok = collect_statements(ok)?;
            let err = collect_statements(err)?;
            Ok(Statement::new(format!(
                "if ({value}.IsOk)\n{{\n{}\n}}\nelse\n{{\n{}\n}}",
                indent(
                    &format!(
                        "{}.WriteU8(0);\nvar {binder} = {value}.OkValue;\n{ok}",
                        self.name
                    ),
                    4
                ),
                indent(
                    &format!(
                        "{}.WriteU8(1);\nvar {binder} = {value}.ErrValue;\n{err}",
                        self.name
                    ),
                    4
                ),
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
        vec![self.value(value).and_then(|value| {
            let entry = Identifier::parse("boltffiEntry")?;
            let key_binder = ValueExpression::binder(key_binder)?;
            let value_binder = ValueExpression::binder(value_binder)?;
            let key = collect_statements(key)?;
            let map_value = collect_statements(map_value)?;
            Ok(Statement::new(format!(
                "{}.WriteU32(checked((uint){value}.Count));\nforeach (var {entry} in {value})\n{{\n{}\n}}",
                self.name,
                indent(
                    &format!(
                        "var {key_binder} = {entry}.Key;\n{key}\nvar {value_binder} = {entry}.Value;\n{map_value}"
                    ),
                    4
                ),
            )))
        })]
    }
}

pub(in crate::target::csharp) fn primitive_write_method(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Bool => "WriteBool",
        Primitive::I8 => "WriteI8",
        Primitive::U8 => "WriteU8",
        Primitive::I16 => "WriteI16",
        Primitive::U16 => "WriteU16",
        Primitive::I32 => "WriteI32",
        Primitive::U32 => "WriteU32",
        Primitive::I64 => "WriteI64",
        Primitive::U64 => "WriteU64",
        Primitive::ISize => "WriteNInt",
        Primitive::USize => "WriteNUInt",
        Primitive::F32 => "WriteF32",
        Primitive::F64 => "WriteF64",
        _ => unreachable!("Primitive is exhaustively matched"),
    }
}

fn collect_statements(statements: Vec<Result<Statement>>) -> Result<String> {
    statements
        .into_iter()
        .collect::<Result<Vec<_>>>()
        .map(|statements| {
            statements
                .into_iter()
                .map(|statement| statement.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })
}

pub(super) fn indent(source: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
