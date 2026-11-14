use boltffi_ast::MethodDef;
use boltffi_binding::{
    ExecutionDecl, ExportedCallable, ExportedMethodDecl, InitializerDecl, NativeSymbol,
};
use proc_macro2::TokenStream;
use syn::{Ident, parse_quote};

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{export, names},
};

pub trait Owner<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered>;

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered>;

    fn receiver(
        &self,
        export: ReceiverExport<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>;
}

pub struct ReceiverExport<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    callable: &'lowered ExportedCallable<S>,
    method: Ident,
    failure: ReceiverFailure<'expansion, 'lowered, S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
pub struct ReceiverFailure<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct AssociatedFunctions<'expansion, 'lowered, S: boltffi_binding::SurfaceLower, O> {
    owner: O,
    initializers: &'lowered [InitializerDecl<S>],
    methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Export<'lowered, S: boltffi_binding::SurfaceLower> {
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

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower, O>
    AssociatedFunctions<'expansion, 'lowered, S, O>
{
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
}

impl<'expansion, 'lowered, O> AssociatedFunctions<'expansion, 'lowered, boltffi_binding::Native, O>
where
    O: Owner<'expansion, 'lowered, boltffi_binding::Native>,
{
    pub fn render(self) -> Result<TokenStream, Error> {
        let declarations = self.owner.declarations();
        let initializers = self
            .initializers
            .iter()
            .map(|initializer| {
                let source_method = declarations.resolve(initializer.name())?;
                Export::initializer(initializer, source_method)
                    .render(&self.owner, self.expansion)
                    .map_err(|error| error.in_member(source_method.name.spelling()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .methods
            .iter()
            .map(|method| {
                let source_method = declarations.resolve(method.name())?;
                Export::method(method, source_method)
                    .render(&self.owner, self.expansion)
                    .map_err(|error| error.in_member(source_method.name.spelling()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote::quote! {
            #(#initializers)*
            #(#methods)*
        })
    }
}

impl<'expansion, 'lowered, O> AssociatedFunctions<'expansion, 'lowered, boltffi_binding::Wasm32, O>
where
    O: Owner<'expansion, 'lowered, boltffi_binding::Wasm32>,
{
    pub fn render(self) -> Result<TokenStream, Error> {
        let declarations = self.owner.declarations();
        let initializers = self
            .initializers
            .iter()
            .map(|initializer| {
                let source_method = declarations.resolve(initializer.name())?;
                Export::initializer(initializer, source_method)
                    .render(&self.owner, self.expansion)
                    .map_err(|error| error.in_member(source_method.name.spelling()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .methods
            .iter()
            .map(|method| {
                let source_method = declarations.resolve(method.name())?;
                Export::method(method, source_method)
                    .render(&self.owner, self.expansion)
                    .map_err(|error| error.in_member(source_method.name.spelling()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote::quote! {
            #(#initializers)*
            #(#methods)*
        })
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower>
    ReceiverExport<'expansion, 'lowered, S>
{
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

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower>
    ReceiverFailure<'expansion, 'lowered, S>
{
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
}

impl<'expansion, 'lowered> ReceiverFailure<'expansion, 'lowered, boltffi_binding::Native> {
    pub fn render(self) -> Result<TokenStream, Error> {
        match self.callable.execution() {
            ExecutionDecl::Synchronous(_) => crate::expansion::wrapper::returns::FailureInput::new(
                self.callable.returns(),
                self.callable.error(),
                self.source.returns(),
                self.expansion,
            )
            .render(),
            ExecutionDecl::Asynchronous(_) => {
                let rust_return_type = self
                    .source
                    .returns()
                    .written_type()?
                    .unwrap_or_else(|| parse_quote! { () });
                Ok(quote::quote! {
                    return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
                })
            }
            _ => Err(Error::UnsupportedExpansion("unknown execution mode")),
        }
    }
}

impl<'expansion, 'lowered> ReceiverFailure<'expansion, 'lowered, boltffi_binding::Wasm32> {
    pub fn render(self) -> Result<TokenStream, Error> {
        match self.callable.execution() {
            ExecutionDecl::Synchronous(_) => crate::expansion::wrapper::returns::FailureInput::new(
                self.callable.returns(),
                self.callable.error(),
                self.source.returns(),
                self.expansion,
            )
            .render(),
            ExecutionDecl::Asynchronous(_) => {
                let rust_return_type = self
                    .source
                    .returns()
                    .written_type()?
                    .unwrap_or_else(|| parse_quote! { () });
                Ok(quote::quote! {
                    return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
                })
            }
            _ => Err(Error::UnsupportedExpansion("unknown execution mode")),
        }
    }
}

impl<'lowered, S: boltffi_binding::SurfaceLower> Export<'lowered, S> {
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
}

impl<'lowered> Export<'lowered, boltffi_binding::Native> {
    fn render<'expansion, O>(
        self,
        owner: &O,
        expansion: &'expansion Expansion<'lowered, boltffi_binding::Native>,
    ) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, boltffi_binding::Native>,
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
            return crate::expansion::wrapper::async_call::Input::exported(
                self.symbol,
                self.callable,
                source_callable,
                rust_call,
                receiver,
                visibility,
                expansion,
            )
            .render();
        }
        export::Export::<boltffi_binding::Native>::new(
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

impl<'lowered> Export<'lowered, boltffi_binding::Wasm32> {
    fn render<'expansion, O>(
        self,
        owner: &O,
        expansion: &'expansion Expansion<'lowered, boltffi_binding::Wasm32>,
    ) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, boltffi_binding::Wasm32>,
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
            return crate::expansion::wrapper::async_call::Input::exported(
                self.symbol,
                self.callable,
                source_callable,
                rust_call,
                receiver,
                visibility,
                expansion,
            )
            .render();
        }
        export::Export::<boltffi_binding::Wasm32>::new(
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
