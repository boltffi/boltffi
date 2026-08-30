use boltffi_binding::{HandlePresence, HandleTarget, Receive, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::expansion::{
    error::Error,
    rust_api,
    wrapper::{self, names},
};

use super::Tokens;

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
    capture: super::Capture,
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
        capture: super::Capture,
    ) -> Self {
        Self {
            plan,
            source,
            ident,
            failure,
            capture,
        }
    }
}

impl<'lowered> Input<'lowered, native::HandleCarrier> {
    pub fn render(self) -> Result<Tokens, Error> {
        let carrier = wrapper::handle::CarrierTokens::native(self.plan.carrier)?;
        match self.plan.target.clone() {
            HandleTarget::Class(_) => self.class_tokens(carrier),
            HandleTarget::Callback(_) => {
                let handle_binding = {
                    let ident = &self.ident;
                    quote! { #ident }
                };
                self.callback_tokens(carrier, handle_binding)
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown handle parameter target",
            )),
        }
    }
}

impl<'lowered> Input<'lowered, wasm32::HandleCarrier> {
    pub fn render(self) -> Result<Tokens, Error> {
        let carrier = wrapper::handle::CarrierTokens::wasm32(self.plan.carrier)?;
        match self.plan.target.clone() {
            HandleTarget::Class(_) => self.class_tokens(carrier),
            HandleTarget::Callback(_) => {
                let handle_binding = {
                    let ident = &self.ident;
                    quote! { ::boltffi::__private::CallbackHandle::from_wasm_handle(#ident) }
                };
                self.callback_tokens(carrier, handle_binding)
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown handle parameter target",
            )),
        }
    }
}

impl<'lowered, C> Input<'lowered, C> {
    fn class_tokens(self, carrier: wrapper::handle::CarrierTokens) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = carrier.ty();
        let class =
            self.source
                .class_handle(&self.plan.target, self.plan.presence, self.plan.receive)?;
        let conversion = self.conversion(&class, carrier.zero())?;
        let argument = if self.retains_class_handle() {
            quote! { #ident.shared() }
        } else {
            quote! { #ident }
        };

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type.clone()],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument,
        })
    }

    /// Async exports keep a strong reference instead of a bare `&T`, so the
    /// foreign side cannot free the object while the future is alive.
    fn retains_class_handle(&self) -> bool {
        matches!(self.capture, super::Capture::Retained)
            && matches!(self.plan.target, HandleTarget::Class(_))
            && matches!(self.plan.receive, Receive::ByRef)
            && matches!(self.plan.presence, HandlePresence::Required)
    }

    fn callback_tokens(
        self,
        carrier: wrapper::handle::CarrierTokens,
        handle_binding: TokenStream,
    ) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = carrier.ty();
        let callback = self
            .source
            .callback_object(&self.plan.target, self.plan.presence)?;
        let conversion = callback.conversion(ident, &self.failure, handle_binding)?;

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
            (Receive::ByRef, HandlePresence::Required) if self.retains_class_handle() => quote! {
                #null_check
                let #ident = match unsafe { #handle_type::retain(#handle_pointer) } {
                    Some(handle) => handle,
                    None => {
                        ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": released class handle"));
                        #failure
                    }
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
    fn conversion(
        &self,
        ident: &Ident,
        failure: &TokenStream,
        handle_binding: TokenStream,
    ) -> Result<TokenStream, Error> {
        let handle = names::Parameter::new(ident).handle();
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
