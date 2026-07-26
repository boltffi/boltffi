use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, ClosureReturn, CustomTypeId, DirectValueType,
    DirectVectorElementType, EnumId, HandlePresence, HandleTarget, IntoRust, Native, OutOfRust,
    ParamPlan, ParamPlanRender, Primitive, ReadPlan, Receive, RecordId, ReturnPlan,
    ReturnPlanRender, ReturnValueSlot, TypeRef, TypeRefRender, WritePlan, native,
};

use crate::{
    core::{Error, Result},
    target::python::{render::Package, syntax::TypeAnnotation},
};

pub struct TypeHint {
    annotation: TypeAnnotation,
    uses_sequence: bool,
}

impl TypeHint {
    pub fn from_type_ref(ty: &TypeRef, package: &Package) -> Result<Self> {
        ty.render_with(&mut TypeRefHint::value(package))
    }

    pub fn from_direct_value(ty: &DirectValueType, package: &Package) -> Result<Self> {
        match ty {
            DirectValueType::Primitive(primitive) => Self::from_primitive(*primitive),
            DirectValueType::Record(record) => Ok(Self::new(TypeAnnotation::identifier(
                package.record_name(*record)?,
            ))),
            DirectValueType::Enum(enumeration) => Ok(Self::new(TypeAnnotation::identifier(
                package.enum_name(*enumeration)?,
            ))),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "direct value type hint",
            }),
        }
    }

    pub fn from_parameter(plan: &ParamPlan<Native, IntoRust>, package: &Package) -> Result<Self> {
        plan.render_with(&mut ParameterHint { package })
    }

    pub fn from_return(plan: &ReturnPlan<Native, OutOfRust>, package: &Package) -> Result<Self> {
        plan.render_with(&mut ReturnHint { package })
    }

    pub fn from_direct_vector_parameter(
        element: &DirectVectorElementType,
        package: &Package,
    ) -> Result<Self> {
        let element = Self::from_direct_vector_element(element, package)?;
        Ok(Self::sequence(TypeAnnotation::sequence(element.annotation)))
    }

    pub fn from_primitive(primitive: Primitive) -> Result<Self> {
        Ok(match primitive {
            Primitive::Bool => Self::new(TypeAnnotation::bool()),
            Primitive::F32 | Primitive::F64 => Self::new(TypeAnnotation::float()),
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize => Self::new(TypeAnnotation::int()),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported primitive type hint",
                });
            }
        })
    }

    pub fn into_annotation(self) -> TypeAnnotation {
        self.annotation
    }

    pub fn uses_sequence(&self) -> bool {
        self.uses_sequence
    }

    fn from_parameter_type_ref(ty: &TypeRef, package: &Package) -> Result<Self> {
        ty.render_with(&mut TypeRefHint::parameter(package))
    }

    fn from_direct_vector_return(
        element: &DirectVectorElementType,
        package: &Package,
    ) -> Result<Self> {
        let element = Self::from_direct_vector_element(element, package)?;
        Ok(Self::new(TypeAnnotation::list(element.annotation)))
    }

    fn from_direct_vector_element(
        element: &DirectVectorElementType,
        package: &Package,
    ) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => {
                Self::from_primitive(primitive.primitive())
            }
            DirectVectorElementType::Record(record) => Ok(Self::new(TypeAnnotation::identifier(
                package.record_name(*record)?,
            ))),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "direct vector element type hint",
            }),
        }
    }

    fn new(annotation: TypeAnnotation) -> Self {
        Self {
            annotation,
            uses_sequence: false,
        }
    }

    fn sequence(annotation: TypeAnnotation) -> Self {
        Self {
            annotation,
            uses_sequence: true,
        }
    }

    fn compose(annotation: TypeAnnotation, parts: impl IntoIterator<Item = Self>) -> Self {
        Self {
            annotation,
            uses_sequence: parts.into_iter().any(|part| part.uses_sequence),
        }
    }

    fn from_builtin(builtin: BuiltinType) -> Self {
        match builtin {
            BuiltinType::Duration | BuiltinType::SystemTime => Self::new(TypeAnnotation::float()),
            BuiltinType::Uuid => Self::new(TypeAnnotation::uuid()),
            BuiltinType::Url => Self::new(TypeAnnotation::string()),
        }
    }
}

#[derive(Clone, Copy)]
enum SequenceHint {
    List,
    Sequence,
}

struct TypeRefHint<'package> {
    package: &'package Package<'package>,
    sequence: SequenceHint,
}

impl<'package> TypeRefHint<'package> {
    fn value(package: &'package Package<'package>) -> Self {
        Self {
            package,
            sequence: SequenceHint::List,
        }
    }

    fn parameter(package: &'package Package<'package>) -> Self {
        Self {
            package,
            sequence: SequenceHint::Sequence,
        }
    }
}

impl<'package> TypeRefRender for TypeRefHint<'package> {
    type Output = Result<TypeHint>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        TypeHint::from_primitive(primitive)
    }

    fn string(&mut self) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::string()))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        unreachable!(
            "InternedString type ref reached Python renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::bytes()))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::identifier(
            self.package.record_name(id)?,
        )))
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::identifier(
            self.package.enum_name(id)?,
        )))
    }

    fn class(&mut self, id: ClassId) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::identifier(
            self.package.class_name(&id)?,
        )))
    }

    fn callback(&mut self, _: CallbackId) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::object()))
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        self.package.custom_representation(id)?.render_with(self)
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(TypeHint::from_builtin(kind))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        let inner = inner?;
        let annotation = TypeAnnotation::optional(inner.annotation.clone());
        Ok(TypeHint::compose(annotation, [inner]))
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        let element = element?;
        match self.sequence {
            SequenceHint::List => {
                let annotation = TypeAnnotation::list(element.annotation.clone());
                Ok(TypeHint::compose(annotation, [element]))
            }
            SequenceHint::Sequence => Ok(TypeHint::sequence(TypeAnnotation::sequence(
                element.annotation.clone(),
            ))),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output {
        let elements = elements.into_iter().collect::<Result<Vec<_>>>()?;
        let annotation =
            TypeAnnotation::tuple(elements.iter().map(|element| element.annotation.clone()));
        Ok(TypeHint::compose(annotation, elements))
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        let ok = ok?;
        let err = err?;
        let annotation = TypeAnnotation::result_pair(ok.annotation.clone(), err.annotation.clone());
        Ok(TypeHint::compose(annotation, [ok, err]))
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        let key = key?;
        let value = value?;
        let annotation = TypeAnnotation::dict(key.annotation.clone(), value.annotation.clone());
        Ok(TypeHint::compose(annotation, [key, value]))
    }
}

struct ParameterHint<'package> {
    package: &'package Package<'package>,
}

impl<'package> ParameterHint<'package> {
    fn encoded_type_ref(&self, ty: &TypeRef, shape: native::BufferShape) -> Result<TypeHint> {
        if shape != native::BufferShape::Slice {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            });
        }
        TypeHint::from_parameter_type_ref(ty, self.package)
    }
}

impl<'plan, 'package> ParamPlanRender<'plan, Native, IntoRust> for ParameterHint<'package> {
    type Output = Result<TypeHint>;

    fn direct(&mut self, ty: &DirectValueType, _: Receive) -> Self::Output {
        TypeHint::from_direct_value(ty, self.package)
    }

    fn encoded(
        &mut self,
        ty: &TypeRef,
        _: &WritePlan,
        shape: native::BufferShape,
        _: Receive,
    ) -> Self::Output {
        self.encoded_type_ref(ty, shape)
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => Ok(TypeHint::new(
                TypeAnnotation::identifier(self.package.class_name(class_id)?),
            )),
            (HandleTarget::Class(class_id), HandlePresence::Nullable) => {
                Ok(TypeHint::new(TypeAnnotation::optional(
                    TypeAnnotation::identifier(self.package.class_name(class_id)?),
                )))
            }
            (HandleTarget::Callback(_), _) => Ok(TypeHint::new(TypeAnnotation::object())),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::optional(
            TypeHint::from_primitive(primitive)?.into_annotation(),
        )))
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType, _: Receive) -> Self::Output {
        TypeHint::from_direct_vector_parameter(element, self.package)
    }
}

struct ReturnHint<'package> {
    package: &'package Package<'package>,
}

impl<'package> ReturnHint<'package> {
    fn type_ref(&self, ty: &TypeRef) -> Result<TypeHint> {
        TypeHint::from_type_ref(ty, self.package)
    }
}

impl<'plan, 'package> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnHint<'package> {
    type Output = Result<TypeHint>;

    fn void(&mut self) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::none()))
    }

    fn direct(&mut self, _: ReturnValueSlot, ty: &DirectValueType) -> Self::Output {
        TypeHint::from_direct_value(ty, self.package)
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &TypeRef,
        _: &ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => self.type_ref(ty),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported return stub",
            }),
        }
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => Ok(TypeHint::new(
                TypeAnnotation::identifier(self.package.class_name(class_id)?),
            )),
            (HandleTarget::Callback(_), _) => Ok(TypeHint::new(TypeAnnotation::object())),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported return stub",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(TypeHint::new(TypeAnnotation::optional(
            TypeHint::from_primitive(primitive)?.into_annotation(),
        )))
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType) -> Self::Output {
        TypeHint::from_direct_vector_return(element, self.package)
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported return stub",
        })
    }
}
