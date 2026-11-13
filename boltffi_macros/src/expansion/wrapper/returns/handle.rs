use boltffi_binding::{HandlePresence, HandleTarget, Native, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

use crate::expansion::{contract::Expansion, error::Error, rust_api, wrapper};

pub struct ValueInput<'expansion, 'lowered, S: boltffi_binding::SurfaceLower, C> {
    expansion: &'expansion Expansion<'lowered, S>,
    target: &'lowered HandleTarget,
    carrier: C,
    presence: HandlePresence,
    value: syn::Ident,
    handle_return: rust_api::HandleReturn,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower, C>
    ValueInput<'expansion, 'lowered, S, C>
{
    pub fn new(
        expansion: &'expansion Expansion<'lowered, S>,
        target: &'lowered HandleTarget,
        carrier: C,
        presence: HandlePresence,
        value: syn::Ident,
        handle_return: rust_api::HandleReturn,
    ) -> Self {
        Self {
            expansion,
            target,
            carrier,
            presence,
            value,
            handle_return,
        }
    }
}

pub struct FailureInput<C> {
    target: HandleTarget,
    carrier: C,
}

impl<C> FailureInput<C> {
    pub fn new(target: HandleTarget, carrier: C) -> Self {
        Self { target, carrier }
    }
}

pub struct ValueTokens {
    ty: TokenStream,
    value: TokenStream,
}

impl ValueTokens {
    pub fn ty(&self) -> &TokenStream {
        &self.ty
    }

    pub fn value(&self) -> &TokenStream {
        &self.value
    }
}

impl<'expansion, 'lowered> ValueInput<'expansion, 'lowered, Native, native::HandleCarrier> {
    pub fn render(self) -> Result<ValueTokens, Error> {
        NativeReturn::new(self).tokens()
    }
}

impl<'expansion, 'lowered> ValueInput<'expansion, 'lowered, Wasm32, wasm32::HandleCarrier> {
    pub fn render(self) -> Result<ValueTokens, Error> {
        WasmReturn::new(self).tokens()
    }
}

impl FailureInput<native::HandleCarrier> {
    pub fn render(self) -> Result<TokenStream, Error> {
        self.validate_target()?;
        let carrier = wrapper::handle::CarrierTokens::native(self.carrier)?;
        let zero = carrier.zero();
        Ok(quote! { return #zero; })
    }
}

impl FailureInput<wasm32::HandleCarrier> {
    pub fn render(self) -> Result<TokenStream, Error> {
        self.validate_target()?;
        let carrier = wrapper::handle::CarrierTokens::wasm32(self.carrier)?;
        let zero = carrier.zero();
        Ok(quote! { return #zero; })
    }
}

impl<C> FailureInput<C> {
    fn validate_target(&self) -> Result<(), Error> {
        if matches!(
            self.target,
            HandleTarget::Class(_) | HandleTarget::Callback(_)
        ) {
            Ok(())
        } else {
            Err(Error::UnsupportedExpansion("unknown handle return failure"))
        }
    }
}

struct NativeReturn<'expansion, 'lowered> {
    input: ValueInput<'expansion, 'lowered, Native, native::HandleCarrier>,
}

impl<'expansion, 'lowered> NativeReturn<'expansion, 'lowered> {
    fn new(input: ValueInput<'expansion, 'lowered, Native, native::HandleCarrier>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<ValueTokens, Error> {
        let carrier = wrapper::handle::CarrierTokens::native(self.input.carrier)?;
        let ty = carrier.ty().clone();
        let zero = carrier.zero();
        let value = match self.input.handle_return {
            rust_api::HandleReturn::Class(ref class) => self.class_value(class, &ty, zero)?,
            rust_api::HandleReturn::Callback(ref callback) => {
                self.callback_value(callback, zero)?
            }
        };

        Ok(ValueTokens { ty, value })
    }

    fn class_value(
        &self,
        class: &Type,
        ty: &TokenStream,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion("non-class handle return"));
        }
        let handle = wrapper::names::Class::handle_of(class);
        let value = &self.input.value;
        match self.input.presence {
            HandlePresence::Required => Ok(quote! {
                #handle::new(#value) as usize as #ty
            }),
            HandlePresence::Nullable => Ok(quote! {
                match #value {
                    Some(__boltffi_value) => {
                        #handle::new(__boltffi_value) as usize as #ty
                    }
                    None => #zero,
                }
            }),
            _ => Err(Error::UnsupportedExpansion("unknown class handle presence")),
        }
    }

    fn callback_value(
        &self,
        callback: &rust_api::CallbackReturn,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let HandleTarget::Callback(id) = self.input.target else {
            return Err(Error::UnsupportedExpansion("non-callback handle return"));
        };
        if !self.input.expansion.has_local_callback(*id) {
            return Err(Error::UnsupportedExpansion(
                "callback return without local callback protocol",
            ));
        }
        let local_handle = callback.local_handle();
        let value = &self.input.value;
        Ok(match (callback.form(), callback.presence()) {
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::from(#value))
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Required) => quote! {
                #local_handle(#value)
            },
            (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::new(#value))
            },
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::from(__boltffi_callback))
                    })
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(#local_handle)
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::new(__boltffi_callback))
                    })
                    .unwrap_or(#zero)
            },
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown callback handle presence",
                ));
            }
        })
    }
}

struct WasmReturn<'expansion, 'lowered> {
    input: ValueInput<'expansion, 'lowered, Wasm32, wasm32::HandleCarrier>,
}

impl<'expansion, 'lowered> WasmReturn<'expansion, 'lowered> {
    fn new(input: ValueInput<'expansion, 'lowered, Wasm32, wasm32::HandleCarrier>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<ValueTokens, Error> {
        let carrier = wrapper::handle::CarrierTokens::wasm32(self.input.carrier)?;
        let ty = carrier.ty().clone();
        let zero = carrier.zero();
        let value = match self.input.handle_return {
            rust_api::HandleReturn::Class(ref class) => self.class_value(class, &ty, zero)?,
            rust_api::HandleReturn::Callback(ref callback) => {
                self.callback_value(callback, zero)?
            }
        };

        Ok(ValueTokens { ty, value })
    }

    fn class_value(
        &self,
        class: &Type,
        ty: &TokenStream,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion("non-class handle return"));
        }
        let handle = wrapper::names::Class::handle_of(class);
        let value = &self.input.value;
        match self.input.presence {
            HandlePresence::Required => Ok(quote! {
                #handle::new(#value) as usize as #ty
            }),
            HandlePresence::Nullable => Ok(quote! {
                match #value {
                    Some(__boltffi_value) => {
                        #handle::new(__boltffi_value) as usize as #ty
                    }
                    None => #zero,
                }
            }),
            _ => Err(Error::UnsupportedExpansion("unknown class handle presence")),
        }
    }

    fn callback_value(
        &self,
        callback: &rust_api::CallbackReturn,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let HandleTarget::Callback(id) = self.input.target else {
            return Err(Error::UnsupportedExpansion("non-callback handle return"));
        };
        if !self.input.expansion.has_local_callback(*id) {
            return Err(Error::UnsupportedExpansion(
                "callback return without local callback protocol",
            ));
        }
        let local_handle = callback.local_handle();
        let value = &self.input.value;
        Ok(match (callback.form(), callback.presence()) {
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::from(#value)).handle() as u32
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Required) => quote! {
                #local_handle(#value).handle() as u32
            },
            (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::new(#value)).handle() as u32
            },
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::from(__boltffi_callback)).handle() as u32
                    })
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| #local_handle(__boltffi_callback).handle() as u32)
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::new(__boltffi_callback)).handle() as u32
                    })
                    .unwrap_or(#zero)
            },
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown callback handle presence",
                ));
            }
        })
    }
}
