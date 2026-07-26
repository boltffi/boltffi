use boltffi_ast::ConstantDef;
use boltffi_binding::{ConstantDecl, ConstantValueDecl};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, export, names},
};

pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, ConstantDef, ConstantDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
    owner: Option<AssociatedOwner<'lowered>>,
}

struct AssociatedOwner<'source> {
    callable: rust_api::CallableOwner<'source>,
    rust_type: TokenStream,
}

impl<'expansion, 'lowered, S> Renderer<'expansion, 'lowered, S>
where
    S: RenderSurface,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
{
    pub fn new(
        pair: DeclarationPair<'lowered, ConstantDef, ConstantDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            pair,
            expansion,
            owner: None,
        }
    }

    pub fn with_owner(
        mut self,
        owner: rust_api::CallableOwner<'lowered>,
        rust_type: TokenStream,
    ) -> Self {
        self.owner = Some(AssociatedOwner {
            callable: owner,
            rust_type,
        });
        self
    }

    pub fn render(self) -> Result<TokenStream, Error> {
        match self.pair.binding().value() {
            ConstantValueDecl::Inline { .. } => Ok(TokenStream::new()),
            ConstantValueDecl::Accessor { symbol, callable } => {
                let source = self.pair.source();
                let constant = Self::constant_ident(source)?;
                let source_callable = self.owner.as_ref().map_or_else(
                    || rust_api::Callable::constant(source),
                    |owner| rust_api::Callable::associated_constant(source, owner.callable),
                );
                let rust_call = match self.owner {
                    Some(owner) => export::RustCall::associated_constant(owner.rust_type, constant),
                    None => export::RustCall::constant(constant),
                };
                export::Renderer::new(
                    symbol,
                    callable,
                    source_callable,
                    rust_call,
                    export::ReceiverTokens::none(),
                    rust_api::VisibilityTokens::new(&source.source.visibility).into_tokens()?,
                    self.expansion,
                )
                .render()
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown constant value delivery",
            )),
        }
    }

    fn constant_ident(source: &ConstantDef) -> Result<syn::Ident, Error> {
        names::SourceSpelling::new(&source.name)
            .ident("source constant name is not a Rust identifier")
    }
}
