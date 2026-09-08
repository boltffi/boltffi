use boltffi_binding::{ExecutionDecl, ExportedCallable};
use proc_macro2::TokenStream;

use crate::expansion::{contract::Expansion, error::Error, rust_api, wrapper};

pub struct Input<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    capture: wrapper::param::Capture,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Input<'expansion, 'lowered, S> {
    pub fn new(
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            callable,
            source,
            failure,
            expansion,
            capture: wrapper::param::Capture::Borrowed,
        }
    }

    /// Async exports outlive the extern function, so parameters that can be
    /// held past the call are retained rather than borrowed.
    fn retaining(mut self) -> Self {
        self.capture = wrapper::param::Capture::Retained;
        self
    }

    fn validate_parameter_count(&self) -> Result<(), Error> {
        let binding = self.callable;
        if binding.params().len() != self.source.parameter_count() {
            return Err(Error::SourceSyntaxMismatch(
                "source parameter count does not match binding parameter count",
            ));
        }

        Ok(())
    }

    fn validate_execution(&self, asynchronous: bool) -> Result<(), Error> {
        match (self.callable.execution(), asynchronous) {
            (ExecutionDecl::Synchronous(_), false) | (ExecutionDecl::Asynchronous(_), true) => {
                Ok(())
            }
            (ExecutionDecl::Synchronous(_), true) => {
                Err(Error::UnsupportedExpansion("sync function"))
            }
            (ExecutionDecl::Asynchronous(_), false) => {
                Err(Error::UnsupportedExpansion("async function"))
            }
            _ => Err(Error::UnsupportedExpansion("unknown execution")),
        }
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, boltffi_binding::Native> {
    pub fn render_sync(self) -> Result<Tokens, Error> {
        self.validate_execution(false)?;
        self.render_parameters()
    }

    pub fn render_async(self) -> Result<Tokens, Error> {
        self.validate_execution(true)?;
        self.retaining().render_parameters()
    }

    fn render_parameters(self) -> Result<Tokens, Error> {
        self.validate_parameter_count()?;
        let params = self
            .callable
            .params()
            .iter()
            .zip(self.source.parameters())
            .map(|(param, source)| {
                wrapper::param::Input::new(param, source, self.failure.clone(), self.expansion)
                    .with_capture(self.capture)
                    .render()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Tokens::from_parameters(params))
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, boltffi_binding::Wasm32> {
    pub fn render_sync(self) -> Result<Tokens, Error> {
        self.validate_execution(false)?;
        self.render_parameters()
    }

    pub fn render_async(self) -> Result<Tokens, Error> {
        self.validate_execution(true)?;
        self.retaining().render_parameters()
    }

    fn render_parameters(self) -> Result<Tokens, Error> {
        self.validate_parameter_count()?;

        let params = self
            .callable
            .params()
            .iter()
            .zip(self.source.parameters())
            .map(|(param, source)| {
                wrapper::param::Input::new(param, source, self.failure.clone(), self.expansion)
                    .with_capture(self.capture)
                    .render()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Tokens::from_parameters(params))
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    rust_arguments: Vec<TokenStream>,
}

impl Tokens {
    fn from_parameters(params: Vec<wrapper::param::Tokens>) -> Self {
        let items = params
            .iter()
            .flat_map(|param| param.items().iter().cloned())
            .collect();
        let ffi_parameters = params
            .iter()
            .flat_map(|param| param.ffi_parameters().iter().cloned())
            .collect();
        let conversions = params
            .iter()
            .flat_map(|param| param.conversions().iter().cloned())
            .collect();
        let writebacks = params
            .iter()
            .flat_map(|param| param.writebacks().iter().cloned())
            .collect();
        let rust_arguments = params
            .iter()
            .map(|param| param.argument().clone())
            .collect();

        Self {
            items,
            ffi_parameters,
            conversions,
            writebacks,
            rust_arguments,
        }
    }
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn writebacks(&self) -> &[TokenStream] {
        &self.writebacks
    }

    pub fn rust_arguments(&self) -> &[TokenStream] {
        &self.rust_arguments
    }
}
