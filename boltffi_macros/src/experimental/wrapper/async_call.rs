use boltffi_binding::{
    DirectValueType, ErrorDecl, ExecutionDecl, ExportedCallable, FunctionDecl, HandleTarget,
    IncomingParam, IntoRust, Native, NativeSymbol, ParamDecl, ParamPlan, Receive, ReturnPlan,
    Wasm32, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Type, parse_quote};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{
        self, Render, export, names,
        returns::{closure, direct_vec, encoded, fallible, handle, scalar_option},
    },
};

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    rust_call: export::RustCall,
    receiver: export::ReceiverTokens,
    visibility: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
    pub fn new(
        function: &'lowered FunctionDecl<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: export::RustCall,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self::exported(
            function.symbol(),
            function.callable(),
            source,
            rust_call,
            export::ReceiverTokens::none(),
            visibility,
            expansion,
        )
    }

    pub fn exported(
        symbol: &'lowered NativeSymbol,
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: export::RustCall,
        receiver: export::ReceiverTokens,
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
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = TokenStream;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        input.render_native()
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = TokenStream;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        input.render_wasm32()
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Native> {
    fn render_native(self) -> Result<TokenStream, Error> {
        let ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
            poll,
            complete,
            cancel,
            free,
            panic_message,
            ..
        }) = self.callable.execution()
        else {
            return Err(Error::UnsupportedExpansion("native async protocol"));
        };

        AsyncExports::new(
            self.symbol,
            self.callable,
            self.source,
            self.rust_call,
            self.receiver,
            self.visibility,
            self.expansion,
        )?
        .tokens(NativeProtocol {
            poll: poll.clone(),
            complete: complete.clone(),
            cancel: cancel.clone(),
            free: free.clone(),
            panic_message: panic_message.clone(),
        })
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Wasm32> {
    fn render_wasm32(self) -> Result<TokenStream, Error> {
        let ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::PollHandle {
            poll_sync,
            complete,
            cancel,
            free,
            panic_message,
            ..
        }) = self.callable.execution()
        else {
            return Err(Error::UnsupportedExpansion("wasm async protocol"));
        };

        AsyncExports::new(
            self.symbol,
            self.callable,
            self.source,
            self.rust_call,
            self.receiver,
            self.visibility,
            self.expansion,
        )?
        .tokens(WasmProtocol {
            poll_sync: poll_sync.clone(),
            complete: complete.clone(),
            cancel: cancel.clone(),
            free: free.clone(),
            panic_message: panic_message.clone(),
        })
    }
}

struct AsyncExports<'expansion, 'lowered, S: RenderSurface> {
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    rust_call: export::RustCall,
    receiver: export::ReceiverTokens,
    visibility: TokenStream,
    rust_return_type: Type,
    complete: Complete,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S> AsyncExports<'expansion, 'lowered, S>
where
    S: RenderSurface,
    for<'plan> encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'plan, 'lowered, S>, Output = encoded::Tokens>,
    encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    for<'plan> closure::Write:
        Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
        + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
    for<'plan> fallible::Success:
        Render<S, fallible::SuccessInput<'expansion, 'plan, S>, Output = fallible::SuccessTokens>,
    for<'plan> handle::Value: Render<
            S,
            handle::ValueInput<'expansion, 'plan, S, S::HandleCarrier>,
            Output = handle::ValueTokens,
        >,
    wrapper::handle::Carrier: Render<
            S,
            wrapper::handle::CarrierInput<S::HandleCarrier>,
            Output = wrapper::handle::CarrierTokens,
        >,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
        + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
{
    fn new(
        symbol: &'lowered NativeSymbol,
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: export::RustCall,
        receiver: export::ReceiverTokens,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        let rust_return_type = source
            .returns()
            .written_type()?
            .unwrap_or_else(|| parse_quote! { () });
        let complete = Complete::new(
            rust_call.owner().clone(),
            callable,
            source.returns(),
            &rust_return_type,
            expansion,
        )?;
        Ok(Self {
            symbol,
            callable,
            source,
            rust_call,
            receiver,
            visibility,
            rust_return_type,
            complete,
            expansion,
        })
    }

    fn tokens<P>(self, protocol: P) -> Result<TokenStream, Error>
    where
        P: AsyncProtocol,
        wrapper::arguments::AsyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
    {
        let cfg = S::cfg_attr();
        let visibility = &self.visibility;
        let start_ident = names::Symbol::new(self.symbol).ident();
        let rust_return_type = &self.rust_return_type;
        CapturedParameters::new(self.callable.params()).validate()?;
        if !self.receiver.writebacks().is_empty() {
            return Err(Error::UnsupportedExpansion("async receiver writeback"));
        }
        let failure = quote! {
            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
        };
        let params = <wrapper::arguments::AsyncRenderer as Render<S, _>>::render(
            wrapper::arguments::AsyncRenderer,
            wrapper::arguments::Input::new(self.callable, self.source, failure, self.expansion),
        )?;
        let ffi_parameters = self
            .receiver
            .ffi_parameters()
            .iter()
            .chain(params.ffi_parameters())
            .collect::<Vec<_>>();
        let conversions = self
            .receiver
            .conversions()
            .iter()
            .chain(params.conversions())
            .collect::<Vec<_>>();
        let rust_call = self.rust_call.awaited_expression(params.rust_arguments());
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });
        let start = quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #start_ident(#(#ffi_parameters),*) -> ::boltffi::__private::RustFutureHandle {
                #(#conversions)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #rust_call
                })
            }
        };
        let poll = protocol.poll::<S>(visibility, rust_return_type);
        let complete = protocol.complete::<S>(visibility, rust_return_type, self.complete);
        let panic_message = protocol.panic_message::<S>(visibility, rust_return_type);
        let cancel = protocol.cancel::<S>(visibility, rust_return_type);
        let free = protocol.free::<S>(visibility, rust_return_type);

        Ok(quote! {
            #start
            #poll
            #complete
            #panic_message
            #cancel
            #free
        })
    }
}

struct CapturedParameters<'lowered, S: RenderSurface> {
    params: &'lowered [ParamDecl<S, IntoRust>],
}

impl<'lowered, S: RenderSurface> CapturedParameters<'lowered, S> {
    fn new(params: &'lowered [ParamDecl<S, IntoRust>]) -> Self {
        Self { params }
    }

    fn validate(&self) -> Result<(), Error> {
        self.params
            .iter()
            .try_for_each(|param| self.validate_param(param))
    }

    fn validate_param(&self, param: &ParamDecl<S, IntoRust>) -> Result<(), Error> {
        match param.payload() {
            IncomingParam::Value(ParamPlan::Direct { receive, .. })
            | IncomingParam::Value(ParamPlan::Encoded { receive, .. }) => {
                self.validate_receive(*receive)
            }
            IncomingParam::Value(ParamPlan::Handle {
                target, receive, ..
            }) => {
                self.validate_receive(*receive)?;
                self.validate_handle_target(target)
            }
            IncomingParam::Value(ParamPlan::ScalarOption { .. })
            | IncomingParam::Value(ParamPlan::DirectVec { .. }) => Ok(()),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("async parameter shape")),
            IncomingParam::Closure(_) => {
                Err(Error::UnsupportedExpansion("async closure parameter"))
            }
        }
    }

    fn validate_receive(&self, receive: Receive) -> Result<(), Error> {
        match receive {
            Receive::ByValue => Ok(()),
            Receive::ByRef | Receive::ByMutRef => {
                Err(Error::UnsupportedExpansion("async reference parameter"))
            }
            _ => Err(Error::UnsupportedExpansion("async receive mode")),
        }
    }

    fn validate_handle_target(&self, target: &HandleTarget) -> Result<(), Error> {
        match target {
            HandleTarget::Class(_) | HandleTarget::Callback(_) => Ok(()),
            _ => Err(Error::UnsupportedExpansion("async handle parameter")),
        }
    }
}

trait AsyncProtocol {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream;
    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
}

struct NativeProtocol {
    poll: NativeSymbol,
    complete: NativeSymbol,
    cancel: NativeSymbol,
    free: NativeSymbol,
    panic_message: NativeSymbol,
}

impl AsyncProtocol for NativeProtocol {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let ident = names::Symbol::new(&self.poll).ident();
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                callback_data: u64,
                callback: ::boltffi::__private::RustFutureContinuationCallback,
            ) {
                unsafe {
                    ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(
                        handle,
                        callback,
                        callback_data
                    )
                }
            }
        }
    }

    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(
            visibility,
            names::Symbol::new(&self.complete).ident(),
            rust_return_type,
        )
    }

    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .panic_message::<S>(names::Symbol::new(&self.panic_message).ident())
    }

    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .cancel::<S>(names::Symbol::new(&self.cancel).ident())
    }

    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .free::<S>(names::Symbol::new(&self.free).ident())
    }
}

struct WasmProtocol {
    poll_sync: NativeSymbol,
    complete: NativeSymbol,
    cancel: NativeSymbol,
    free: NativeSymbol,
    panic_message: NativeSymbol,
}

impl AsyncProtocol for WasmProtocol {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let ident = names::Symbol::new(&self.poll_sync).ident();
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> i32 {
                unsafe {
                    ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
                }
            }
        }
    }

    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(
            visibility,
            names::Symbol::new(&self.complete).ident(),
            rust_return_type,
        )
    }

    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .panic_message::<S>(names::Symbol::new(&self.panic_message).ident())
    }

    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .cancel::<S>(names::Symbol::new(&self.cancel).ident())
    }

    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        FutureSupport::new(visibility, rust_return_type)
            .free::<S>(names::Symbol::new(&self.free).ident())
    }
}

enum Complete {
    Plain(PlainComplete),
    Fallible(FallibleComplete),
}

struct PlainComplete {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    ok_pattern: TokenStream,
    ok_body: TokenStream,
    err_body: TokenStream,
}

impl Complete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        owner: syn::Ident,
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Return<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        closure::Write:
            Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
        direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
            + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
        fallible::Success: Render<
                S,
                fallible::SuccessInput<'expansion, 'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
            + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
    {
        if !matches!(callable.error(), ErrorDecl::None(_)) {
            return FallibleComplete::new(
                owner,
                callable,
                source.fallible()?,
                rust_return_type,
                expansion,
            )
            .map(Self::Fallible);
        }
        PlainComplete::new(owner, callable, source, rust_return_type, expansion).map(Self::Plain)
    }

    fn tokens<S: RenderSurface>(
        self,
        visibility: &TokenStream,
        ident: syn::Ident,
        rust_return_type: &Type,
    ) -> TokenStream {
        match self {
            Self::Plain(complete) => complete.tokens::<S>(visibility, ident, rust_return_type),
            Self::Fallible(complete) => complete.tokens::<S>(visibility, ident),
        }
    }
}

impl PlainComplete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        owner: syn::Ident,
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Return<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        closure::Write:
            Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
        direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
            + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
            + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
    {
        let result = names::Locals::new(proc_macro2::Span::call_site()).result();
        match callable.returns().plan() {
            ReturnPlan::Void => Ok(Self {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: TokenStream::new(),
                ok_pattern: quote! { _ },
                ok_body: TokenStream::new(),
                err_body: TokenStream::new(),
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let result = syn::Ident::new("result", proc_macro2::Span::call_site());
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #ty },
                    ok_pattern: quote! { #result },
                    ok_body: quote! { #result },
                    err_body: quote! { <#ty as ::core::default::Default>::default() },
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => Ok(Self {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> <#rust_return_type as ::boltffi::__private::Passable>::Out },
                ok_pattern: quote! { #result },
                ok_body: quote! {
                    <#rust_return_type as ::boltffi::__private::Passable>::pack(#result)
                },
                err_body: quote! {
                    unsafe {
                        ::core::mem::MaybeUninit::zeroed().assume_init()
                    }
                },
            }),
            ReturnPlan::DirectViaOutPointer { .. } => {
                let out = names::Locals::new(proc_macro2::Span::call_site()).return_out();
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! {
                        #out: *mut <#rust_return_type as ::boltffi::__private::Passable>::Out
                    }],
                    return_type: TokenStream::new(),
                    ok_pattern: quote! { #result },
                    ok_body: quote! {
                        if !#out.is_null() {
                            unsafe {
                                ::core::ptr::write(
                                    #out,
                                    <#rust_return_type as ::boltffi::__private::Passable>::pack(#result),
                                );
                            }
                        }
                    },
                    err_body: TokenStream::new(),
                })
            }
            ReturnPlan::EncodedViaReturnSlot { codec, shape, .. } => {
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::new(codec, *shape, result.clone(), expansion),
                )?;
                let empty = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Empty::new(*shape),
                )?;
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: encoded.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: encoded.value().clone(),
                    err_body: empty.value().clone(),
                })
            }
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => {
                let handle_return = source.handle_return(target, *presence)?;
                let handle = <handle::Value as Render<S, _>>::render(
                    handle::Value,
                    handle::ValueInput::new(
                        expansion,
                        target,
                        *carrier,
                        *presence,
                        result.clone(),
                        handle_return,
                    ),
                )?;
                let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
                    wrapper::handle::Carrier,
                    wrapper::handle::CarrierInput::new(*carrier),
                )?;
                let ty = handle.ty();
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #ty },
                    ok_pattern: quote! { #result },
                    ok_body: handle.value().clone(),
                    err_body: carrier.zero().clone(),
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                source.scalar_option(*primitive)?;
                let optional = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(*primitive, result.clone()),
                )?;
                let empty = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Empty::new(*primitive),
                )?;
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: optional.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: optional.body().clone(),
                    err_body: empty.body().clone(),
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                let element = source.direct_vec_element_type()?;
                let sequence = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Input::new(result.clone(), element),
                )?;
                let empty = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Empty,
                )?;
                Ok(Self {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: sequence.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: sequence.body().clone(),
                    err_body: empty.body().clone(),
                })
            }
            ReturnPlan::ClosureViaOutPointer(closure) => {
                let source_closure = source.closure(closure.presence())?;
                let writer = <closure::Write as Render<S, _>>::render(
                    closure::Write,
                    closure::WriteInput::returned(
                        closure,
                        source_closure,
                        result.clone(),
                        owner,
                        expansion,
                    ),
                )?;
                let (items, ffi_parameters, body) = writer.into_parts();
                Ok(Self {
                    items,
                    ffi_parameters,
                    return_type: quote! { -> ::boltffi::__private::FfiStatus },
                    ok_pattern: quote! { #result },
                    ok_body: body,
                    err_body: quote! { ::boltffi::__private::FfiStatus::OK },
                })
            }
            _ => Err(Error::UnsupportedExpansion("async return shape")),
        }
    }

    fn tokens<S: RenderSurface>(
        self,
        visibility: &TokenStream,
        ident: syn::Ident,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let Self {
            items,
            ffi_parameters,
            return_type,
            ok_pattern,
            ok_body,
            err_body,
        } = self;
        quote! {
            #(#items)*

            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                out_status: *mut ::boltffi::__private::FfiStatus
                #(, #ffi_parameters)*,
            ) #return_type {
                match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) } {
                    Ok(#ok_pattern) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #ok_body
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = status;
                            }
                        }
                        #err_body
                    }
                }
            }
        }
    }
}

struct FallibleComplete {
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    body: TokenStream,
}

impl FallibleComplete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        owner: syn::Ident,
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Fallible<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        closure::Write:
            Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
        fallible::Success: Render<
                S,
                fallible::SuccessInput<'expansion, 'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
    {
        let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = callable.error() else {
            return Err(Error::UnsupportedExpansion("async error channel"));
        };
        let error = names::Locals::new(proc_macro2::Span::call_site()).error();
        let encoded_error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Input::new(codec, *shape, error.clone(), expansion),
        )?;
        let empty_error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Empty::new(*shape),
        )?;
        let success = <fallible::Success as Render<S, _>>::render(
            fallible::Success,
            fallible::SuccessInput::new(callable.returns(), source, owner, expansion),
        )?;
        let (_, ffi_parameters, success_pattern, success_body) = success.into_parts();
        let return_type = encoded_error.return_type().clone();
        let error_value = encoded_error.value();
        let empty_error_value = empty_error.value();

        Ok(Self {
            ffi_parameters,
            return_type,
            body: quote! {
                match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) } {
                    Ok(Ok(#success_pattern)) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #success_body
                        #empty_error_value
                    }
                    Ok(Err(#error)) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #error_value
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = status;
                            }
                        }
                        #empty_error_value
                    }
                }
            },
        })
    }

    fn tokens<S: RenderSurface>(self, visibility: &TokenStream, ident: syn::Ident) -> TokenStream {
        let cfg = S::cfg_attr();
        let Self {
            ffi_parameters,
            return_type,
            body,
        } = self;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                out_status: *mut ::boltffi::__private::FfiStatus
                #(, #ffi_parameters)*
            ) #return_type {
                #body
            }
        }
    }
}

struct FutureSupport<'render> {
    visibility: &'render TokenStream,
    rust_return_type: &'render Type,
}

impl<'render> FutureSupport<'render> {
    fn new(visibility: &'render TokenStream, rust_return_type: &'render Type) -> Self {
        Self {
            visibility,
            rust_return_type,
        }
    }

    fn panic_message<S: RenderSurface>(&self, ident: syn::Ident) -> TokenStream {
        let cfg = S::cfg_attr();
        let visibility = self.visibility;
        let rust_return_type = self.rust_return_type;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> ::boltffi::__private::FfiBuf {
                match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<#rust_return_type>(handle) } {
                    Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                    None => ::boltffi::__private::FfiBuf::empty(),
                }
            }
        }
    }

    fn cancel<S: RenderSurface>(&self, ident: syn::Ident) -> TokenStream {
        let cfg = S::cfg_attr();
        let visibility = self.visibility;
        let rust_return_type = self.rust_return_type;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
                unsafe {
                    ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
                }
            }
        }
    }

    fn free<S: RenderSurface>(&self, ident: syn::Ident) -> TokenStream {
        let cfg = S::cfg_attr();
        let visibility = self.visibility;
        let rust_return_type = self.rust_return_type;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
                unsafe {
                    ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
                }
            }
        }
    }
}
