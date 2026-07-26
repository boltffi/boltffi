use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, DirectValueType, DirectVectorElementType,
    Direction, EnumId, HandlePresence, HandleTarget, IntoRust, Native, ParamPlanRender, Primitive,
    Receive, RecordId, Surface, TypeRef, TypeRefRender,
};

use crate::{
    bridge::jni::{CallbackHandleMethod, JniType, NativeReturn},
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::ScalarOption,
        name_style::KotlinPackage,
        primitive::KotlinPrimitive,
        render::{
            callback::CallbackHandle, class::ClassHandle, direct_vector::DirectVector,
            enumeration::Enumeration, record::Record,
        },
        syntax::TypeName,
        tuple::Arity,
    },
};

pub struct KotlinType;

impl KotlinType {
    pub fn primitive(primitive: Primitive) -> Result<TypeName> {
        KotlinPrimitive::new(primitive).api_type()
    }

    pub fn builtin(kind: BuiltinType) -> TypeName {
        match kind {
            BuiltinType::Duration => TypeName::new("java.time.Duration"),
            BuiltinType::SystemTime => TypeName::new("java.time.Instant"),
            BuiltinType::Uuid => TypeName::new("java.util.UUID"),
            BuiltinType::Url => TypeName::new("java.net.URI"),
        }
    }

    pub fn jni(jni_type: JniType) -> Result<TypeName> {
        match jni_type {
            JniType::Boolean => Ok(TypeName::boolean()),
            JniType::Byte => Ok(TypeName::byte()),
            JniType::Short => Ok(TypeName::short()),
            JniType::Int => Ok(TypeName::int()),
            JniType::Long => Ok(TypeName::long()),
            JniType::Float => Ok(TypeName::float()),
            JniType::Double => Ok(TypeName::double()),
        }
    }

    pub fn jni_array(jni_type: JniType) -> Result<TypeName> {
        match jni_type {
            JniType::Boolean => Ok(TypeName::new("BooleanArray")),
            JniType::Byte => Ok(TypeName::new("ByteArray")),
            JniType::Short => Ok(TypeName::new("ShortArray")),
            JniType::Int => Ok(TypeName::new("IntArray")),
            JniType::Long => Ok(TypeName::new("LongArray")),
            JniType::Float => Ok(TypeName::new("FloatArray")),
            JniType::Double => Ok(TypeName::new("DoubleArray")),
        }
    }

    pub fn native_return(return_value: &NativeReturn) -> Result<TypeName> {
        match return_value {
            NativeReturn::Void | NativeReturn::Status | NativeReturn::EncodedError => {
                Ok(TypeName::unit())
            }
            NativeReturn::Value(scalar) => Self::jni(scalar.jni_type()),
            NativeReturn::Bytes | NativeReturn::Record(_) | NativeReturn::StatusWriteback(_) => {
                Ok(TypeName::byte_array(true))
            }
            NativeReturn::Callback(_) => Ok(TypeName::long()),
            NativeReturn::StatusValue(value) => Self::native_return(value.value()),
            NativeReturn::EncodedErrorValue(value) => Self::native_return(value.success().value()),
        }
    }

    pub fn callback_handle_return(method: &CallbackHandleMethod) -> Result<TypeName> {
        match method.synchronous_return() {
            Some(return_value) => Self::native_return(return_value),
            None if method.returns_closure() => Ok(TypeName::long()),
            None => Ok(TypeName::unit()),
        }
    }

    pub fn type_ref(ty: &TypeRef, context: &RenderContext<Native>) -> Result<TypeName> {
        ty.render_with(&mut KotlinTypeRef::new(context))
            .map(ApiType::into_type)
    }

    pub fn type_ref_with_package(
        ty: &TypeRef,
        context: &RenderContext<Native>,
        package: &KotlinPackage,
    ) -> Result<TypeName> {
        ty.render_with(&mut KotlinTypeRef::new(context).package(package))
            .map(ApiType::into_type)
    }

    pub fn direct_vector_element(
        element: &DirectVectorElementType,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        DirectVector::from_element(element, context).map(|vector| vector.ty().clone())
    }
}

struct KotlinTypeRef<'context> {
    context: &'context RenderContext<'context, Native>,
    package: Option<KotlinPackage>,
}

struct ApiType {
    ty: TypeName,
    primitive: Option<Primitive>,
}

impl<'context> KotlinTypeRef<'context> {
    pub fn new(context: &'context RenderContext<Native>) -> Self {
        Self {
            context,
            package: None,
        }
    }

    pub fn package(mut self, package: &KotlinPackage) -> Self {
        self.package = Some(package.clone());
        self
    }

    fn qualify(&self, ty: TypeName) -> TypeName {
        self.package
            .as_ref()
            .map_or(ty.clone(), |package| TypeName::qualified(package, ty))
    }
}

impl TypeRefRender for KotlinTypeRef<'_> {
    type Output = Result<ApiType>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        KotlinType::primitive(primitive).map(|ty| ApiType {
            ty,
            primitive: Some(primitive),
        })
    }

    fn string(&mut self) -> Self::Output {
        Ok(ApiType::new(TypeName::string()))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        // Kotlin does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString type ref reached Kotlin renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(ApiType::new(TypeName::byte_array(false)))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        Record::type_name_from_id(id, self.context).map(|record| ApiType::new(self.qualify(record)))
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        Enumeration::type_name_from_id(id, self.context)
            .map(|enumeration| ApiType::new(self.qualify(enumeration)))
    }

    fn class(&mut self, _id: ClassId) -> Self::Output {
        Err(KotlinHost::unsupported("class type"))
    }

    fn callback(&mut self, _id: CallbackId) -> Self::Output {
        Err(KotlinHost::unsupported("callback type"))
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        if let Some(mapping) = self.context.custom_type_mapping(id) {
            return Ok(ApiType::new(KotlinHost::custom_type_name(mapping)));
        }

        self.context
            .custom_type(id)
            .map(|custom_type| custom_type.representation())
            .ok_or(KotlinHost::unsupported("custom type without declaration"))?
            .render_with(self)
            .map(|inner| ApiType::new(inner.ty))
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(ApiType::new(KotlinType::builtin(kind)))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        inner.map(|inner| ApiType::new(inner.ty.nullable()))
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        let element = element?;
        match element.primitive {
            Some(primitive) => KotlinPrimitive::new(primitive)
                .direct_vector_type()
                .map(ApiType::new),
            None => Ok(ApiType::new(TypeName::list(element.ty))),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output {
        let arity = Arity::from_count(elements.len())?;
        elements
            .into_iter()
            .map(|element| element.map(ApiType::into_type))
            .collect::<Result<Vec<_>>>()
            .and_then(|elements| arity.type_name(elements))
            .map(ApiType::new)
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        Ok(ApiType::new(TypeName::result(ok?.ty, err?.ty)))
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        Ok(ApiType::new(TypeName::map(key?.ty, value?.ty)))
    }
}

impl ApiType {
    fn new(ty: TypeName) -> Self {
        Self {
            ty,
            primitive: None,
        }
    }

    fn into_type(self) -> TypeName {
        self.ty
    }
}

pub struct ParameterType<'context> {
    context: &'context RenderContext<'context, Native>,
    package: Option<KotlinPackage>,
}

impl<'context> ParameterType<'context> {
    pub fn new(context: &'context RenderContext<'context, Native>) -> Self {
        Self {
            context,
            package: None,
        }
    }

    pub fn package(mut self, package: Option<&KotlinPackage>) -> Self {
        self.package = package.cloned();
        self
    }

    fn qualify(&self, ty: TypeName) -> TypeName {
        self.package
            .as_ref()
            .map_or(ty.clone(), |package| TypeName::qualified(package, ty))
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterType<'_> {
    type Output = Result<TypeName>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => KotlinType::primitive(*primitive),
            DirectValueType::Record(record) => {
                Record::type_name_from_id(*record, self.context).map(|record| self.qualify(record))
            }
            DirectValueType::Enum(enumeration) => {
                Enumeration::type_name_from_id(*enumeration, self.context)
                    .map(|enumeration| self.qualify(enumeration))
            }
            _ => Err(KotlinHost::unsupported("unknown direct function parameter")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        _codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match &self.package {
            Some(package) => KotlinType::type_ref_with_package(ty, self.context, package),
            None => KotlinType::type_ref(ty, self.context),
        }
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                ClassHandle::new(*class, presence, self.context).and_then(|handle| handle.ty())
            }
            HandleTarget::Callback(callback) => {
                CallbackHandle::new(*callback, presence, self.context)
                    .and_then(|handle| handle.ty())
            }
            HandleTarget::Stream(_) => Err(KotlinHost::unsupported("handle function parameter")),
            _ => Err(KotlinHost::unsupported("unknown handle function parameter")),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        ScalarOption::new(primitive).ty()
    }

    fn direct_vector(
        &mut self,
        element: &'plan DirectVectorElementType,
        _: Receive,
    ) -> Self::Output {
        KotlinType::direct_vector_element(element, self.context)
    }
}
