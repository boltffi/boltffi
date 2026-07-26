use boltffi_binding::{ExecutionDecl, ExportedCallable, NativeSymbol, Receive};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, names},
};

pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    rust_call: RustCall,
    receiver: ReceiverTokens,
    visibility: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct RustCall {
    owner: syn::Ident,
    target: RustCallTarget,
}

pub struct ReceiverTokens {
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    requires_failure_return: bool,
}

enum RustCallTarget {
    Constant(syn::Ident),
    Function(syn::Ident),
    FunctionPath(TokenStream),
    Associated {
        owner: TokenStream,
        method: syn::Ident,
    },
    Method {
        receiver: syn::Ident,
        method: syn::Ident,
    },
    ClassMethod {
        class: TokenStream,
        handle: syn::Ident,
        receiver: ClassReceiverBinding,
        receive: Receive,
        method: syn::Ident,
    },
}

pub enum ClassReceiverBinding {
    Raw(syn::Ident),
    Retained(syn::Ident),
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
        symbol: &'lowered NativeSymbol,
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: RustCall,
        receiver: ReceiverTokens,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            symbol,
            callable,
            source,
            rust_call,
            receiver,
            visibility,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error> {
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return Err(Error::UnsupportedExpansion("async exported callable"));
        }

        let cfg = S::cfg_attr();
        let failure = self.failure()?;
        let wrapper_arguments = <wrapper::arguments::SyncRenderer as Render<S, _>>::render(
            wrapper::arguments::SyncRenderer,
            wrapper::arguments::Input::new(self.callable, self.source, failure, self.expansion),
        )?;
        let rust_call = self
            .rust_call
            .expression(wrapper_arguments.rust_arguments());
        let has_ffi_parameters = !self.receiver.ffi_parameters.is_empty()
            || !wrapper_arguments.ffi_parameters().is_empty();
        let rust_invocation = wrapper::returns::RustInvocation::new(
            self.rust_call.owner,
            rust_call,
            self.receiver
                .conversions
                .into_iter()
                .chain(wrapper_arguments.conversions().iter().cloned())
                .collect(),
            wrapper_arguments
                .writebacks()
                .iter()
                .cloned()
                .chain(self.receiver.writebacks)
                .collect(),
            has_ffi_parameters,
        );
        let return_tokens = <wrapper::returns::Renderer as Render<S, _>>::render(
            wrapper::returns::Renderer,
            wrapper::returns::Input::new(
                self.callable.returns(),
                self.callable.error(),
                self.source.returns(),
                self.source.returns().written_type()?,
                rust_invocation,
                self.expansion,
            ),
        )?;
        let export_ident = names::Symbol::new(self.symbol).ident();
        let ffi_parameters = self
            .receiver
            .ffi_parameters
            .iter()
            .chain(wrapper_arguments.ffi_parameters().iter())
            .chain(return_tokens.ffi_parameters().iter())
            .collect::<Vec<_>>();
        let argument_items = wrapper_arguments.items();
        let return_items = return_tokens.items();
        let return_type = return_tokens.return_type();
        let body = return_tokens.body();
        let visibility = self.visibility;
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });

        Ok(quote! {
            #(#argument_items)*
            #(#return_items)*
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #export_ident(#(#ffi_parameters),*) #return_type {
                #body
            }
        })
    }

    fn failure(&self) -> Result<TokenStream, Error> {
        match self
            .callable
            .params()
            .iter()
            .any(wrapper::param::requires_failure_return::<S>)
            || self.receiver.requires_failure_return()
        {
            true => <wrapper::returns::Failure as Render<S, _>>::render(
                wrapper::returns::Failure,
                wrapper::returns::FailureInput::new(
                    self.callable.returns(),
                    self.callable.error(),
                    self.source.returns(),
                    self.expansion,
                ),
            ),
            false => Ok(TokenStream::new()),
        }
    }
}

impl RustCall {
    pub fn constant(constant: syn::Ident) -> Self {
        Self {
            owner: constant.clone(),
            target: RustCallTarget::Constant(constant),
        }
    }

    pub fn function(function: syn::Ident) -> Self {
        Self {
            owner: function.clone(),
            target: RustCallTarget::Function(function),
        }
    }

    pub fn function_path(owner: syn::Ident, path: TokenStream) -> Self {
        Self {
            owner,
            target: RustCallTarget::FunctionPath(path),
        }
    }

    pub fn associated(owner: TokenStream, method: syn::Ident) -> Self {
        Self {
            owner: method.clone(),
            target: RustCallTarget::Associated { owner, method },
        }
    }

    pub fn method(receiver: syn::Ident, method: syn::Ident) -> Self {
        Self {
            owner: method.clone(),
            target: RustCallTarget::Method { receiver, method },
        }
    }

    pub fn class_method(
        class: TokenStream,
        handle: syn::Ident,
        receiver: ClassReceiverBinding,
        receive: Receive,
        method: syn::Ident,
    ) -> Result<Self, Error> {
        match receive {
            Receive::ByRef | Receive::ByMutRef => Ok(Self {
                owner: method.clone(),
                target: RustCallTarget::ClassMethod {
                    class,
                    handle,
                    receiver,
                    receive,
                    method,
                },
            }),
            Receive::ByValue => Err(Error::UnsupportedExpansion("owned class receiver")),
            _ => Err(Error::UnsupportedExpansion("unknown class receiver mode")),
        }
    }

    pub fn expression(&self, arguments: &[TokenStream]) -> TokenStream {
        match &self.target {
            RustCallTarget::Constant(constant) => {
                debug_assert!(arguments.is_empty());
                quote! { #constant }
            }
            RustCallTarget::Function(function) => quote! { #function(#(#arguments),*) },
            RustCallTarget::FunctionPath(path) => quote! { #path(#(#arguments),*) },
            RustCallTarget::Associated { owner, method } => {
                quote! { #owner::#method(#(#arguments),*) }
            }
            RustCallTarget::Method { receiver, method } => {
                quote! { #receiver.#method(#(#arguments),*) }
            }
            RustCallTarget::ClassMethod {
                class,
                handle,
                receiver,
                receive,
                method,
            } => {
                Self::class_method_expression(*receive, class, handle, receiver, method, arguments)
            }
        }
    }

    pub fn awaited_expression(&self, arguments: &[TokenStream]) -> TokenStream {
        match &self.target {
            RustCallTarget::ClassMethod {
                class,
                handle,
                receiver,
                receive,
                method,
            } => Self::class_method_awaited_expression(
                *receive, class, handle, receiver, method, arguments,
            ),
            _ => {
                let expression = self.expression(arguments);
                quote! { #expression.await }
            }
        }
    }

    pub fn owner(&self) -> &syn::Ident {
        &self.owner
    }

    fn class_method_expression(
        receive: Receive,
        class: &TokenStream,
        handle: &syn::Ident,
        receiver: &ClassReceiverBinding,
        method: &syn::Ident,
        arguments: &[TokenStream],
    ) -> TokenStream {
        let receiver = receiver.access(receive, class, handle);
        quote! {
            {
                #receiver
                #handle.#method(#(#arguments),*)
            }
        }
    }

    fn class_method_awaited_expression(
        receive: Receive,
        class: &TokenStream,
        handle: &syn::Ident,
        receiver: &ClassReceiverBinding,
        method: &syn::Ident,
        arguments: &[TokenStream],
    ) -> TokenStream {
        let receiver = receiver.access(receive, class, handle);
        quote! {
            {
                #receiver
                #handle.#method(#(#arguments),*).await
            }
        }
    }
}

impl ClassReceiverBinding {
    fn access(&self, receive: Receive, class: &TokenStream, receiver: &syn::Ident) -> TokenStream {
        match (self, receive) {
            (Self::Raw(handle), Receive::ByRef) => quote! {
                let #receiver: &#class = unsafe {
                    #handle::shared(#receiver as usize as *mut #handle)
                };
            },
            (Self::Raw(handle), Receive::ByMutRef) => quote! {
                let #receiver: &mut #class = unsafe {
                    #handle::mutable(#receiver as usize as *mut #handle)
                };
            },
            (Self::Retained(handle), Receive::ByRef) => quote! {
                let #receiver: &#class = #handle.shared();
            },
            (Self::Retained(handle), Receive::ByMutRef) => quote! {
                let mut #handle = #handle;
                let #receiver: &mut #class = #handle.mutable();
            },
            _ => unreachable!("class receiver mode is validated before RustCall construction"),
        }
    }
}

impl ReceiverTokens {
    pub fn none() -> Self {
        Self {
            ffi_parameters: Vec::new(),
            conversions: Vec::new(),
            writebacks: Vec::new(),
            requires_failure_return: false,
        }
    }

    pub fn new(
        ffi_parameters: Vec<TokenStream>,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        requires_failure_return: bool,
    ) -> Self {
        Self {
            ffi_parameters,
            conversions,
            writebacks,
            requires_failure_return,
        }
    }

    fn requires_failure_return(&self) -> bool {
        self.requires_failure_return
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
}
