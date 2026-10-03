use boltffi_binding::{
    DirectValueType, DirectVectorElementType, EnumDecl, HandlePresence, HandleTarget, OutOfRust,
    ParamPlanRender, Primitive, TypeRef, Wasm32, wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    codec::Reader,
    name_style::Name,
    primitive::Scalar,
    syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
};
use super::{Type, direct_vector::DirectVector, scalar_option::ScalarOption};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub class_release: Option<Identifier>,
    pub name: Identifier,
    pub public_type: TypeName,
    pub bindings: Vec<Binding>,
    pub setup: Vec<Statement>,
    pub argument: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    pub name: Identifier,
    pub carrier_type: TypeName,
}

struct Renderer<'context> {
    name: Identifier,
    context: &'context RenderContext<'context, Wasm32>,
}

impl Parameter {
    pub fn from_declaration(
        parameter: &boltffi_binding::ParamDecl<Wasm32, OutOfRust>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let name = Name::new(parameter.name()).identifier()?;
        let parameter = parameter
            .payload()
            .as_value()
            .ok_or_else(|| Self::unsupported("outgoing closure parameter"))?
            .render_with(&mut Renderer {
                name: Identifier::parse(format!("__boltffi_arg_{name}"))?,
                context,
            })?;
        Ok(Self { name, ..parameter })
    }

    pub fn primitive(name: Identifier, primitive: Primitive) -> Result<Self> {
        let scalar = Scalar::new(primitive)?;
        let value = Expression::identifier(name.clone());
        Ok(Self {
            class_release: None,
            name: name.clone(),
            public_type: scalar.ty(),
            bindings: vec![Binding {
                name,
                carrier_type: scalar.carrier_type(),
            }],
            setup: Vec::new(),
            argument: scalar.lift(value),
        })
    }

    fn direct(
        name: Identifier,
        ty: &DirectValueType,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match ty {
            DirectValueType::Primitive(primitive) => Self::primitive(name, *primitive),
            DirectValueType::Enum(id) => {
                let enumeration = context
                    .enumeration(*id)
                    .ok_or_else(|| Self::unsupported("imported enum without declaration"))?;
                let EnumDecl::CStyle(enumeration) = enumeration else {
                    return Err(Self::unsupported("direct data enum parameter"));
                };
                let public_type = Name::new(enumeration.name()).type_name();
                let mut parameter = Self::primitive(name, enumeration.repr().primitive())?;
                parameter.public_type = public_type.clone();
                parameter.argument = parameter.argument.cast(public_type);
                Ok(parameter)
            }
            DirectValueType::Record(id) => {
                let record = context
                    .record(*id)
                    .ok_or_else(|| Self::unsupported("imported record without declaration"))?;
                let size = match record {
                    boltffi_binding::RecordDecl::Direct(record) => record.layout().size().get(),
                    _ => return Err(Self::unsupported("encoded imported direct record")),
                };
                let pointer = Identifier::parse(format!("{name}Pointer"))?;
                let reader = Identifier::parse(format!("{name}Reader"))?;
                Ok(Self {
                    class_release: None,
                    name: name.clone(),
                    public_type: Name::new(record.name()).type_name(),
                    bindings: vec![Binding {
                        name: pointer.clone(),
                        carrier_type: TypeName::number(),
                    }],
                    setup: vec![
                        Statement::constant(
                            reader.clone(),
                            Expression::call(
                                Expression::identifier(Identifier::known("_module")),
                                Identifier::known("readerFromMemory"),
                                [Expression::identifier(pointer), Expression::integer(size)]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                        ),
                        Statement::constant(
                            name.clone(),
                            Expression::call(
                                Expression::identifier(
                                    Name::new(record.name()).codec_identifier()?,
                                ),
                                Identifier::known("decode"),
                                [Expression::identifier(reader)]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                        ),
                    ],
                    argument: Expression::identifier(name),
                })
            }
            _ => Err(Self::unsupported("imported direct parameter")),
        }
    }

    fn encoded(
        name: Identifier,
        ty: &TypeRef,
        codec: &boltffi_binding::ReadPlan,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let pointer = Identifier::parse(format!("{name}Pointer"))?;
        let length = Identifier::parse(format!("{name}Length"))?;
        let reader = Identifier::parse(format!("{name}Reader"))?;
        let decoded = codec.render_with(&mut Reader::new(reader.clone(), context))?;
        Ok(Self {
            class_release: None,
            name: name.clone(),
            public_type: Type::from_ref(ty, context)?,
            bindings: vec![
                Binding {
                    name: pointer.clone(),
                    carrier_type: TypeName::number(),
                },
                Binding {
                    name: length.clone(),
                    carrier_type: TypeName::number(),
                },
            ],
            setup: vec![
                Statement::constant(
                    reader,
                    Expression::call(
                        Expression::identifier(Identifier::known("_module")),
                        Identifier::known("readerFromMemory"),
                        [
                            Expression::identifier(pointer),
                            Expression::identifier(length),
                        ]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ),
                ),
                Statement::constant(name.clone(), decoded.into_expression()),
            ],
            argument: Expression::identifier(name),
        })
    }

    fn scalar_option(name: Identifier, primitive: Primitive) -> Result<Self> {
        let option = ScalarOption::new(primitive)?;
        let value = Expression::identifier(name.clone());
        Ok(Self {
            class_release: None,
            name: name.clone(),
            public_type: Scalar::new(primitive)?.ty().nullable(),
            bindings: vec![Binding {
                name,
                carrier_type: option.carrier_type(),
            }],
            setup: Vec::new(),
            argument: Expression::call(
                Expression::identifier(Identifier::known("_module")),
                option.unpack_method(),
                [value].into_iter().collect::<ArgumentList>(),
            ),
        })
    }

    fn direct_vector(
        name: Identifier,
        element: &DirectVectorElementType,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let vector = DirectVector::outgoing(element, context)?;
        let pointer = Identifier::parse(format!("{name}Pointer"))?;
        let length = Identifier::parse(format!("{name}Length"))?;
        Ok(Self {
            class_release: None,
            name: name.clone(),
            public_type: vector.return_type(),
            bindings: vec![
                Binding {
                    name: pointer.clone(),
                    carrier_type: TypeName::number(),
                },
                Binding {
                    name: length.clone(),
                    carrier_type: TypeName::number(),
                },
            ],
            setup: vec![Statement::constant(
                name.clone(),
                vector.borrow(
                    Expression::identifier(pointer),
                    Expression::identifier(length),
                ),
            )],
            argument: Expression::identifier(name),
        })
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Wasm32, OutOfRust> for Renderer<'_> {
    type Output = Result<Parameter>;

    fn direct(&mut self, ty: &'plan DirectValueType, _receive: ()) -> Self::Output {
        Parameter::direct(self.name.clone(), ty, self.context)
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan boltffi_binding::ReadPlan,
        shape: wasm32::BufferShape,
        _receive: (),
    ) -> Self::Output {
        match shape {
            wasm32::BufferShape::Slice => {
                Parameter::encoded(self.name.clone(), ty, codec, self.context)
            }
            _ => Err(Parameter::unsupported("imported encoded parameter shape")),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: wasm32::HandleCarrier,
        presence: HandlePresence,
        _receive: (),
    ) -> Self::Output {
        let HandleTarget::Class(class) = target else {
            return Err(Parameter::unsupported("imported handle parameter"));
        };
        let declaration = self
            .context
            .class(*class)
            .ok_or_else(|| Parameter::unsupported("imported class without declaration"))?;
        let class = Name::new(declaration.name()).type_name();
        let binding = self.name.clone();
        let local = Identifier::parse(format!("{binding}_value"))?;
        let value = Expression::identifier(binding.clone());
        let wrap = Expression::static_call(
            &class,
            Identifier::known("_fromHandle"),
            [value.clone()].into_iter().collect(),
        );
        let (public_type, wrap) = match presence {
            HandlePresence::Required => (class, wrap),
            HandlePresence::Nullable => (
                class.nullable(),
                value
                    .strict_equal(Expression::integer(0))
                    .conditional(Expression::null(), wrap),
            ),
            _ => return Err(Parameter::unsupported("imported class handle presence")),
        };
        Ok(Parameter {
            class_release: Some(Identifier::parse(declaration.release().name().as_str())?),
            name: self.name.clone(),
            public_type,
            bindings: vec![Binding {
                name: binding,
                carrier_type: TypeName::number(),
            }],
            setup: vec![Statement::assignment(local.clone(), wrap)],
            argument: Expression::identifier(local),
        })
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Parameter::scalar_option(self.name.clone(), primitive)
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        _receive: (),
    ) -> Self::Output {
        Parameter::direct_vector(self.name.clone(), element, self.context)
    }
}
