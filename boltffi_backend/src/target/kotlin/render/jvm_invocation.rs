use boltffi_binding::{
    DirectValueType, DirectVectorElementType, Direction, HandlePresence, HandleTarget, Native,
    OutOfRust, OutgoingParam, ParamDecl, ParamPlanRender, Primitive, Surface, TypeRef,
};

use crate::{
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{Reader, ScalarOption},
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            callback::CallbackHandle, class::ClassHandle, direct_vector::DirectVector,
            enumeration::Enumeration, record::Record, signature::Parameter as SignatureParameter,
            type_name::KotlinType,
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    public: SignatureParameter,
    jvm: SignatureParameter,
    setup: Vec<Statement>,
    argument: Expression,
}

impl Parameter {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, OutOfRust>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let OutgoingParam::Value(plan) = parameter.payload() else {
            return Err(KotlinHost::unsupported("jvm invocation closure parameter"));
        };
        let source_name = Name::new(parameter.name());
        let name = source_name.parameter()?;
        plan.render_with(&mut Renderer {
            source_name,
            name,
            host,
            context,
        })
    }

    pub fn public(&self) -> &SignatureParameter {
        &self.public
    }

    pub fn jvm(&self) -> &SignatureParameter {
        &self.jvm
    }

    pub fn setup(&self) -> &[Statement] {
        &self.setup
    }

    pub fn argument(&self) -> &Expression {
        &self.argument
    }
}

struct Renderer<'render> {
    source_name: Name,
    name: Identifier,
    host: &'render KotlinHost,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for Renderer<'_> {
    type Output = Result<Parameter>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => {
                let value = Expression::identifier(self.name.clone());
                Ok(Parameter {
                    public: SignatureParameter::new(
                        self.name.clone(),
                        KotlinPrimitive::new(*primitive).api_type()?,
                    ),
                    jvm: SignatureParameter::new(
                        self.name.clone(),
                        KotlinPrimitive::new(*primitive).native_type()?,
                    ),
                    setup: Vec::new(),
                    argument: KotlinPrimitive::new(*primitive).public_return(value)?,
                })
            }
            DirectValueType::Enum(enumeration) => {
                let enumeration = Enumeration::from_id(*enumeration, self.host, self.context)?;
                let repr = enumeration.repr()?;
                Ok(Parameter {
                    public: SignatureParameter::new(self.name.clone(), enumeration.name().clone()),
                    jvm: SignatureParameter::new(
                        self.name.clone(),
                        KotlinPrimitive::new(repr).native_type()?,
                    ),
                    setup: Vec::new(),
                    argument: Expression::call(
                        enumeration.name().clone(),
                        Identifier::parse("fromValue")?,
                        [Expression::identifier(self.name.clone())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                })
            }
            DirectValueType::Record(record) => {
                let ty = Record::type_name_from_id(*record, self.context)?;
                Ok(Parameter {
                    public: SignatureParameter::new(self.name.clone(), ty.clone()),
                    jvm: SignatureParameter::new(self.name.clone(), TypeName::byte_array(false)),
                    setup: Vec::new(),
                    argument: Record::decode_expression(
                        ty,
                        Expression::identifier(self.name.clone()),
                    )?,
                })
            }
            _ => Err(KotlinHost::unsupported("jvm invocation direct parameter")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let reader = self.source_name.generated("reader")?;
        let value = self.source_name.generated("value")?;
        let expression = codec
            .render_with(&mut Reader::new(reader.clone(), self.host, self.context))?
            .into_expression();
        Ok(Parameter {
            public: SignatureParameter::new(
                self.name.clone(),
                KotlinType::type_ref(ty, self.context)?,
            ),
            jvm: SignatureParameter::new(self.name.clone(), TypeName::byte_array(false)),
            setup: vec![
                Statement::value(
                    reader,
                    Expression::construct(
                        TypeName::new("WireReader"),
                        [Expression::identifier(self.name.clone())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                ),
                Statement::value(value.clone(), expression),
            ],
            argument: Expression::identifier(value),
        })
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <OutOfRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match target {
            HandleTarget::Class(class) => {
                let handle = ClassHandle::new(*class, presence, self.context)?;
                Ok(Parameter {
                    public: SignatureParameter::new(self.name.clone(), handle.ty()?),
                    jvm: SignatureParameter::new(self.name.clone(), TypeName::long()),
                    setup: Vec::new(),
                    argument: handle.value_expression(value)?,
                })
            }
            HandleTarget::Callback(callback) => {
                let handle = CallbackHandle::new(*callback, presence, self.context)?;
                Ok(Parameter {
                    public: SignatureParameter::new(self.name.clone(), handle.ty()?),
                    jvm: SignatureParameter::new(self.name.clone(), TypeName::long()),
                    setup: Vec::new(),
                    argument: handle.value_expression(value)?,
                })
            }
            HandleTarget::Stream(_) => {
                Err(KotlinHost::unsupported("jvm invocation stream parameter"))
            }
            _ => Err(KotlinHost::unsupported(
                "unknown jvm invocation handle parameter",
            )),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let reader = self.source_name.generated("reader")?;
        let value = self.source_name.generated("value")?;
        Ok(Parameter {
            public: SignatureParameter::new(self.name.clone(), ScalarOption::new(primitive).ty()?),
            jvm: SignatureParameter::new(self.name.clone(), TypeName::byte_array(false)),
            setup: vec![
                Statement::value(
                    reader.clone(),
                    Expression::construct(
                        TypeName::new("WireReader"),
                        [Expression::identifier(self.name.clone())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                ),
                Statement::value(
                    value.clone(),
                    ScalarOption::new(primitive).read_expression(reader)?,
                ),
            ],
            argument: Expression::identifier(value),
        })
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let vector = DirectVector::from_element(element, self.context)?;
        let decoded = vector.decoded_argument(&self.source_name, self.name.clone())?;
        Ok(Parameter {
            public: SignatureParameter::new(self.name.clone(), vector.ty().clone()),
            jvm: SignatureParameter::new(self.name.clone(), decoded.jvm_ty().clone()),
            setup: decoded.setup().to_vec(),
            argument: decoded.call_argument().clone(),
        })
    }
}
