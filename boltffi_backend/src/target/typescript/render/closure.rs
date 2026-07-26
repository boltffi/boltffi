use askama::Template as AskamaTemplate;
use boltffi_binding::{
    DirectValueType, OutgoingParam, ParamPlan, Primitive, ReturnPlan, TypeRef, Wasm32,
    WasmIncomingClosure, wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    codec::{Sizer, Writer},
    name_style::Name,
    primitive::Scalar,
    syntax::{ArgumentList, Expression, Identifier, Statement, StringLiteral, TypeName},
};
use super::imported::Parameter as ImportedParameter;

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/closure.ts", escape = "none")]
pub struct ClosureAdapter {
    name: TypeName,
    registry_name: StringLiteral,
    registry: Identifier,
    register: Identifier,
    unregister: Identifier,
    call_import: StringLiteral,
    free_import: StringLiteral,
    parameters: Vec<Parameter>,
    public_return: TypeName,
    carrier_return: TypeName,
    invocation: Expression,
    returns_void: bool,
    returns_string: bool,
    returns_encoded: bool,
    returns_direct_record: bool,
    return_pointer: Option<Identifier>,
    encoded_setup: Vec<Statement>,
    fallible: Option<Fallible>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Parameter {
    signature: String,
    imported: ImportedParameter,
}

struct Fallible {
    success_pointer: Identifier,
    success_write: Identifier,
    error_setup: Vec<Statement>,
}

impl ClosureAdapter {
    pub fn from_closure(
        closure: WasmIncomingClosure<'_>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Option<Self>> {
        let Some(parameters) = closure
            .invoke()
            .params()
            .iter()
            .map(|parameter| {
                let signature = match parameter.payload() {
                    OutgoingParam::Value(ParamPlan::Direct {
                        ty: DirectValueType::Primitive(primitive),
                        ..
                    }) => Self::primitive_signature(*primitive).map(str::to_owned),
                    OutgoingParam::Value(ParamPlan::Direct {
                        ty: DirectValueType::Record(id),
                        ..
                    }) => context
                        .record(*id)
                        .map(|record| Name::new(record.name()).type_name().to_string())
                        .ok_or(Error::UnsupportedTarget {
                            target: "typescript",
                            shape: "closure record without declaration",
                        }),
                    OutgoingParam::Value(ParamPlan::Direct {
                        ty: DirectValueType::Enum(id),
                        ..
                    }) => context
                        .enumeration(*id)
                        .map(|enumeration| Name::new(enumeration.name()).type_name().to_string())
                        .ok_or(Error::UnsupportedTarget {
                            target: "typescript",
                            shape: "closure enum without declaration",
                        }),
                    OutgoingParam::Value(ParamPlan::Encoded {
                        ty,
                        shape: wasm32::BufferShape::Slice,
                        ..
                    }) => Self::type_signature(ty, context),
                    OutgoingParam::Value(_) | OutgoingParam::Closure(_) => return Ok(None),
                }?;
                Ok(Some(Parameter {
                    signature,
                    imported: ImportedParameter::from_declaration(parameter, context)?,
                }))
            })
            .collect::<Result<Option<Vec<_>>>>()?
        else {
            return Ok(None);
        };
        let fallible = Self::fallible(closure, context)?;
        let (
            public_return,
            return_signature,
            carrier_return,
            returns_void,
            returns_string,
            return_primitive,
            encoded_setup,
        ) = match &fallible {
            Some((public_return, return_signature, _)) => (
                public_return.clone(),
                return_signature.clone(),
                TypeName::bigint(),
                false,
                false,
                None,
                Vec::new(),
            ),
            None => match closure.invoke().returns().plan() {
                ReturnPlan::Void => (
                    TypeName::void(),
                    String::new(),
                    TypeName::void(),
                    true,
                    false,
                    None,
                    Vec::new(),
                ),
                ReturnPlan::DirectViaReturnSlot {
                    ty: DirectValueType::Primitive(primitive),
                } => (
                    Scalar::new(*primitive)?.ty(),
                    Self::primitive_signature(*primitive)?.to_owned(),
                    ImportedParameter::carrier_type(*primitive)?,
                    false,
                    false,
                    Some(*primitive),
                    Vec::new(),
                ),
                ReturnPlan::DirectViaReturnSlot {
                    ty: DirectValueType::Enum(id),
                } => (
                    context
                        .enumeration(*id)
                        .map(|enumeration| Name::new(enumeration.name()).type_name())
                        .ok_or(Error::UnsupportedTarget {
                            target: "typescript",
                            shape: "closure enum without declaration",
                        })?,
                    context
                        .enumeration(*id)
                        .map(|enumeration| Name::new(enumeration.name()).type_name().to_string())
                        .ok_or(Error::UnsupportedTarget {
                            target: "typescript",
                            shape: "closure enum without declaration",
                        })?,
                    TypeName::number(),
                    false,
                    false,
                    None,
                    Vec::new(),
                ),
                ReturnPlan::DirectViaOutPointer {
                    ty: DirectValueType::Record(id),
                } => {
                    let record = context.record(*id).ok_or(Error::UnsupportedTarget {
                        target: "typescript",
                        shape: "closure record without declaration",
                    })?;
                    let size = match record {
                        boltffi_binding::RecordDecl::Direct(record) => record.layout().size().get(),
                        _ => return Ok(None),
                    };
                    let codec = Name::new(record.name()).codec_identifier()?;
                    let writer = Identifier::known("resultWriter");
                    let result = Expression::identifier(Identifier::known("result"));
                    (
                        Name::new(record.name()).type_name(),
                        Name::new(record.name()).type_name().to_string(),
                        TypeName::void(),
                        false,
                        false,
                        None,
                        vec![
                            Statement::constant(
                                writer.clone(),
                                Expression::call(
                                    Expression::identifier(Identifier::known("_module")),
                                    Identifier::known("writerFromMemory"),
                                    [
                                        Expression::identifier(Identifier::known("resultPointer")),
                                        Expression::integer(size),
                                    ]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                                ),
                            ),
                            Statement::expression(Expression::call(
                                Expression::identifier(codec),
                                Identifier::known("encode"),
                                [Expression::identifier(writer), result]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            )),
                        ],
                    )
                }
                ReturnPlan::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                } => (
                    TypeName::string(),
                    "String".to_owned(),
                    TypeName::bigint(),
                    false,
                    true,
                    None,
                    Vec::new(),
                ),
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
                    let setup = std::iter::once(Statement::constant(
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
                    .collect();
                    (
                        super::Type::from_ref(ty, context)?,
                        Self::type_signature(ty, context)?,
                        TypeName::bigint(),
                        false,
                        false,
                        None,
                        setup,
                    )
                }
                _ => return Ok(None),
            },
        };
        if fallible.is_none()
            && !matches!(
                closure.invoke().error().channel(),
                boltffi_binding::ErrorChannel::None
            )
            || !matches!(
                closure.invoke().execution(),
                boltffi_binding::ExecutionDecl::Synchronous(_)
            )
        {
            return Ok(None);
        }
        let parameter_signature = parameters
            .iter()
            .map(|parameter| parameter.signature.as_str())
            .collect::<String>();
        let public_signature = match (parameter_signature.is_empty(), return_signature.is_empty()) {
            (true, true) => "Void".to_owned(),
            (true, false) => format!("To{return_signature}"),
            (false, true) => parameter_signature,
            (false, false) => format!("{parameter_signature}To{return_signature}"),
        };
        let closure_name = format!("Closure{public_signature}");
        let name = TypeName::named(&closure_name);
        let registry = Identifier::parse(format!("_closure{public_signature}Registry"))?;
        let register = Identifier::parse(format!("registerClosure{public_signature}"))?;
        let unregister = Identifier::parse(format!("unregisterClosure{public_signature}"))?;
        let callback = Identifier::known("callback");
        let arguments = parameters
            .iter()
            .map(|parameter| parameter.imported.argument.clone())
            .collect::<ArgumentList>();
        let invocation = Expression::invoke(callback, arguments);
        let invocation = match return_primitive {
            Some(Primitive::Bool) => {
                invocation.conditional(Expression::integer(1), Expression::integer(0))
            }
            Some(_) | None => invocation,
        };
        let returns_direct_record = matches!(
            closure.invoke().returns().plan(),
            ReturnPlan::DirectViaOutPointer {
                ty: DirectValueType::Record(_)
            }
        );
        Ok(Some(Self {
            name,
            registry_name: StringLiteral::new(&closure_name),
            registry,
            register,
            unregister,
            call_import: StringLiteral::new(closure.registration().call().name().as_str()),
            free_import: StringLiteral::new(closure.registration().free().name().as_str()),
            parameters,
            public_return,
            carrier_return,
            invocation,
            returns_void,
            returns_string,
            returns_encoded: !encoded_setup.is_empty() && !returns_direct_record,
            returns_direct_record,
            return_pointer: returns_direct_record.then(|| Identifier::known("resultPointer")),
            encoded_setup,
            fallible: fallible.map(|(_, _, fallible)| fallible),
        }))
    }

    pub fn render_all<'bindings>(
        closures: impl IntoIterator<Item = WasmIncomingClosure<'bindings>>,
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        closures
            .into_iter()
            .map(|closure| Self::from_closure(closure, context))
            .filter_map(|adapter| match adapter {
                Ok(Some(adapter)) => Some(adapter.render().map_err(Into::into)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>>>()
            .map(|adapters| adapters.join("\n\n"))
    }

    pub fn parameter_type(&self) -> TypeName {
        self.name.clone()
    }

    pub fn register(&self) -> Identifier {
        self.register.clone()
    }

    fn fallible(
        closure: WasmIncomingClosure<'_>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Option<(TypeName, String, Fallible)>> {
        let ReturnPlan::DirectViaOutPointer {
            ty: DirectValueType::Primitive(Primitive::I32),
        } = closure.invoke().returns().plan()
        else {
            return Ok(None);
        };
        let boltffi_binding::ErrorDecl::EncodedViaReturnSlot {
            ty,
            codec,
            shape: wasm32::BufferShape::Packed,
        } = closure.invoke().error()
        else {
            return Ok(None);
        };
        let success = TypeName::number();
        let error = super::Type::from_ref(ty, context)?;
        let public_return = TypeName::union(
            success.clone(),
            TypeName::union(
                TypeName::generic("WireResult", [success, error]),
                TypeName::named("Error"),
            ),
        );
        let error_value = Expression::identifier(Identifier::known("error"));
        let size = codec.size_with(&mut Sizer::new(error_value.clone(), context))?;
        let writer = Identifier::known("resultWriter");
        let writes = codec
            .render_with(&mut Writer::new(writer.clone(), error_value, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let error_setup = std::iter::once(Statement::constant(
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
        .collect();
        Ok(Some((
            public_return,
            format!("ResultI32Err{}", Self::type_signature(ty, context)?),
            Fallible {
                success_pointer: Identifier::known("successPointer"),
                success_write: Identifier::known("writeI32"),
                error_setup,
            },
        )))
    }

    fn primitive_signature(primitive: Primitive) -> Result<&'static str> {
        match primitive {
            Primitive::Bool => Ok("Bool"),
            Primitive::I8 => Ok("I8"),
            Primitive::U8 => Ok("U8"),
            Primitive::I16 => Ok("I16"),
            Primitive::U16 => Ok("U16"),
            Primitive::I32 => Ok("I32"),
            Primitive::U32 => Ok("U32"),
            Primitive::I64 => Ok("I64"),
            Primitive::U64 => Ok("U64"),
            Primitive::ISize => Ok("ISize"),
            Primitive::USize => Ok("USize"),
            Primitive::F32 => Ok("F32"),
            Primitive::F64 => Ok("F64"),
            _ => Err(Error::UnsupportedTarget {
                target: "typescript",
                shape: "closure primitive signature",
            }),
        }
    }

    fn type_signature(ty: &TypeRef, context: &RenderContext<Wasm32>) -> Result<String> {
        Ok(match ty {
            TypeRef::Primitive(primitive) => Self::primitive_signature(*primitive)?.to_owned(),
            TypeRef::String => "String".to_owned(),
            TypeRef::Bytes => "Bytes".to_owned(),
            TypeRef::Record(id) => context
                .record(*id)
                .map(|record| Name::new(record.name()).type_name().to_string())
                .ok_or(Error::UnsupportedTarget {
                    target: "typescript",
                    shape: "closure record without declaration",
                })?,
            TypeRef::Enum(id) => context
                .enumeration(*id)
                .map(|enumeration| Name::new(enumeration.name()).type_name().to_string())
                .ok_or(Error::UnsupportedTarget {
                    target: "typescript",
                    shape: "closure enum without declaration",
                })?,
            TypeRef::Optional(inner) => format!("Opt{}", Self::type_signature(inner, context)?),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "typescript",
                    shape: "closure public signature type",
                });
            }
        })
    }
}
