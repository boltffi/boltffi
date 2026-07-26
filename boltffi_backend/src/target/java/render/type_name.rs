use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, EnumId, Native, Primitive as BindingPrimitive,
    RecordId, TypeRef, TypeRefRender,
};

use crate::{
    core::{RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        name_style::{JavaPackage, Name},
        primitive::Primitive,
        render::ResultClass,
        syntax::{Expression, Identifier, TypeIdentifier, TypeName},
    },
};

pub struct JavaType;

pub struct JavaFieldType {
    ty: TypeName,
    semantics: ValueSemantics,
}

enum ValueSemantics {
    Primitive(Primitive),
    Array,
    Reference,
    Optional(Box<ValueSemantics>),
    Sequence(Box<ValueSemantics>),
    Result {
        ok: Box<ValueSemantics>,
        err: Box<ValueSemantics>,
    },
    Map {
        key: Box<ValueSemantics>,
        value: Box<ValueSemantics>,
    },
}

struct JavaTypeRef<'context> {
    context: &'context RenderContext<'context, Native>,
    version: JavaVersion,
    package: Option<&'context JavaPackage>,
}

struct ApiType {
    value: TypeName,
    boxed: TypeName,
    semantics: ValueSemantics,
}

impl JavaType {
    pub fn type_ref(
        ty: &TypeRef,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        ty.render_with(&mut JavaTypeRef {
            context,
            version,
            package: None,
        })
        .map(ApiType::into_value)
    }

    pub fn field(
        ty: &TypeRef,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<JavaFieldType> {
        ty.render_with(&mut JavaTypeRef {
            context,
            version,
            package: None,
        })
        .map(|ty| JavaFieldType {
            ty: ty.value,
            semantics: ty.semantics,
        })
    }

    pub fn qualified_type_ref<'context>(
        ty: &TypeRef,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
        package: &'context JavaPackage,
    ) -> Result<TypeName> {
        ty.render_with(&mut JavaTypeRef {
            context,
            version,
            package: Some(package),
        })
        .map(ApiType::into_value)
    }

    pub fn boxed_type_ref(
        ty: &TypeRef,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        ty.render_with(&mut JavaTypeRef {
            context,
            version,
            package: None,
        })
        .map(ApiType::into_boxed)
    }

    pub fn qualified_field<'context>(
        ty: &TypeRef,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
        package: &'context JavaPackage,
    ) -> Result<JavaFieldType> {
        ty.render_with(&mut JavaTypeRef {
            context,
            version,
            package: Some(package),
        })
        .map(|ty| JavaFieldType {
            ty: ty.value,
            semantics: ty.semantics,
        })
    }

    pub fn optional_primitive(primitive: Primitive, version: JavaVersion) -> TypeName {
        Self::optional(TypeName::boxed_primitive(primitive, version), version)
    }

    pub fn optional(ty: TypeName, version: JavaVersion) -> TypeName {
        TypeName::parameterized(Self::optional_type(version), [ty])
    }

    pub fn optional_type(version: JavaVersion) -> TypeName {
        Self::qualified(version, &["java", "util"], "Optional")
    }

    fn qualified(version: JavaVersion, package: &[&'static str], name: &'static str) -> TypeName {
        TypeName::qualified(
            package.iter().copied().map(Identifier::known).collect(),
            TypeIdentifier::known(name, version),
        )
    }
}

impl ApiType {
    fn reference(value: TypeName) -> Self {
        Self {
            boxed: value.clone(),
            value,
            semantics: ValueSemantics::Reference,
        }
    }

    fn primitive(primitive: Primitive, version: JavaVersion) -> Self {
        Self {
            value: TypeName::primitive(primitive),
            boxed: TypeName::boxed_primitive(primitive, version),
            semantics: ValueSemantics::Primitive(primitive),
        }
    }

    fn into_value(self) -> TypeName {
        self.value
    }

    fn into_boxed(self) -> TypeName {
        self.boxed
    }
}

impl JavaFieldType {
    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn equals(
        &self,
        left: Expression,
        right: Expression,
        version: JavaVersion,
    ) -> Result<Expression> {
        self.semantics.equals(left, right, version, 0)
    }

    pub fn hash(&self, value: Expression, version: JavaVersion) -> Result<Expression> {
        self.semantics.hash(value, version, 0)
    }

    pub fn native_record_safe(&self) -> bool {
        self.semantics.native_record_safe()
    }

    pub fn requires_identity(&self) -> bool {
        self.semantics.requires_identity()
    }
}

impl ValueSemantics {
    fn primitive(&self) -> Option<Primitive> {
        match self {
            Self::Primitive(primitive) => Some(*primitive),
            _ => None,
        }
    }

    fn native_record_safe(&self) -> bool {
        match self {
            Self::Primitive(_) | Self::Reference => true,
            Self::Array => false,
            Self::Optional(inner) | Self::Sequence(inner) => inner.native_record_safe(),
            Self::Result { ok, err } => ok.native_record_safe() && err.native_record_safe(),
            Self::Map { key, value } => key.native_record_safe() && value.native_record_safe(),
        }
    }

    fn has_java_identity(&self) -> bool {
        match self {
            Self::Primitive(_) | Self::Reference | Self::Result { .. } => true,
            Self::Array | Self::Optional(_) => false,
            Self::Sequence(inner) => inner.has_java_identity(),
            Self::Map { key, value } => key.has_java_identity() && value.has_java_identity(),
        }
    }

    fn requires_identity(&self) -> bool {
        match self {
            Self::Optional(_) => true,
            Self::Sequence(inner) => !inner.has_java_identity(),
            _ => false,
        }
    }

    fn equals(
        &self,
        left: Expression,
        right: Expression,
        version: JavaVersion,
        depth: usize,
    ) -> Result<Expression> {
        match self {
            Self::Primitive(primitive) => Ok(primitive.equals(left, right)),
            Self::Array => Ok(Self::static_identity(
                "Arrays",
                "equals",
                [left, right],
                version,
            )),
            Self::Reference | Self::Result { .. } => Ok(Self::static_identity(
                "Objects",
                "equals",
                [left, right],
                version,
            )),
            Self::Optional(inner) => {
                Self::container_equals("optionalEquals", inner, left, right, version, depth)
            }
            Self::Sequence(inner) if inner.has_java_identity() => Ok(Self::static_identity(
                "Objects",
                "equals",
                [left, right],
                version,
            )),
            Self::Sequence(inner) => {
                Self::container_equals("sequenceEquals", inner, left, right, version, depth)
            }
            Self::Map { key, value } if key.has_java_identity() && value.has_java_identity() => Ok(
                Self::static_identity("Objects", "equals", [left, right], version),
            ),
            Self::Map { .. } => Err(JavaHost::unsupported("map field identity")),
        }
    }

    fn hash(&self, value: Expression, version: JavaVersion, depth: usize) -> Result<Expression> {
        match self {
            Self::Primitive(primitive) => Ok(primitive.hash(value)),
            Self::Array => Ok(Self::static_identity(
                "Arrays",
                "hashCode",
                [value],
                version,
            )),
            Self::Reference | Self::Result { .. } => Ok(Self::static_identity(
                "Objects",
                "hashCode",
                [value],
                version,
            )),
            Self::Optional(inner) => {
                Self::container_hash("optionalHash", inner, value, version, depth)
            }
            Self::Sequence(inner) if inner.has_java_identity() => Ok(Self::static_identity(
                "Objects",
                "hashCode",
                [value],
                version,
            )),
            Self::Sequence(inner) => {
                Self::container_hash("sequenceHash", inner, value, version, depth)
            }
            Self::Map {
                key,
                value: map_value,
            } if key.has_java_identity() && map_value.has_java_identity() => Ok(
                Self::static_identity("Objects", "hashCode", [value], version),
            ),
            Self::Map { .. } => Err(JavaHost::unsupported("map field identity")),
        }
    }

    fn container_equals(
        method: &'static str,
        inner: &Self,
        left: Expression,
        right: Expression,
        version: JavaVersion,
        depth: usize,
    ) -> Result<Expression> {
        let left_value = Identifier::parse_for(format!("leftValue{depth}"), version)?;
        let right_value = Identifier::parse_for(format!("rightValue{depth}"), version)?;
        let equals = inner.equals(
            Expression::identifier(left_value.clone()),
            Expression::identifier(right_value.clone()),
            version,
            depth + 1,
        )?;
        Ok(Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFFIValueIdentity", version)),
            Identifier::known(method),
            [
                left,
                right,
                Expression::lambda([left_value, right_value], equals),
            ]
            .into_iter()
            .collect(),
        ))
    }

    fn container_hash(
        method: &'static str,
        inner: &Self,
        value: Expression,
        version: JavaVersion,
        depth: usize,
    ) -> Result<Expression> {
        let item = Identifier::parse_for(format!("itemValue{depth}"), version)?;
        let hash = inner.hash(Expression::identifier(item.clone()), version, depth + 1)?;
        Ok(Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFFIValueIdentity", version)),
            Identifier::known(method),
            [value, Expression::lambda([item], hash)]
                .into_iter()
                .collect(),
        ))
    }

    fn static_identity<const COUNT: usize>(
        owner: &'static str,
        method: &'static str,
        arguments: [Expression; COUNT],
        version: JavaVersion,
    ) -> Expression {
        Expression::static_call(
            JavaType::qualified(version, &["java", "util"], owner),
            Identifier::known(method),
            arguments.into_iter().collect(),
        )
    }
}

impl TypeRefRender for JavaTypeRef<'_> {
    type Output = Result<ApiType>;

    fn primitive(&mut self, primitive: BindingPrimitive) -> Self::Output {
        Primitive::try_from(primitive).map(|primitive| ApiType::primitive(primitive, self.version))
    }

    fn string(&mut self) -> Self::Output {
        Ok(ApiType::reference(TypeName::named(TypeIdentifier::known(
            "String",
            self.version,
        ))))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Output {
        // Java does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString type reached Java renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(ApiType {
            value: TypeName::array(TypeName::primitive(Primitive::Byte)),
            boxed: TypeName::array(TypeName::primitive(Primitive::Byte)),
            semantics: ValueSemantics::Array,
        })
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        self.context
            .record(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "record type was not found in render context",
            ))
            .and_then(|record| Name::new(record.name()).type_name(self.version))
            .map(|name| self.qualify(name))
            .map(ApiType::reference)
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        self.context
            .enumeration(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))
            .and_then(|enumeration| Name::new(enumeration.name()).type_name(self.version))
            .map(|name| self.qualify(name))
            .map(ApiType::reference)
    }

    fn class(&mut self, id: ClassId) -> Self::Output {
        self.context
            .class(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "class type was not found in render context",
            ))
            .and_then(|class| Name::new(class.name()).type_name(self.version))
            .map(|name| self.qualify(name))
            .map(ApiType::reference)
    }

    fn callback(&mut self, id: CallbackId) -> Self::Output {
        self.context
            .callback(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "callback type was not found in render context",
            ))
            .and_then(|callback| Name::new(callback.name()).type_name(self.version))
            .map(|name| self.qualify(name))
            .map(ApiType::reference)
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        self.context
            .custom_type(id)
            .ok_or(JavaHost::unsupported("custom type without declaration"))?
            .representation()
            .render_with(self)
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        let ty = match kind {
            BuiltinType::Duration => {
                JavaType::qualified(self.version, &["java", "time"], "Duration")
            }
            BuiltinType::SystemTime => {
                JavaType::qualified(self.version, &["java", "time"], "Instant")
            }
            BuiltinType::Uuid => JavaType::qualified(self.version, &["java", "util"], "UUID"),
            BuiltinType::Url => JavaType::qualified(self.version, &["java", "net"], "URI"),
        };
        Ok(ApiType::reference(ty))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        let inner = inner?;
        Ok(ApiType {
            value: JavaType::optional(inner.boxed.clone(), self.version),
            boxed: JavaType::optional(inner.boxed, self.version),
            semantics: ValueSemantics::Optional(Box::new(inner.semantics)),
        })
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        let element = element?;
        match element.semantics.primitive() {
            Some(primitive) => Ok(ApiType {
                value: TypeName::array(TypeName::primitive(primitive)),
                boxed: TypeName::array(TypeName::primitive(primitive)),
                semantics: ValueSemantics::Array,
            }),
            None => Ok(ApiType {
                value: TypeName::parameterized(
                    JavaType::qualified(self.version, &["java", "util"], "List"),
                    [element.boxed.clone()],
                ),
                boxed: TypeName::parameterized(
                    JavaType::qualified(self.version, &["java", "util"], "List"),
                    [element.boxed],
                ),
                semantics: ValueSemantics::Sequence(Box::new(element.semantics)),
            }),
        }
    }

    fn tuple(&mut self, _elements: Vec<Self::Output>) -> Self::Output {
        Err(JavaHost::unsupported("tuple type"))
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        let ok = ok?;
        let err = err?;
        let value = TypeName::parameterized(
            TypeName::named(ResultClass::type_name(self.version)),
            [ok.boxed.clone(), err.boxed.clone()],
        );
        Ok(ApiType {
            boxed: value.clone(),
            value,
            semantics: ValueSemantics::Result {
                ok: Box::new(ok.semantics),
                err: Box::new(err.semantics),
            },
        })
    }

    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output {
        let key = key?;
        let value = value?;
        let ty = TypeName::parameterized(
            JavaType::qualified(self.version, &["java", "util"], "Map"),
            [key.boxed, value.boxed],
        );
        Ok(ApiType {
            boxed: ty.clone(),
            value: ty,
            semantics: ValueSemantics::Map {
                key: Box::new(key.semantics),
                value: Box::new(value.semantics),
            },
        })
    }
}

impl JavaTypeRef<'_> {
    fn qualify(&self, name: TypeIdentifier) -> TypeName {
        match self.package {
            Some(package) => package.type_name(name),
            None => TypeName::named(name),
        }
    }
}
