use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumDecl, EnumId,
    MapKind, Native, Op, Primitive, RecordId,
};

use crate::{
    core::{CustomTypeConversion, Error, RenderContext, Result},
    target::csharp::{
        name_style::Namespace,
        render::primitive_type,
        syntax::{Expression, Identifier, TypeFragment},
        type_name,
    },
};

pub(in crate::target::csharp) struct Reader<'context, 'bindings> {
    name: Identifier,
    context: &'context RenderContext<'bindings, Native>,
    namespace: Option<Namespace>,
}

pub(in crate::target::csharp) struct ReadExpression {
    expression: Expression,
    ty: TypeFragment,
}

impl<'context, 'bindings> Reader<'context, 'bindings> {
    pub(in crate::target::csharp) fn new(
        name: Identifier,
        context: &'context RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            name,
            context,
            namespace: None,
        }
    }

    pub(in crate::target::csharp) fn qualified(mut self, namespace: &Namespace) -> Self {
        self.namespace = Some(namespace.clone());
        self
    }

    fn call(&self, method: &str, ty: TypeFragment) -> ReadExpression {
        ReadExpression::new(Expression::new(format!("{}.{}()", self.name, method)), ty)
    }

    fn c_style_enum_repr(&self, id: EnumId) -> Result<Primitive> {
        match self.context.enumeration(id) {
            Some(EnumDecl::CStyle(enumeration)) => Ok(enumeration.repr().primitive()),
            Some(_) => super::super::unsupported("data enum where C-style enum was expected"),
            None => Err(Error::BrokenBridgeContract {
                bridge: "csharp",
                invariant: "missing enum type in C# codec reader",
            }),
        }
    }

    fn record_type(&self, id: RecordId) -> Result<TypeFragment> {
        let ty = type_name::record(id, self.context)?;
        Ok(self.namespace.as_ref().map_or(ty.clone(), |namespace| {
            TypeFragment::new(format!("global::{namespace}.{ty}"))
        }))
    }

    fn enum_type(&self, id: EnumId) -> Result<TypeFragment> {
        let ty = type_name::enumeration(id, self.context)?;
        Ok(self.namespace.as_ref().map_or(ty.clone(), |namespace| {
            TypeFragment::new(format!("global::{namespace}.{ty}"))
        }))
    }
}

impl ReadExpression {
    pub(in crate::target::csharp) fn into_expression(self) -> Expression {
        self.expression
    }

    fn new(expression: Expression, ty: TypeFragment) -> Self {
        Self { expression, ty }
    }
}

impl CodecRead for Reader<'_, '_> {
    type Expr = Result<ReadExpression>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        Ok(self.call(primitive_read_method(primitive), primitive_type(primitive)))
    }

    fn string(&mut self) -> Self::Expr {
        Ok(self.call("ReadString", TypeFragment::new("string")))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        unreachable!(
            "InternedString codec read reached C# renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(self.call("ReadBytes", TypeFragment::new("byte[]")))
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        let ty = self.record_type(id)?;
        Ok(ReadExpression::new(
            Expression::new(format!("{ty}.Decode({})", self.name)),
            ty,
        ))
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        let ty = self.record_type(id)?;
        Ok(ReadExpression::new(
            Expression::new(format!("{ty}.Decode({})", self.name)),
            ty,
        ))
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        let ty = self.enum_type(id)?;
        let read = primitive_read_method(self.c_style_enum_repr(id)?);
        Ok(ReadExpression::new(
            Expression::new(format!("({ty}){}.{}()", self.name, read)),
            ty,
        ))
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        let ty = self.enum_type(id)?;
        Ok(ReadExpression::new(
            Expression::new(format!("{ty}.Decode({})", self.name)),
            ty,
        ))
    }

    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        super::super::unsupported("class handle codec read")
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        super::super::unsupported("callback handle codec read")
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        match self.context.custom_type_mapping(id) {
            Some(mapping) => {
                let target = TypeFragment::new(mapping.target_type().as_str());
                let expression = match mapping.conversion() {
                    CustomTypeConversion::UuidString => {
                        Expression::new(format!("new {target}({})", representation.expression))
                    }
                    CustomTypeConversion::UrlString => {
                        Expression::new(format!("new {target}({})", representation.expression))
                    }
                };
                Ok(ReadExpression::new(expression, target))
            }
            None => Ok(representation),
        }
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        let (method, ty) = match kind {
            BuiltinType::Duration => ("ReadDuration", "global::System.TimeSpan"),
            BuiltinType::SystemTime => ("ReadDateTime", "global::System.DateTime"),
            BuiltinType::Uuid => ("ReadGuid", "global::System.Guid"),
            BuiltinType::Url => ("ReadUri", "global::System.Uri"),
        };
        Ok(self.call(method, TypeFragment::new(ty)))
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        let inner = inner?;
        let ty = TypeFragment::new(format!("{}?", inner.ty));
        Ok(ReadExpression::new(
            Expression::new(format!(
                "{}.ReadU8() == 0 ? default({ty}) : {}",
                self.name, inner.expression
            )),
            ty,
        ))
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        let ty = TypeFragment::new(format!("{}[]", element.ty));
        Ok(ReadExpression::new(
            Expression::new(format!(
                "{}.ReadArray(reader => {})",
                self.name, element.expression
            )),
            ty,
        ))
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        let elements = elements.into_iter().collect::<Result<Vec<_>>>()?;
        let ty = TypeFragment::new(format!(
            "({})",
            elements
                .iter()
                .map(|element| element.ty.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        Ok(ReadExpression::new(
            Expression::new(format!(
                "({})",
                elements
                    .iter()
                    .map(|element| element.expression.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            ty,
        ))
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        let ok = ok?;
        let err = err?;
        let ty = TypeFragment::new(format!("BoltFFIResult<{}, {}>", ok.ty, err.ty));
        Ok(ReadExpression::new(
            Expression::new(format!(
                "{}.ReadResult(reader => {}, reader => {})",
                self.name, ok.expression, err.expression
            )),
            ty,
        ))
    }

    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        let key = key?;
        let value = value?;
        let ty = TypeFragment::new(format!(
            "global::System.Collections.Generic.Dictionary<{}, {}>",
            key.ty, value.ty
        ));
        Ok(ReadExpression::new(
            Expression::new(format!(
                "{}.ReadMap(reader => {}, reader => {})",
                self.name, key.expression, value.expression
            )),
            ty,
        ))
    }
}

pub(in crate::target::csharp) fn primitive_read_method(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Bool => "ReadBool",
        Primitive::I8 => "ReadI8",
        Primitive::U8 => "ReadU8",
        Primitive::I16 => "ReadI16",
        Primitive::U16 => "ReadU16",
        Primitive::I32 => "ReadI32",
        Primitive::U32 => "ReadU32",
        Primitive::I64 => "ReadI64",
        Primitive::U64 => "ReadU64",
        Primitive::ISize => "ReadNInt",
        Primitive::USize => "ReadNUInt",
        Primitive::F32 => "ReadF32",
        Primitive::F64 => "ReadF64",
        _ => unreachable!("Primitive is exhaustively matched"),
    }
}
