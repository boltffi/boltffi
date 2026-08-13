use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, ConstantDecl, ConstantOwner, ConstantValueDecl, DefaultValue, ExportedCallable,
    Native, NativeSymbol, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        name_style::Name,
        render::{
            Documentation,
            default_value::DefaultExpression,
            function::{ExportedCall, ExportedCallRenderer},
            type_name::KotlinType,
        },
        syntax::{Expression, Identifier, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/constant.kt", escape = "none")]
struct ConstantTemplate<'constant> {
    constant: &'constant Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    documentation: Documentation,
    body: Body,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AssociatedConstants(Vec<Constant>);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Inline {
    Stored(InlineValue),
    Computed(InlineValue),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    Inline(Inline),
    Accessor(Box<ExportedCall>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InlineValue {
    name: Identifier,
    ty: TypeName,
    value: Expression,
}

#[derive(Clone, Copy)]
enum Scope {
    TopLevel,
    Associated,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build(declaration, Scope::TopLevel, host, bridge, context)
    }

    fn from_associated(
        declaration: &ConstantDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build(declaration, Scope::Associated, host, bridge, context)
    }

    fn build(
        declaration: &ConstantDecl<Native>,
        scope: Scope,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = scope.name(declaration)?;
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                documentation: Documentation::new(declaration.meta().doc()),
                body: Body::Inline(scope.inline(InlineValue::new(name, ty, value, context)?)),
            }),
            ConstantValueDecl::Accessor { symbol, callable } => Ok(Self {
                documentation: Documentation::new(declaration.meta().doc()),
                body: Body::Accessor(Box::new(Self::build_accessor(
                    name, symbol, callable, host, bridge, context,
                )?)),
            }),
            _ => Err(KotlinHost::unsupported("unknown constant value")),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(self.render_source()?))
    }

    fn render_associated(&self, prefix: &str) -> Result<String> {
        self.render_source().map(|source| {
            source
                .lines()
                .map(|line| format!("{prefix}{line}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    fn render_source(&self) -> Result<String> {
        Ok(ConstantTemplate { constant: self }
            .render()?
            .trim()
            .to_owned())
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn body(&self) -> &Body {
        &self.body
    }

    fn build_accessor(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<ExportedCall> {
        let call = ExportedCallRenderer::new(host, bridge, context)
            .exported(name, symbol, callable, None)?;
        if call.async_call().is_some() {
            return Err(KotlinHost::unsupported("async constant accessor"));
        }
        if call.returns().is_none() {
            return Err(KotlinHost::unsupported("constant accessor without return"));
        }
        Ok(call)
    }
}

impl AssociatedConstants {
    pub(super) fn from_c_style_enum(
        enumeration: &CStyleEnumDecl<Native>,
        host: &KotlinHost,
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        bridge
            .map(|bridge| {
                let variants = enumeration
                    .variants()
                    .iter()
                    .map(|variant| {
                        let name = match enumeration.is_error_payload() {
                            true => Name::new(variant.name()).variant()?,
                            false => Name::new(variant.name()).enum_entry()?,
                        };
                        Ok((variant.name(), name))
                    })
                    .collect::<Result<Vec<_>>>()?;
                context
                    .associated_constants(ConstantOwner::Enum(enumeration.id()))
                    .map(|declaration| {
                        let constant_name = Name::new(declaration.name()).constant()?;
                        let colliding_variant = variants
                            .iter()
                            .find(|(_, variant_name)| variant_name == &constant_name);
                        match colliding_variant {
                            None => Constant::from_associated(declaration, host, bridge, context)
                                .map(Some),
                            Some((variant, _))
                                if matches!(
                                    declaration.owned_enum_variant_alias(),
                                    Some((owner, aliased_variant))
                                        if owner == enumeration.id()
                                            && aliased_variant == *variant
                                ) =>
                            {
                                Ok(None)
                            }
                            Some(_) => Err(Error::KotlinNameCollision {
                                scope: Name::new(enumeration.name()).type_name().to_string(),
                                name: constant_name.to_string(),
                            }),
                        }
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(|constants| constants.into_iter().flatten().collect())
            })
            .transpose()
            .map(|constants| Self(constants.unwrap_or_default()))
    }

    pub(super) fn from_owner(
        owner: ConstantOwner,
        host: &KotlinHost,
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        bridge
            .map(|bridge| {
                context
                    .associated_constants(owner)
                    .map(|constant| Constant::from_associated(constant, host, bridge, context))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()
            .map(|constants| Self(constants.unwrap_or_default()))
    }

    pub(super) fn render(&self, prefix: &str) -> Result<Vec<String>> {
        self.0
            .iter()
            .map(|constant| constant.render_associated(prefix))
            .collect()
    }
}

impl InlineValue {
    fn new(
        name: Identifier,
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            name,
            ty: KotlinType::type_ref(ty, context)?,
            value: DefaultExpression::render(ty, value, context)?,
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn value(&self) -> &Expression {
        &self.value
    }
}

impl Scope {
    fn name(self, declaration: &ConstantDecl<Native>) -> Result<Identifier> {
        match self {
            Self::TopLevel => Name::new(declaration.name()).function(),
            Self::Associated => Name::new(declaration.name()).constant(),
        }
    }

    fn inline(self, value: InlineValue) -> Inline {
        match self {
            Self::TopLevel => Inline::Stored(value),
            Self::Associated => Inline::Computed(value),
        }
    }
}
