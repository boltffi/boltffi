use boltffi_binding::{HandlePresence, HandleTarget, Native, Receive, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, names},
};

use super::Tokens;

pub struct Renderer;
struct CallbackCarrier;

pub struct Plan<C> {
    target: HandleTarget,
    carrier: C,
    presence: HandlePresence,
    receive: Receive,
}

pub struct Input<'lowered, C> {
    plan: Plan<C>,
    source: rust_api::Parameter<'lowered>,
    ident: Ident,
    failure: TokenStream,
}

struct CallbackHandleInput {
    ident: Ident,
}

impl CallbackHandleInput {
    fn new(ident: Ident) -> Self {
        Self { ident }
    }
}

impl Render<Native, CallbackHandleInput> for CallbackCarrier {
    type Output = TokenStream;

    fn render(self, input: CallbackHandleInput) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { #ident })
    }
}

impl Render<Wasm32, CallbackHandleInput> for CallbackCarrier {
    type Output = TokenStream;

    fn render(self, input: CallbackHandleInput) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { ::boltffi::__private::CallbackHandle::from_wasm_handle(#ident) })
    }
}

impl<C> Plan<C> {
    pub fn new(
        target: &HandleTarget,
        carrier: C,
        presence: HandlePresence,
        receive: Receive,
    ) -> Self {
        Self {
            target: target.clone(),
            carrier,
            presence,
            receive,
        }
    }
}

impl<'lowered, C> Input<'lowered, C> {
    pub fn new(
        plan: Plan<C>,
        source: rust_api::Parameter<'lowered>,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            plan,
            source,
            ident,
            failure,
        }
    }
}

impl<'lowered> Render<Native, Input<'lowered, native::HandleCarrier>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'lowered, native::HandleCarrier>) -> Result<Self::Output, Error> {
        input.tokens::<Native>()
    }
}

impl<'lowered> Render<Wasm32, Input<'lowered, wasm32::HandleCarrier>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'lowered, wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        input.tokens::<Wasm32>()
    }
}

impl<'lowered, C> Input<'lowered, C> {
    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: RenderSurface<HandleCarrier = C>,
        CallbackCarrier: Render<S, CallbackHandleInput, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        match self.plan.target {
            HandleTarget::Class(_) => self.class_tokens::<S>(),
            HandleTarget::Callback(_) => self.callback_tokens::<S>(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown handle parameter target",
            )),
        }
    }

    fn class_tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: RenderSurface<HandleCarrier = C>,
        CallbackCarrier: Render<S, CallbackHandleInput, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.plan.carrier),
        )?;
        let ident = &self.ident;
        let ffi_type = carrier.ty();
        let class =
            self.source
                .class_handle(&self.plan.target, self.plan.presence, self.plan.receive)?;
        let conversion = self.conversion(&class, carrier.zero())?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type.clone()],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn callback_tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: RenderSurface<HandleCarrier = C>,
        CallbackCarrier: Render<S, CallbackHandleInput, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.plan.carrier),
        )?;
        let ident = &self.ident;
        let ffi_type = carrier.ty();
        let callback = self
            .source
            .callback_object(&self.plan.target, self.plan.presence)?;
        let conversion = callback.conversion::<S>(ident, &self.failure)?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type.clone()],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn conversion(
        &self,
        class: &rust_api::ClassHandle,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let ident = &self.ident;
        let ty = class.ty();
        let handle_type = names::Class::from_type_path(ty)?.handle();
        let handle_pointer = quote! { #ident as usize as *mut #handle_type };
        let failure = &self.failure;
        let null_check = matches!(self.plan.presence, HandlePresence::Required).then(|| {
            quote! {
                if #ident == #zero {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null class handle"));
                    #failure
                }
            }
        });

        Ok(match (self.plan.receive, class.presence()) {
            (Receive::ByValue, HandlePresence::Required) => quote! {
                #null_check
                let #ident: #ty = match unsafe { #handle_type::take(#handle_pointer) } {
                    Some(value) => value,
                    None => {
                        ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": released class handle"));
                        #failure
                    }
                };
            },
            (Receive::ByValue, HandlePresence::Nullable) => quote! {
                let #ident: Option<#ty> = if #ident == #zero {
                    None
                } else {
                    Some(match unsafe { #handle_type::take(#handle_pointer) } {
                        Some(value) => value,
                        None => {
                            ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": released class handle"));
                            #failure
                        }
                    })
                };
            },
            (Receive::ByRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &#ty = unsafe {
                    #handle_type::shared(#handle_pointer)
                };
            },
            (Receive::ByMutRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &mut #ty = unsafe {
                    #handle_type::mutable(#handle_pointer)
                };
            },
            (Receive::ByRef | Receive::ByMutRef, HandlePresence::Nullable) => {
                return Err(Error::UnsupportedExpansion(
                    "nullable borrowed class handle",
                ));
            }
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown class handle receive mode",
                ));
            }
        })
    }
}

impl rust_api::CallbackObject {
    fn conversion<S>(&self, ident: &Ident, failure: &TokenStream) -> Result<TokenStream, Error>
    where
        S: crate::experimental::surface::RenderSurface,
        CallbackCarrier: Render<S, CallbackHandleInput, Output = TokenStream>,
    {
        let handle = names::Parameter::new(ident).handle();
        let handle_binding = <CallbackCarrier as Render<S, _>>::render(
            CallbackCarrier,
            CallbackHandleInput::new(ident.clone()),
        )?;
        let value = self.value_from_handle(&quote! { #handle })?;
        let ty = self.value();
        match self.presence() {
            HandlePresence::Required => Ok(quote! {
                let #handle = #handle_binding;
                if #handle.is_null() {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null callback handle"));
                    #failure
                }
                let #ident: #ty = unsafe {
                    #value
                };
            }),
            HandlePresence::Nullable => Ok(quote! {
                let #handle = #handle_binding;
                let #ident: Option<#ty> = if #handle.is_null() {
                    None
                } else {
                    Some(unsafe {
                        #value
                    })
                };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown callback handle presence",
            )),
        }
    }

    fn value_from_handle(&self, handle: &TokenStream) -> Result<TokenStream, Error> {
        let proxy = self.proxy();
        Ok(match self.form() {
            rust_api::CallbackCarrier::BoxedDyn => {
                quote! {
                    <#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                }
            }
            rust_api::CallbackCarrier::ArcDyn => {
                quote! {
                    <#proxy as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                }
            }
            rust_api::CallbackCarrier::ImplTrait => {
                quote! {
                    *<#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                }
            }
        })
    }
}
