use boltffi_ast::{FunctionDef, Path, PathRoot};
use boltffi_binding::{ExecutionDecl, FunctionDecl};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, names},
};

use super::export;

/// A function wrapper renderer for one target surface.
///
/// The renderer receives a paired source and binding declaration, then renders only the
/// generated extern wrapper. The original Rust function item remains owned by the caller.
pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, FunctionDef, FunctionDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
    rust_path: Option<TokenStream>,
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
    /// Creates a renderer for one paired function declaration.
    pub fn new(
        pair: DeclarationPair<'lowered, FunctionDef, FunctionDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            pair,
            expansion,
            rust_path: None,
        }
    }

    pub fn with_path(mut self, path: &Path) -> Result<Self, Error> {
        self.rust_path = Some(Self::path_tokens(path)?);
        Ok(self)
    }

    /// Renders the generated extern wrapper.
    pub fn render(self) -> Result<TokenStream, Error> {
        let function = self.pair.binding();
        let source = self.pair.source();
        let source_signature = rust_api::Callable::function(source);
        let function_ident = Self::function_ident(source)?;
        let rust_call = self.rust_call(function_ident.clone());
        let visibility =
            rust_api::VisibilityTokens::new(&source.source.visibility).into_tokens()?;
        if matches!(
            function.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return <wrapper::async_call::Renderer as Render<S, _>>::render(
                wrapper::async_call::Renderer,
                wrapper::async_call::Input::new(
                    function,
                    source_signature,
                    rust_call,
                    visibility,
                    self.expansion,
                ),
            );
        }

        export::Renderer::new(
            function.symbol(),
            function.callable(),
            source_signature,
            rust_call,
            export::ReceiverTokens::none(),
            visibility,
            self.expansion,
        )
        .render()
    }

    fn function_ident(source: &FunctionDef) -> Result<Ident, Error> {
        names::SourceSpelling::new(&source.name)
            .ident("source function name is not a Rust identifier")
    }

    fn rust_call(&self, function_ident: Ident) -> export::RustCall {
        match &self.rust_path {
            Some(path) => export::RustCall::function_path(function_ident, path.clone()),
            None => export::RustCall::function(function_ident),
        }
    }

    fn path_tokens(path: &Path) -> Result<TokenStream, Error> {
        let prefix = match path.root {
            PathRoot::Relative => TokenStream::new(),
            PathRoot::Crate => quote! { crate:: },
            PathRoot::Self_ => quote! { self:: },
            PathRoot::Super(levels) => {
                let parents =
                    std::iter::repeat_n(quote! { super }, levels.get()).collect::<Vec<_>>();
                quote! { #(#parents)::*:: }
            }
            PathRoot::Absolute => quote! { :: },
        };
        let segments = path
            .segments
            .iter()
            .map(|segment| {
                if !segment.arguments.is_empty() {
                    return Err(Error::UnsupportedExpansion("generic function path"));
                }
                parse_str::<Ident>(segment.name.as_str()).map_err(|_| {
                    Error::SourceSyntaxMismatch("function path segment is not Rust syntax")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! { #prefix #(#segments)::* })
    }
}
