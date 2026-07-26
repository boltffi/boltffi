use std::{collections::HashSet, fmt};

use boltffi_binding::{
    DirectValueType, DirectVectorElementType, Direction, HandlePresence, HandleTarget, IntoRust,
    Native, ParamDecl, ParamPlanRender, Primitive as BindingPrimitive, Receive, TypeRef, native,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::java::{
        JavaFile, JavaPackage, JavaVersion,
        admission::FunctionShape,
        name_style::Name,
        primitive::Primitive,
        render::{
            ClosureHandle, DirectVector, Enumeration, callback::CallbackHandle, class::ClassHandle,
            record::Record, type_name::JavaType,
        },
        syntax::{Identifier, TypeIdentifier, TypeName},
    },
    target::jvm::method::{Parameter as JvmParameter, Parameters as JvmParameters, SlotWidth},
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Parameter<T> {
    name: Identifier,
    ty: T,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ValueType {
    Primitive(Primitive),
    Record(TypeIdentifier),
    Reference(TypeName),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ReturnType {
    Void,
    Value(ValueType),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CallSignature {
    name: Identifier,
    parameters: JvmParameters<Parameter<ValueType>>,
    returns: ReturnType,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ErasedSignature {
    name: Identifier,
    parameters: Vec<ValueType>,
}

struct ParameterRender<'context, 'bindings> {
    name: Identifier,
    version: JavaVersion,
    context: &'context RenderContext<'bindings, Native>,
    package: Option<&'context JavaPackage>,
}

impl<T> Parameter<T> {
    pub fn new(name: Identifier, ty: T) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &T {
        &self.ty
    }

    pub fn validate_unique(scope: &str, parameters: &[Self]) -> Result<()> {
        let mut names = HashSet::new();
        parameters
            .iter()
            .find(|parameter| !names.insert(&parameter.name))
            .map_or(Ok(()), |parameter| {
                Err(Error::JavaNameCollision {
                    scope: scope.to_owned(),
                    name: parameter.name.to_string(),
                })
            })
    }
}

impl Parameter<ValueType> {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
    ) -> Result<Self> {
        let name = Name::new(parameter.name()).parameter(version)?;
        match parameter.payload().as_value() {
            Some(plan) => plan.render_with(&mut ParameterRender {
                name,
                version,
                context,
                package,
            }),
            None => ClosureHandle::type_name(
                parameter
                    .payload()
                    .as_closure()
                    .ok_or_else(FunctionShape::unexpected_shape)?,
                version,
                package,
            )
            .map(ValueType::Reference)
            .map(|ty| Parameter::new(name, ty)),
        }
    }
}

impl JvmParameter for Parameter<ValueType> {
    fn slot_width(&self) -> SlotWidth {
        self.ty.slot_width()
    }
}

impl CallSignature {
    pub fn new(
        name: Identifier,
        parameters: Vec<Parameter<ValueType>>,
        returns: ReturnType,
    ) -> Result<Self> {
        Parameter::validate_unique(name.as_str(), &parameters)?;
        let parameters = JvmParameters::for_static(parameters)?;
        Ok(Self {
            name,
            parameters,
            returns,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[Parameter<ValueType>] {
        self.parameters.as_slice()
    }

    pub fn returns(&self) -> &ReturnType {
        &self.returns
    }

    pub fn erased(&self) -> ErasedSignature {
        ErasedSignature::new(
            self.name.clone(),
            self.parameters.iter().map(|parameter| parameter.ty.clone()),
        )
    }
}

impl ErasedSignature {
    pub fn new(name: Identifier, parameters: impl IntoIterator<Item = ValueType>) -> Self {
        Self {
            name,
            parameters: parameters.into_iter().collect(),
        }
    }

    pub fn validate_unique<'signature>(
        scope: &str,
        signatures: impl IntoIterator<Item = &'signature Self>,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        signatures
            .into_iter()
            .find(|signature| !seen.insert(*signature))
            .map_or(Ok(()), |signature| {
                Err(Error::JavaNameCollision {
                    scope: scope.to_owned(),
                    name: signature.to_string(),
                })
            })
    }

    pub fn validate_owner(file: &JavaFile, signatures: &[Self]) -> Result<()> {
        Self::validate_unique(file.as_str(), signatures)?;
        signatures
            .iter()
            .find(|signature| signature.conflicts_with_object_instance_method())
            .map_or(Ok(()), |signature| {
                Err(Error::JavaNameCollision {
                    scope: format!("{file} inherited java.lang.Object methods"),
                    name: signature.to_string(),
                })
            })
    }

    pub fn conflicts_with_object_instance_method(&self) -> bool {
        let name = self.name.as_str();
        match self.parameters.as_slice() {
            [] => matches!(
                name,
                "clone"
                    | "finalize"
                    | "getClass"
                    | "hashCode"
                    | "notify"
                    | "notifyAll"
                    | "toString"
                    | "wait"
            ),
            [ValueType::Primitive(Primitive::Long)] => name == "wait",
            [
                ValueType::Primitive(Primitive::Long),
                ValueType::Primitive(Primitive::Int),
            ] => name == "wait",
            _ => false,
        }
    }
}

impl fmt::Display for ErasedSignature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}(", self.name)?;
        formatter.write_str(
            &self
                .parameters
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )?;
        formatter.write_str(")")
    }
}

impl ReturnType {
    pub fn require_void(&self) -> Result<()> {
        match self {
            Self::Void => Ok(()),
            Self::Value(_) => Err(FunctionShape::unexpected_shape()),
        }
    }

    pub fn future(&self, version: JavaVersion) -> Self {
        let value = match self {
            Self::Void => TypeName::named(TypeIdentifier::known("Void", version)),
            Self::Value(value) => value.boxed(version),
        };
        Self::Value(ValueType::Reference(TypeName::parameterized(
            TypeName::qualified(
                ["java", "util", "concurrent"]
                    .into_iter()
                    .map(Identifier::known)
                    .collect(),
                TypeIdentifier::known("CompletableFuture", version),
            ),
            [value],
        )))
    }
}

impl fmt::Display for ReturnType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => formatter.write_str("void"),
            Self::Value(value) => value.fmt(formatter),
        }
    }
}

impl ValueType {
    pub fn slot_width(&self) -> SlotWidth {
        match self {
            Self::Primitive(primitive) => primitive.slot_width(),
            Self::Record(_) | Self::Reference(_) => SlotWidth::Single,
        }
    }

    fn boxed(&self, version: JavaVersion) -> TypeName {
        match self {
            Self::Primitive(primitive) => TypeName::boxed_primitive(*primitive, version),
            Self::Record(record) => TypeName::named(record.clone()),
            Self::Reference(reference) => reference.clone(),
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive(primitive) => primitive.fmt(formatter),
            Self::Record(record) => record.fmt(formatter),
            Self::Reference(reference) => reference.fmt(formatter),
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterRender<'_, '_> {
    type Output = Result<Parameter<ValueType>>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: Receive) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => {
                let primitive = Primitive::try_from(*primitive)?;
                Ok(Parameter::new(
                    self.name.clone(),
                    ValueType::Primitive(primitive),
                ))
            }
            DirectValueType::Record(record) => Ok(Parameter::new(
                self.name.clone(),
                ValueType::Reference(self.generated_type(Record::type_name_for(
                    *record,
                    self.context,
                    self.version,
                )?)),
            )),
            DirectValueType::Enum(enumeration) => Ok(Parameter::new(
                self.name.clone(),
                ValueType::Reference(self.generated_type(Enumeration::type_name_for(
                    *enumeration,
                    self.context,
                    self.version,
                )?)),
            )),
            _ => Err(FunctionShape::unexpected_shape()),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        _: &'plan <IntoRust as Direction>::Codec,
        _: native::BufferShape,
        _: Receive,
    ) -> Self::Output {
        self.type_ref(ty)
            .map(ValueType::Reference)
            .map(|ty| Parameter::new(self.name.clone(), ty))
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => ClassHandle::new(
                *class,
                carrier,
                presence,
                self.version,
                self.context,
                self.package,
            )
            .map(|handle| {
                Parameter::new(self.name.clone(), ValueType::Reference(handle.ty().clone()))
            }),
            HandleTarget::Callback(callback) => CallbackHandle::new(
                *callback,
                carrier,
                presence,
                self.version,
                self.context,
                self.package,
            )
            .map(|handle| {
                Parameter::new(self.name.clone(), ValueType::Reference(handle.ty().clone()))
            }),
            _ => Err(FunctionShape::unexpected_shape()),
        }
    }

    fn scalar_option(&mut self, primitive: BindingPrimitive) -> Self::Output {
        let primitive = Primitive::try_from(primitive)?;
        Ok(Parameter::new(
            self.name.clone(),
            ValueType::Reference(JavaType::optional_primitive(primitive, self.version)),
        ))
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        _: Receive,
    ) -> Self::Output {
        DirectVector::from_element(element, self.version, self.context).map(|vector| {
            Parameter::new(self.name.clone(), ValueType::Reference(vector.ty().clone()))
        })
    }
}

impl ParameterRender<'_, '_> {
    fn generated_type(&self, name: TypeIdentifier) -> TypeName {
        match self.package {
            Some(package) => package.type_name(name),
            None => TypeName::named(name),
        }
    }

    fn type_ref(&self, ty: &TypeRef) -> Result<TypeName> {
        match self.package {
            Some(package) => JavaType::qualified_type_ref(ty, self.version, self.context, package),
            None => JavaType::type_ref(ty, self.version, self.context),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ErasedSignature, ValueType};
    use crate::target::java::{primitive::Primitive, syntax::Identifier};

    #[test]
    fn permits_overloads_and_rejects_return_only_duplicates() {
        let word = ErasedSignature::new(
            Identifier::known("convert"),
            [ValueType::Primitive(Primitive::Int)],
        );
        let wide = ErasedSignature::new(
            Identifier::known("convert"),
            [ValueType::Primitive(Primitive::Long)],
        );
        assert!(ErasedSignature::validate_unique("Demo", [&word, &wide]).is_ok());
        assert!(ErasedSignature::validate_unique("Demo", [&word, &word]).is_err());
    }

    #[test]
    fn recognizes_inherited_object_instance_signatures() {
        let wait = ErasedSignature::new(Identifier::known("wait"), []);
        let timed_wait = ErasedSignature::new(
            Identifier::known("wait"),
            [ValueType::Primitive(Primitive::Long)],
        );
        let notify_value = ErasedSignature::new(
            Identifier::known("notify"),
            [ValueType::Primitive(Primitive::Int)],
        );

        assert!(wait.conflicts_with_object_instance_method());
        assert!(timed_wait.conflicts_with_object_instance_method());
        assert!(!notify_value.conflicts_with_object_instance_method());
    }
}
