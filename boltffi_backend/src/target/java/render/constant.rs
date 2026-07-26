use askama::Template as AskamaTemplate;
use boltffi_binding::{ConstantDecl, ConstantOwner, ConstantValueDecl, Native};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        name_style::Name,
        render::{
            Call, Enumeration, VariantInitialization, default_value::DefaultExpression,
            type_name::JavaType,
        },
        syntax::{Expression, Identifier, Javadoc, TypeIdentifier, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/constant.java", escape = "none")]
struct ConstantTemplate<'constant> {
    constant: &'constant Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    name: Identifier,
    ty: TypeName,
    body: Body,
    doc: Option<Javadoc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    Inline(Expression),
    Accessor(Box<Call>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AssociatedConstants(Vec<Constant>);

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).constant(version)?;
        let doc = declaration.meta().doc().map(Javadoc::new);
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                name,
                ty: JavaType::type_ref(ty, version, context)?,
                body: Body::Inline(match (ty, value) {
                    (
                        boltffi_binding::TypeRef::Enum(id),
                        boltffi_binding::DefaultValue::EnumVariant { variant_name, .. },
                    ) => Enumeration::constant_variant(
                        *id,
                        variant_name,
                        match declaration.owner() {
                            Some(ConstantOwner::Enum(owner)) if owner == *id => {
                                VariantInitialization::OwningType
                            }
                            _ => VariantInitialization::External,
                        },
                        version,
                        context,
                    )?,
                    (_, boltffi_binding::DefaultValue::EnumVariant { .. }) => {
                        return Err(JavaHost::unsupported("enum constant type"));
                    }
                    _ => DefaultExpression::render(ty, value, version)?,
                }),
                doc,
            }),
            ConstantValueDecl::Accessor { symbol, callable } => {
                let helper = Identifier::parse_for(
                    format!("__boltffiReadConstant{}", declaration.id().raw()),
                    version,
                )?;
                let accessor = Call::from_constant_accessor(
                    helper,
                    symbol,
                    callable,
                    bridge,
                    native_owner,
                    version,
                    context,
                )?;
                let ty = accessor
                    .returns()
                    .type_name()
                    .ok_or_else(|| JavaHost::unsupported("constant accessor without return"))?;
                Ok(Self {
                    name,
                    ty,
                    body: Body::Accessor(Box::new(accessor)),
                    doc,
                })
            }
            _ => Err(JavaHost::unsupported("unknown constant value")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        self.extend(Emitted::primary(self.render_member()?))
    }

    pub fn render_member(&self) -> Result<String> {
        Ok(ConstantTemplate { constant: self }.render()?)
    }

    pub fn extend(&self, emitted: Emitted) -> Result<Emitted> {
        let Body::Accessor(call) = &self.body else {
            return Ok(emitted);
        };
        let emitted = call
            .native_forwards()?
            .into_iter()
            .fold(emitted, Emitted::with_aux);
        let emitted = match call.requires_wire_runtime() {
            true => emitted.with_aux(crate::target::java::codec::Runtime::helper()?),
            false => emitted,
        };
        let emitted = match call.requires_direct_vector_runtime() {
            true => emitted.with_aux(crate::target::java::codec::Runtime::direct_vector_helper()?),
            false => emitted,
        };
        match call.requires_async_runtime() {
            true => Ok(emitted.with_aux(crate::target::java::codec::Runtime::async_helper()?)),
            false => Ok(emitted),
        }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn value(&self) -> Option<&Expression> {
        match &self.body {
            Body::Inline(value) => Some(value),
            Body::Accessor(_) => None,
        }
    }

    pub fn accessor(&self) -> Option<&Call> {
        match &self.body {
            Body::Inline(_) => None,
            Body::Accessor(call) => Some(call),
        }
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }
}

impl AssociatedConstants {
    pub(super) fn from_owner(
        owner: ConstantOwner,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        context
            .associated_constants(owner)
            .map(|constant| {
                Constant::from_declaration(constant, bridge, native_owner, version, context)
            })
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    pub(super) fn as_slice(&self) -> &[Constant] {
        &self.0
    }

    pub(super) fn extend(&self, emitted: Emitted) -> Result<Emitted> {
        self.0
            .iter()
            .try_fold(emitted, |emitted, constant| constant.extend(emitted))
    }
}
