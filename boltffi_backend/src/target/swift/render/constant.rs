use askama::Template;
use boltffi_binding::{ConstantDecl, ConstantValueDecl, ExportedCallable, Native, NativeSymbol};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Emitted, RenderContext, Result},
    target::swift::{
        SwiftHost,
        default_value::DefaultExpression,
        name_style::Name,
        render::{
            Documentation, SwiftType,
            function::{AssociatedFunction, Invocation, ReturnSignature},
        },
        syntax::{Expression, Identifier, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/constant.swift", escape = "none")]
struct ConstantTemplate<'a> {
    constant: &'a Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    documentation: Documentation,
    name: Identifier,
    body: Body,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    Inline {
        ty: TypeName,
        value: Expression,
    },
    Accessor {
        ty: TypeName,
        body: String,
        wire: bool,
    },
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).function()?;
        let body = match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Body::Inline {
                ty: SwiftType::type_ref(ty, context)?,
                value: DefaultExpression::render(ty, value)?,
            },
            ConstantValueDecl::Accessor { symbol, callable } => {
                Self::accessor_body(symbol, callable, bridge, context)?
            }
            _ => return Err(SwiftHost::unsupported("unknown constant value")),
        };
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), ""),
            name,
            body,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = ConstantTemplate { constant: self }
            .render()?
            .trim_end()
            .to_owned();
        source.push_str("\n\n");
        let emitted = Emitted::primary(source);
        match self.requires_wire_runtime() {
            true => Ok(emitted.with_aux(Self::wire_helper()?)),
            false => Ok(emitted),
        }
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn inline(&self) -> bool {
        matches!(self.body, Body::Inline { .. })
    }

    fn accessor(&self) -> bool {
        matches!(self.body, Body::Accessor { .. })
    }

    fn ty(&self) -> &TypeName {
        match &self.body {
            Body::Inline { ty, .. } | Body::Accessor { ty, .. } => ty,
        }
    }

    fn value(&self) -> &Expression {
        match &self.body {
            Body::Inline { value, .. } => value,
            Body::Accessor { .. } => unreachable!(),
        }
    }

    fn body(&self) -> &str {
        match &self.body {
            Body::Accessor { body, .. } => body,
            Body::Inline { .. } => unreachable!(),
        }
    }

    fn requires_wire_runtime(&self) -> bool {
        matches!(self.body, Body::Accessor { wire: true, .. })
    }

    fn wire_helper() -> Result<AuxChunk> {
        AssociatedFunction::wire_helper()
    }

    fn accessor_body(
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Body> {
        let invocation = Invocation::from_callable(symbol, callable, None, bridge, context)?;
        let wire = invocation.requires_wire_runtime();
        let (parameters, body, returns) = invocation.into_rendered("    ")?;
        if !parameters.is_empty() {
            return Err(SwiftHost::unsupported("constant accessor parameters"));
        }
        Self::accessor_return(returns).map(|ty| Body::Accessor { ty, body, wire })
    }

    fn accessor_return(returns: ReturnSignature) -> Result<TypeName> {
        if returns.fallible() {
            return Err(SwiftHost::unsupported("fallible constant accessor"));
        }
        returns
            .type_name()
            .cloned()
            .ok_or(SwiftHost::unsupported("constant accessor without return"))
    }
}
