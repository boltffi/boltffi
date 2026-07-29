use boltffi_ast::MethodDef;
use boltffi_binding::{
    ExecutionDecl, ExportedCallable, ExportedMethodDecl, InitializerDecl, NativeSymbol,
};
use proc_macro2::TokenStream;
use syn::{Ident, parse_quote};

use crate::{
    experimental::{
        error::Error,
        expansion::Expansion,
        rust_api,
        surface::RenderSurface,
        wrapper::{self, Render, export, names},
    },
    future_runtime,
};

pub trait Owner<'expansion, 'lowered, S: RenderSurface> {
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered>;

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered>;

    fn receiver(
        &self,
        export: ReceiverExport<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>;
}

pub struct ReceiverExport<'expansion, 'lowered, S: RenderSurface> {
    callable: &'lowered ExportedCallable<S>,
    method: Ident,
    failure: ReceiverFailure<'expansion, 'lowered, S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
pub struct ReceiverFailure<'expansion, 'lowered, S: RenderSurface> {
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct Renderer<'expansion, 'lowered, S: RenderSurface, O> {
    owner: O,
    initializers: &'lowered [InitializerDecl<S>],
    methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Export<'lowered, S: RenderSurface> {
    kind: ExportKind,
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source_method: &'lowered MethodDef,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ExportKind {
    Initializer,
    Method,
}

impl<'expansion, 'lowered, S: RenderSurface, O> Renderer<'expansion, 'lowered, S, O> {
    pub fn new(
        owner: O,
        initializers: &'lowered [InitializerDecl<S>],
        methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            owner,
            initializers,
            methods,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, S>,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        let declarations = self.owner.declarations();
        let initializers = self
            .initializers
            .iter()
            .map(|initializer| {
                let source_method = declarations.resolve(initializer.name())?;
                Export::initializer(initializer, source_method).render(&self.owner, self.expansion)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .methods
            .iter()
            .map(|method| {
                let source_method = declarations.resolve(method.name())?;
                Export::method(method, source_method).render(&self.owner, self.expansion)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote::quote! {
            #(#initializers)*
            #(#methods)*
        })
    }
}

impl<'expansion, 'lowered, S: RenderSurface> ReceiverExport<'expansion, 'lowered, S> {
    pub fn new(
        callable: &'lowered ExportedCallable<S>,
        method: Ident,
        failure: ReceiverFailure<'expansion, 'lowered, S>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            callable,
            method,
            failure,
            expansion,
        }
    }

    pub fn callable(&self) -> &'lowered ExportedCallable<S> {
        self.callable
    }

    pub fn method(&self) -> &Ident {
        &self.method
    }

    pub fn failure(&self) -> ReceiverFailure<'expansion, 'lowered, S> {
        self.failure
    }

    pub fn expansion(&self) -> &'expansion Expansion<'lowered, S> {
        self.expansion
    }
}

impl<'expansion, 'lowered, S: RenderSurface> ReceiverFailure<'expansion, 'lowered, S> {
    pub fn new(
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            callable,
            source,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        match self.callable.execution() {
            ExecutionDecl::Synchronous(_) => <wrapper::returns::Failure as Render<S, _>>::render(
                wrapper::returns::Failure,
                wrapper::returns::FailureInput::new(
                    self.callable.returns(),
                    self.callable.error(),
                    self.source.returns(),
                    self.expansion,
                ),
            ),
            ExecutionDecl::Asynchronous(_) => {
                let rust_return_type = self
                    .source
                    .returns()
                    .written_type()?
                    .unwrap_or_else(|| parse_quote! { () });
                let protocol = match self.callable.execution() {
                    ExecutionDecl::Asynchronous(protocol) => protocol,
                    _ => unreachable!(),
                };
                let symbols = S::poll_handle(protocol)?;
                let invalid = future_runtime::invalid(symbols.mobility(), rust_return_type);
                Ok(quote::quote! { return #invalid; })
            }
            _ => Err(Error::UnsupportedExpansion("unknown execution mode")),
        }
    }
}

impl<'lowered, S: RenderSurface> Export<'lowered, S> {
    fn initializer(
        initializer: &'lowered InitializerDecl<S>,
        source_method: &'lowered MethodDef,
    ) -> Self {
        Self {
            kind: ExportKind::Initializer,
            symbol: initializer.symbol(),
            callable: initializer.callable(),
            source_method,
        }
    }

    fn method(
        method: &'lowered ExportedMethodDecl<S, NativeSymbol>,
        source_method: &'lowered MethodDef,
    ) -> Self {
        Self {
            kind: ExportKind::Method,
            symbol: method.target(),
            callable: method.callable(),
            source_method,
        }
    }

    fn render<'expansion, O>(
        self,
        owner: &O,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, S>,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        if self.kind == ExportKind::Initializer && self.callable.receiver().is_some() {
            return Err(Error::SourceSyntaxMismatch(
                "initializer binding unexpectedly has a receiver",
            ));
        }

        let method = names::SourceSpelling::new(&self.source_method.name)
            .ident("source method name is not a Rust identifier")?;
        let source_callable = owner.source_callable(self.source_method);
        let (receiver, rust_call) = owner.receiver(ReceiverExport::new(
            self.callable,
            method,
            ReceiverFailure::new(self.callable, source_callable, expansion),
            expansion,
        ))?;
        let visibility =
            rust_api::VisibilityTokens::new(&self.source_method.source.visibility).into_tokens()?;

        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return <wrapper::async_call::Renderer as Render<S, _>>::render(
                wrapper::async_call::Renderer,
                wrapper::async_call::Input::exported(
                    self.symbol,
                    self.callable,
                    source_callable,
                    self.source_method.execution,
                    wrapper::async_call::ExportCall::new(rust_call, receiver, visibility),
                    expansion,
                ),
            );
        }

        export::Renderer::new(
            self.symbol,
            self.callable,
            source_callable,
            rust_call,
            receiver,
            visibility,
            expansion,
        )
        .render()
    }
}
