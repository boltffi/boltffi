use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackDecl, DirectValueType, DirectVectorElementType, ExecutionDecl, Primitive, ReturnPlan,
    TypeRef, Wasm32, wasm32,
};

use crate::core::{Emitted, Error, RenderContext, Result};

use super::super::{
    codec::{Sizer, Writer},
    name_style::Name,
    primitive::Scalar,
    syntax::{
        ArgumentList, Expression, Identifier, MemberName, MethodDeclaration, Statement,
        StringLiteral, TypeName,
    },
};

use super::{function::Function, imported::Parameter};

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/callback.ts", escape = "none")]
pub struct Callback {
    name: TypeName,
    registry_name: StringLiteral,
    registry: Identifier,
    register: Identifier,
    unregister: Identifier,
    create_handle: Identifier,
    free_import: StringLiteral,
    clone_import: StringLiteral,
    methods: Vec<Method>,
    async_methods: Vec<AsyncMethod>,
    local: Option<Local>,
}

struct Method {
    name: MemberName,
    import: StringLiteral,
    parameters: Vec<Parameter>,
    public_return: TypeName,
    carrier_return: TypeName,
    invocation: Expression,
    returns_void: bool,
    returns_string: bool,
    returns_encoded: bool,
    returns_direct_record: bool,
    returns_scalar_option: bool,
    scalar_option_pack: Identifier,
    return_pointer: Option<Identifier>,
    encoded_setup: Vec<Statement>,
    fallible: Option<Fallible>,
    vector_return: Option<VectorReturn>,
}

struct Fallible {
    success_setup: Vec<Statement>,
    error_setup: Vec<Statement>,
    encoded_success: bool,
}

struct VectorReturn {
    allocation: Expression,
    write_method: Identifier,
    alignment: usize,
}

struct AsyncMethod {
    name: MemberName,
    import: StringLiteral,
    complete: Identifier,
    parameters: Vec<Parameter>,
    public_return: TypeName,
    invocation: Expression,
    success_setup: Vec<Statement>,
    returns_void: bool,
    fallible: Option<AsyncFallible>,
}

struct AsyncFallible {
    error_setup: Vec<Statement>,
}

struct Local {
    proxy: TypeName,
    wrap: Identifier,
    finalizer: Identifier,
    free: Identifier,
    methods: Vec<MethodDeclaration>,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name());
        let ty = name.type_name();
        Ok(Self {
            registry_name: StringLiteral::new(&ty.to_string()),
            registry: Identifier::parse(format!("_{}Registry", name.identifier()?))?,
            register: Identifier::parse(format!("register{ty}"))?,
            unregister: Identifier::parse(format!("unregister{ty}"))?,
            create_handle: Identifier::parse(
                declaration.protocol().create_handle().name().as_str(),
            )?,
            free_import: StringLiteral::new(declaration.protocol().free().name().as_str()),
            clone_import: StringLiteral::new(declaration.protocol().clone_import().name().as_str()),
            methods: declaration
                .protocol()
                .methods()
                .iter()
                .filter(|method| {
                    matches!(method.callable().execution(), ExecutionDecl::Synchronous(_))
                })
                .map(|method| Method::from_declaration(method, context))
                .collect::<Result<Vec<_>>>()?,
            async_methods: declaration
                .protocol()
                .methods()
                .iter()
                .filter(|method| {
                    matches!(
                        method.callable().execution(),
                        ExecutionDecl::Asynchronous(_)
                    )
                })
                .map(|method| AsyncMethod::from_declaration(method, context))
                .collect::<Result<Vec<_>>>()?,
            local: declaration
                .local_protocol()
                .map(|protocol| Local::from_protocol(protocol, &ty, context))
                .transpose()?,
            name: ty,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?))
    }
}

impl Local {
    fn from_protocol(
        protocol: &boltffi_binding::CallbackLocalProtocol<Wasm32>,
        name: &TypeName,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let target = |function: &boltffi_binding::CallbackLocalFunction| {
            function
                .segments()
                .last()
                .map(|segment| Identifier::parse(segment.as_str()))
                .transpose()?
                .ok_or(Error::UnsupportedTarget {
                    target: "typescript",
                    shape: "callback local target",
                })
        };
        Ok(Self {
            proxy: TypeName::named(format!("{name}Proxy")),
            wrap: Identifier::parse(format!("wrap{name}"))?,
            finalizer: Identifier::parse(format!("_{name}Finalizer"))?,
            free: target(protocol.free())?,
            methods: protocol
                .methods()
                .iter()
                .map(|method| Function::callback_method(method, context))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Method {
    fn from_declaration(
        method: &boltffi_binding::ImportedMethodDecl<Wasm32, boltffi_binding::ImportSymbol>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        if !matches!(method.callable().execution(), ExecutionDecl::Synchronous(_)) {
            return Err(Self::unsupported("callback method execution"));
        }
        let fallible = Self::fallible(method, context)?;
        if fallible.is_none()
            && !matches!(
                method.callable().error().channel(),
                boltffi_binding::ErrorChannel::None
            )
        {
            return Err(Self::unsupported("callback method error"));
        }
        let parameters = method
            .callable()
            .params()
            .iter()
            .map(|parameter| Parameter::from_declaration(parameter, context))
            .collect::<Result<Vec<_>>>()?;
        let return_shape = match &fallible {
            Some((public_type, _)) => ReturnShape::fallible(public_type.clone()),
            None => Self::return_shape(method.callable().returns().plan(), context)?,
        };
        let invocation = Expression::call(
            Expression::identifier(Identifier::known("callback")),
            Name::new(method.name()).identifier()?,
            parameters
                .iter()
                .map(|parameter| parameter.argument.clone())
                .collect::<ArgumentList>(),
        );
        let invocation = match method.callable().returns().plan() {
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(Primitive::Bool),
            } => invocation.conditional(Expression::integer(1), Expression::integer(0)),
            _ => invocation,
        };
        Ok(Self {
            name: Name::new(method.name()).member()?,
            import: StringLiteral::new(method.target().name().as_str()),
            parameters,
            public_return: return_shape.public_type,
            carrier_return: return_shape.carrier_type,
            invocation,
            returns_void: return_shape.returns_void,
            returns_string: return_shape.returns_string,
            returns_encoded: return_shape.return_pointer.is_some() && fallible.is_none(),
            returns_direct_record: return_shape.direct_record,
            returns_scalar_option: return_shape.returns_scalar_option,
            scalar_option_pack: return_shape.scalar_option_pack,
            return_pointer: return_shape.return_pointer,
            encoded_setup: return_shape.setup,
            fallible: fallible.map(|(_, fallible)| fallible),
            vector_return: return_shape.vector_return,
        })
    }

    fn fallible(
        method: &boltffi_binding::ImportedMethodDecl<Wasm32, boltffi_binding::ImportSymbol>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Option<(TypeName, Fallible)>> {
        let boltffi_binding::ErrorDecl::EncodedViaReturnSlot {
            ty: error_type,
            codec: error_codec,
            shape: wasm32::BufferShape::Packed,
        } = method.callable().error()
        else {
            return Ok(None);
        };
        let error_value = Expression::identifier(Identifier::known("error"));
        let error_setup = Self::owned_encoding(error_codec, error_value, context)?;
        let error_type = super::Type::from_ref(error_type, context)?;
        let success_pointer = Identifier::known("successPointer");
        let (success_type, success_setup, encoded_success) =
            match method.callable().returns().plan() {
                ReturnPlan::DirectViaOutPointer {
                    ty: DirectValueType::Primitive(Primitive::I32),
                } => (
                    TypeName::number(),
                    vec![Statement::expression(Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("writeI32"),
                        [
                            Expression::identifier(success_pointer),
                            Expression::identifier(Identifier::known("success")),
                        ]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ))],
                    false,
                ),
                ReturnPlan::DirectViaOutPointer {
                    ty: DirectValueType::Record(id),
                } => {
                    let record = context
                        .record(*id)
                        .ok_or_else(|| Self::unsupported("callback record without declaration"))?;
                    let boltffi_binding::RecordDecl::Direct(record) = record else {
                        return Err(Self::unsupported("encoded callback direct record"));
                    };
                    let writer = Identifier::known("resultWriter");
                    let codec = Name::new(record.name()).codec_identifier()?;
                    (
                        Name::new(record.name()).type_name(),
                        vec![
                            Statement::constant(
                                writer.clone(),
                                Expression::call(
                                    Expression::identifier(Identifier::known("_module")),
                                    Identifier::known("writerFromMemory"),
                                    [
                                        Expression::identifier(success_pointer.clone()),
                                        Expression::integer(record.layout().size().get()),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            Statement::expression(Expression::call(
                                Expression::identifier(codec),
                                Identifier::known("encode"),
                                [
                                    Expression::identifier(writer),
                                    Expression::identifier(Identifier::known("success")),
                                ]
                                .into_iter()
                                .collect(),
                            )),
                        ],
                        false,
                    )
                }
                ReturnPlan::EncodedViaOutPointer {
                    ty,
                    codec,
                    shape: wasm32::BufferShape::Packed,
                } => {
                    let setup = Self::owned_encoding(
                        codec,
                        Expression::identifier(Identifier::known("success")),
                        context,
                    )?;
                    (super::Type::from_ref(ty, context)?, setup, true)
                }
                _ => return Err(Self::unsupported("callback fallible success")),
            };
        Ok(Some((
            TypeName::union(
                success_type.clone(),
                TypeName::union(
                    TypeName::generic("WireResult", [success_type, error_type]),
                    TypeName::named("Error"),
                ),
            ),
            Fallible {
                success_setup,
                error_setup,
                encoded_success,
            },
        )))
    }

    fn owned_encoding(
        codec: &boltffi_binding::WritePlan,
        value: Expression,
        context: &RenderContext<Wasm32>,
    ) -> Result<Vec<Statement>> {
        let size = codec.size_with(&mut Sizer::new(value.clone(), context))?;
        let writer = Identifier::known("resultWriter");
        let writes = codec
            .render_with(&mut Writer::new(writer.clone(), value, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        Ok(std::iter::once(Statement::constant(
            writer,
            Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("allocOwnedWriter"),
                [size.into_expression()]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        ))
        .chain(writes.into_iter().map(|write| write.into_statement()))
        .collect())
    }

    fn return_shape(
        plan: &ReturnPlan<Wasm32, boltffi_binding::IntoRust>,
        context: &RenderContext<Wasm32>,
    ) -> Result<ReturnShape> {
        match plan {
            ReturnPlan::Void => Ok(ReturnShape::void()),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => Ok(ReturnShape::direct(
                Scalar::new(*primitive)?.ty(),
                Parameter::carrier_type(*primitive)?,
            )),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Enum(id),
            } => Ok(ReturnShape::direct(
                context
                    .enumeration(*id)
                    .map(|enumeration| Name::new(enumeration.name()).type_name())
                    .ok_or_else(|| Self::unsupported("callback enum without declaration"))?,
                TypeName::number(),
            )),
            ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Record(id),
            } => ReturnShape::direct_record(*id, context),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: wasm32::BufferShape::Packed,
                ..
            } => Ok(ReturnShape::string()),
            ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => {
                let result = Expression::identifier(Identifier::known("result"));
                let size = codec.size_with(&mut Sizer::new(result.clone(), context))?;
                let writer = Identifier::known("resultWriter");
                let writes = codec
                    .render_with(&mut Writer::new(writer.clone(), result, context))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                Ok(ReturnShape::encoded(
                    super::Type::from_ref(ty, context)?,
                    std::iter::once(Statement::constant(
                        writer,
                        Expression::call(
                            Expression::identifier(Identifier::known("_module")),
                            Identifier::known("allocWriter"),
                            [size.into_expression()]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        ),
                    ))
                    .chain(writes.into_iter().map(|write| write.into_statement()))
                    .collect(),
                ))
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                let option = super::scalar_option::ScalarOption::new(*primitive)?;
                Ok(ReturnShape::scalar_option(
                    Scalar::new(*primitive)?.ty().nullable(),
                    option.carrier_type(),
                    option.pack_method(),
                ))
            }
            ReturnPlan::DirectVecViaReturnSlot { element } => ReturnShape::vector(element, context),
            _ => Err(Self::unsupported("callback method return")),
        }
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl AsyncMethod {
    fn from_declaration(
        method: &boltffi_binding::ImportedMethodDecl<Wasm32, boltffi_binding::ImportSymbol>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::CallbackCompletion { complete }) =
            method.callable().execution()
        else {
            return Err(Method::unsupported("callback async protocol"));
        };
        let parameters = method
            .callable()
            .params()
            .iter()
            .map(|parameter| Parameter::from_declaration(parameter, context))
            .collect::<Result<Vec<_>>>()?;
        let invocation = Expression::call(
            Expression::identifier(Identifier::known("callback")),
            Name::new(method.name()).identifier()?,
            parameters
                .iter()
                .map(|parameter| parameter.argument.clone())
                .collect::<ArgumentList>(),
        );
        let (public_return, success_setup, returns_void, fallible) = match method.callable().error()
        {
            boltffi_binding::ErrorDecl::None(_) => {
                let (public_return, success_setup, returns_void) =
                    Self::infallible_return(method.callable().returns().plan(), context)?;
                (public_return, success_setup, returns_void, None)
            }
            boltffi_binding::ErrorDecl::EncodedViaReturnSlot {
                ty: error_type,
                codec: error_codec,
                shape: wasm32::BufferShape::Packed,
            } => {
                let (success, success_setup) = match method.callable().returns().plan() {
                    ReturnPlan::EncodedViaOutPointer {
                        ty,
                        codec,
                        shape: wasm32::BufferShape::Packed,
                    } => (
                        super::Type::from_ref(ty, context)?,
                        Self::encoding(
                            codec,
                            Expression::identifier(Identifier::known("success")),
                            context,
                        )?,
                    ),
                    ReturnPlan::DirectViaOutPointer {
                        ty: DirectValueType::Record(id),
                    } => (
                        context
                            .record(*id)
                            .map(|record| Name::new(record.name()).type_name())
                            .ok_or_else(|| {
                                Method::unsupported("callback record without declaration")
                            })?,
                        Self::record_encoding(
                            *id,
                            Expression::identifier(Identifier::known("success")),
                            context,
                        )?,
                    ),
                    _ => {
                        return Err(Method::unsupported("callback async fallible success"));
                    }
                };
                let error = super::Type::from_ref(error_type, context)?;
                (
                    TypeName::union(
                        success.clone(),
                        TypeName::union(
                            TypeName::generic("WireResult", [success, error]),
                            TypeName::named("Error"),
                        ),
                    ),
                    success_setup,
                    false,
                    Some(AsyncFallible {
                        error_setup: Self::encoding(
                            error_codec,
                            Expression::identifier(Identifier::known("error")),
                            context,
                        )?,
                    }),
                )
            }
            _ => return Err(Method::unsupported("callback async error")),
        };
        Ok(Self {
            name: Name::new(method.name()).member()?,
            import: StringLiteral::new(method.target().name().as_str()),
            complete: Identifier::parse(complete.name().as_str())?,
            parameters,
            public_return,
            invocation,
            success_setup,
            returns_void,
            fallible,
        })
    }

    fn infallible_return(
        plan: &ReturnPlan<Wasm32, boltffi_binding::IntoRust>,
        context: &RenderContext<Wasm32>,
    ) -> Result<(TypeName, Vec<Statement>, bool)> {
        match plan {
            ReturnPlan::Void => Ok((TypeName::void(), Vec::new(), true)),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let scalar = Scalar::new(*primitive)?;
                let writer = Identifier::known("resultWriter");
                Ok((
                    scalar.ty(),
                    vec![
                        Statement::constant(
                            writer.clone(),
                            Expression::call(
                                Expression::identifier(Identifier::known("_module")),
                                Identifier::known("allocWriter"),
                                [Expression::integer(primitive.byte_size::<Wasm32>().get())]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                        ),
                        Statement::expression(Expression::call(
                            Expression::identifier(writer),
                            scalar.write_method(),
                            [Expression::identifier(Identifier::known("result"))]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        )),
                    ],
                    false,
                ))
            }
            ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec,
                shape: wasm32::BufferShape::Packed,
            } => Ok((
                super::Type::from_ref(ty, context)?,
                Self::encoding(
                    codec,
                    Expression::identifier(Identifier::known("result")),
                    context,
                )?,
                false,
            )),
            ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Record(id),
            } => Ok((
                context
                    .record(*id)
                    .map(|record| Name::new(record.name()).type_name())
                    .ok_or_else(|| Method::unsupported("callback record without declaration"))?,
                Self::record_encoding(
                    *id,
                    Expression::identifier(Identifier::known("result")),
                    context,
                )?,
                false,
            )),
            _ => Err(Method::unsupported("callback async return")),
        }
    }

    fn encoding(
        codec: &boltffi_binding::WritePlan,
        value: Expression,
        context: &RenderContext<Wasm32>,
    ) -> Result<Vec<Statement>> {
        let size = codec.size_with(&mut Sizer::new(value.clone(), context))?;
        let writer = Identifier::known("resultWriter");
        let writes = codec
            .render_with(&mut Writer::new(writer.clone(), value, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        Ok(std::iter::once(Statement::constant(
            writer,
            Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known("allocWriter"),
                [size.into_expression()]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        ))
        .chain(writes.into_iter().map(|write| write.into_statement()))
        .collect())
    }

    fn record_encoding(
        id: boltffi_binding::RecordId,
        value: Expression,
        context: &RenderContext<Wasm32>,
    ) -> Result<Vec<Statement>> {
        let record = context
            .record(id)
            .ok_or_else(|| Method::unsupported("callback record without declaration"))?;
        let boltffi_binding::RecordDecl::Direct(record) = record else {
            return Err(Method::unsupported("encoded callback direct record"));
        };
        let writer = Identifier::known("resultWriter");
        Ok(vec![
            Statement::constant(
                writer.clone(),
                Expression::call(
                    Expression::identifier(Identifier::known("_module")),
                    Identifier::known("allocWriter"),
                    [Expression::integer(record.layout().size().get())]
                        .into_iter()
                        .collect(),
                ),
            ),
            Statement::expression(Expression::call(
                Expression::identifier(Name::new(record.name()).codec_identifier()?),
                Identifier::known("encode"),
                [Expression::identifier(writer), value]
                    .into_iter()
                    .collect(),
            )),
        ])
    }
}

struct ReturnShape {
    public_type: TypeName,
    carrier_type: TypeName,
    returns_void: bool,
    returns_string: bool,
    returns_scalar_option: bool,
    scalar_option_pack: Identifier,
    return_pointer: Option<Identifier>,
    setup: Vec<Statement>,
    vector_return: Option<VectorReturn>,
    direct_record: bool,
}

impl ReturnShape {
    fn void() -> Self {
        Self {
            public_type: TypeName::void(),
            carrier_type: TypeName::void(),
            returns_void: true,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: None,
            setup: Vec::new(),
            vector_return: None,
            direct_record: false,
        }
    }

    fn direct(public_type: TypeName, carrier_type: TypeName) -> Self {
        Self {
            public_type,
            carrier_type,
            returns_void: false,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: None,
            setup: Vec::new(),
            vector_return: None,
            direct_record: false,
        }
    }

    fn string() -> Self {
        Self {
            public_type: TypeName::string(),
            carrier_type: TypeName::bigint(),
            returns_void: false,
            returns_string: true,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: None,
            setup: Vec::new(),
            vector_return: None,
            direct_record: false,
        }
    }

    fn encoded(public_type: TypeName, setup: Vec<Statement>) -> Self {
        Self {
            public_type,
            carrier_type: TypeName::void(),
            returns_void: false,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: Some(Identifier::known("resultPointer")),
            setup,
            vector_return: None,
            direct_record: false,
        }
    }

    fn scalar_option(public_type: TypeName, carrier_type: TypeName, pack: Identifier) -> Self {
        Self {
            public_type,
            carrier_type,
            returns_void: false,
            returns_string: false,
            returns_scalar_option: true,
            scalar_option_pack: pack,
            return_pointer: None,
            setup: Vec::new(),
            vector_return: None,
            direct_record: false,
        }
    }

    fn fallible(public_type: TypeName) -> Self {
        Self {
            public_type,
            carrier_type: TypeName::bigint(),
            returns_void: false,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: Some(Identifier::known("successPointer")),
            setup: Vec::new(),
            vector_return: None,
            direct_record: false,
        }
    }

    fn vector(element: &DirectVectorElementType, context: &RenderContext<Wasm32>) -> Result<Self> {
        let vector = super::direct_vector::DirectVector::outgoing(element, context)?;
        Ok(Self {
            public_type: vector.return_type(),
            carrier_type: TypeName::void(),
            returns_void: false,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: None,
            setup: Vec::new(),
            vector_return: Some(VectorReturn {
                allocation: vector.allocation(Expression::identifier(Identifier::known("result"))),
                write_method: vector.return_slot_method(),
                alignment: vector.alignment(),
            }),
            direct_record: false,
        })
    }

    fn direct_record(
        id: boltffi_binding::RecordId,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let record = context
            .record(id)
            .ok_or_else(|| Method::unsupported("callback record without declaration"))?;
        let boltffi_binding::RecordDecl::Direct(record) = record else {
            return Err(Method::unsupported("encoded callback direct record"));
        };
        let pointer = Identifier::known("resultPointer");
        let writer = Identifier::known("resultWriter");
        let codec = Name::new(record.name()).codec_identifier()?;
        let size = record.layout().size().get();
        Ok(Self {
            public_type: Name::new(record.name()).type_name(),
            carrier_type: TypeName::void(),
            returns_void: false,
            returns_string: false,
            returns_scalar_option: false,
            scalar_option_pack: Identifier::known("packOptionScalar"),
            return_pointer: Some(pointer.clone()),
            setup: vec![
                Statement::constant(
                    writer.clone(),
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("writerFromMemory"),
                        [Expression::identifier(pointer), Expression::integer(size)]
                            .into_iter()
                            .collect(),
                    ),
                ),
                Statement::expression(Expression::call(
                    Expression::identifier(codec),
                    Identifier::known("encode"),
                    [
                        Expression::identifier(writer),
                        Expression::identifier(Identifier::known("result")),
                    ]
                    .into_iter()
                    .collect(),
                )),
            ],
            vector_return: None,
            direct_record: true,
        })
    }
}
