use askama::Template;
use boltffi_binding::{ConstantDecl, ConstantOwner, ConstantValueDecl, EnumId, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, RenderContext, Result},
};

use super::super::{default_value::DefaultExpression, name_style::Name, type_name};
use super::{Documentation, Function, function::Placement};
use crate::target::dart::syntax::{Literal, TypeFragment};

#[derive(Template)]
#[template(path = "target/dart/constant.dart", escape = "none")]
struct InlineConstantTemplate<'a> {
    documentation: Documentation,
    static_keyword: &'static str,
    ty: &'a TypeFragment,
    name: &'a crate::target::dart::syntax::Identifier,
    value: &'a Literal,
}

pub struct Constant {
    source: String,
}

pub struct AssociatedConstants(Vec<Constant>);

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        associated: bool,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).lower_camel()?;
        let source = match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                let rendered_type = type_name::type_ref(ty, context)?;
                let value = DefaultExpression::render(ty, value, context)?.into_constant()?;
                InlineConstantTemplate {
                    documentation: Documentation::new(declaration.meta().doc(), 0),
                    static_keyword: if associated { "static " } else { "" },
                    ty: &rendered_type,
                    name: &name,
                    value: &value,
                }
                .render()
                .expect("rendering an in-memory Dart constant template cannot fail")
            }
            ConstantValueDecl::Accessor { symbol, callable } => Function::from_callable(
                declaration.name(),
                symbol,
                callable,
                Placement::Getter { associated },
                bridge,
                context,
                declaration.meta().doc(),
            )?
            .source(),
            _ => return super::super::unsupported("unknown constant declaration"),
        };
        Ok(Self { source })
    }

    pub fn render(self) -> Emitted {
        Emitted::primary([self.source, "\n".to_owned()].concat())
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

impl AssociatedConstants {
    pub fn from_owner(
        owner: ConstantOwner,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        context
            .associated_constants(owner)
            .map(|constant| Constant::from_declaration(constant, true, bridge, context))
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    pub fn from_enum(
        owner: EnumId,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        context
            .associated_constants(ConstantOwner::Enum(owner))
            .filter(|constant| constant.owned_enum_variant_alias().is_none())
            .map(|constant| Constant::from_declaration(constant, true, bridge, context))
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Constant> {
        self.0.iter()
    }
}
