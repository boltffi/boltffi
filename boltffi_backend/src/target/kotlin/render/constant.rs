use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ConstantDecl, ConstantValueDecl, DefaultValue, ExportedCallable, Native, NativeSymbol, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        name_style::Name,
        render::{
            default_value::DefaultExpression,
            function::{ExportedCall, ExportedCallRenderer},
            type_name::KotlinType,
        },
        syntax::{Expression, Identifier, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/constant.kt", escape = "none")]
struct ConstantTemplate {
    constant: Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    inline: Option<Inline>,
    accessor: Option<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inline {
    name: Identifier,
    ty: TypeName,
    value: Expression,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                inline: Some(Inline::new(declaration, ty, value, context)?),
                accessor: None,
            }),
            ConstantValueDecl::Accessor { symbol, callable } => Ok(Self {
                inline: None,
                accessor: Some(Self::build_accessor(
                    declaration,
                    symbol,
                    callable,
                    host,
                    bridge,
                    context,
                )?),
            }),
            _ => Err(KotlinHost::unsupported("unknown constant value")),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            ConstantTemplate { constant: self }
                .render()?
                .trim()
                .to_owned(),
        ))
    }

    pub fn inline(&self) -> Option<&Inline> {
        self.inline.as_ref()
    }

    pub fn accessor(&self) -> Option<&ExportedCall> {
        self.accessor.as_ref()
    }

    fn build_accessor(
        declaration: &ConstantDecl<Native>,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<ExportedCall> {
        let call = ExportedCallRenderer::new(host, bridge, context).exported(
            Name::new(declaration.name()).function()?,
            symbol,
            callable,
            Vec::new(),
        )?;
        if call.async_call().is_some() {
            return Err(KotlinHost::unsupported("async constant accessor"));
        }
        if call.returns().is_none() {
            return Err(KotlinHost::unsupported("constant accessor without return"));
        }
        Ok(call)
    }
}

impl Inline {
    fn new(
        declaration: &ConstantDecl<Native>,
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            name: Name::new(declaration.name()).function()?,
            ty: KotlinType::type_ref(ty, context)?,
            value: DefaultExpression::render(ty, value)?,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }
}
