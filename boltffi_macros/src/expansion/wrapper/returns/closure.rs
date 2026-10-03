use boltffi_binding::{
    ClosureForm, ClosureReturn, HandlePresence, Native, OutOfRust, Wasm32, native,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{self, names},
};

use super::{RustInvocation, Tokens};

pub struct Input<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    closure: &'lowered ClosureReturn<S, OutOfRust>,
    source: rust_api::Closure,
    invocation: RustInvocation,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct WriteInput<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    closure: &'lowered ClosureReturn<S, OutOfRust>,
    source: rust_api::Closure,
    value: Ident,
    owner: Ident,
    channel: ClosureReturnChannel,
    span: Span,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
enum ClosureReturnChannel {
    Return,
    Success,
}

pub struct WriteTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    body: TokenStream,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Input<'expansion, 'lowered, S> {
    pub fn new(
        closure: &'lowered ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure,
        invocation: RustInvocation,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            closure,
            source,
            invocation,
            expansion,
        }
    }
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> WriteInput<'expansion, 'lowered, S> {
    pub fn returned(
        closure: &'lowered ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure,
        value: Ident,
        owner: Ident,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self::new(
            closure,
            source,
            value,
            owner,
            ClosureReturnChannel::Return,
            expansion,
        )
    }

    pub fn success(
        closure: &'lowered ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure,
        value: Ident,
        owner: Ident,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self::new(
            closure,
            source,
            value,
            owner,
            ClosureReturnChannel::Success,
            expansion,
        )
    }

    fn new(
        closure: &'lowered ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure,
        value: Ident,
        owner: Ident,
        channel: ClosureReturnChannel,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        let span = owner.span();
        Self {
            closure,
            source,
            value,
            owner,
            channel,
            span,
            expansion,
        }
    }
}

impl ClosureReturnChannel {
    fn suffix(self) -> &'static str {
        match self {
            Self::Return => "closure",
            Self::Success => "success_closure",
        }
    }
}

impl WriteTokens {
    pub fn into_parts(self) -> (Vec<TokenStream>, Vec<TokenStream>, TokenStream) {
        (self.items, self.ffi_parameters, self.body)
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<Tokens, Error> {
        let RustInvocation {
            owner,
            span,
            call,
            conversions,
            writebacks,
            ..
        } = self.invocation;
        let value = names::Locals::new(span).closure();
        let writer = WriteInput::returned(
            self.closure,
            self.source,
            value.clone(),
            owner,
            self.expansion,
        )
        .render()?;
        let (items, ffi_parameters, body) = writer.into_parts();

        Ok(Tokens {
            items,
            ffi_parameters,
            return_type: quote! { -> ::boltffi::__private::FfiStatus },
            body: quote! {
                #(#conversions)*
                let #value = #call;
                #(#writebacks)*
                #body
                ::boltffi::__private::FfiStatus::OK
            },
        })
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<Tokens, Error> {
        let RustInvocation {
            owner,
            span,
            call,
            conversions,
            writebacks,
            ..
        } = self.invocation;
        let value = names::Locals::new(span).closure();
        let writer = WriteInput::returned(
            self.closure,
            self.source,
            value.clone(),
            owner,
            self.expansion,
        )
        .render()?;
        let (items, ffi_parameters, body) = writer.into_parts();

        Ok(Tokens {
            items,
            ffi_parameters,
            return_type: quote! { -> ::boltffi::__private::FfiStatus },
            body: quote! {
                #(#conversions)*
                let #value = #call;
                #(#writebacks)*
                #body
                ::boltffi::__private::FfiStatus::OK
            },
        })
    }
}

impl<'expansion, 'lowered> WriteInput<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<WriteTokens, Error> {
        match self.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => self.render_native(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure return registration",
            )),
        }
    }
}

impl<'expansion, 'lowered> WriteInput<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<WriteTokens, Error> {
        self.render_wasm32()
    }
}

impl<'expansion, 'lowered> WriteInput<'expansion, 'lowered, Native> {
    fn render_native(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(&self.source, self.closure)?;
        let invoke = wrapper::closure::Invoke::<Native>::new(
            self.closure.invoke(),
            self.source.signature(),
            &returned_closure.signature,
            self.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items().to_vec();
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let storage = format_ident!("__BoltffiClosureReturn{}", self.value);
        let channel = self.channel.suffix();
        let registration = names::ReturnedClosureRegistration::new(&self.owner, channel);
        let call = registration.call();
        let release = registration.release();
        let locals = names::Locals::new(self.span);
        let output = locals.return_out();
        let context = locals.closure_context();
        let ffi_parameter_types = invoke_parameters
            .ffi_parameter_types()
            .iter()
            .cloned()
            .chain(return_ffi_parameter_types)
            .collect::<Vec<_>>();
        let ffi_parameters = invoke_parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions();
        let arguments = invoke_parameters.arguments();
        let return_type = return_tokens.return_type();
        let invocation = returned_closure.invocation();
        let call_body = return_tokens.body(quote! { #invocation(#(#arguments),*) });
        let context_type = returned_closure.context_type();
        let context_binding = returned_closure.context_binding(quote! {
            __boltffi_context as *mut #context_type
        });
        let context_value = returned_closure.context_value(&self.value)?;
        let write_present = quote! {
            let #context = Box::into_raw(Box::new(#context_value)) as *mut ::core::ffi::c_void;
            unsafe {
                *#output = #storage {
                    invoke: Some(#call),
                    context: #context,
                    release: Some(#release),
                };
            }
        };
        let write_body = returned_closure.write_body(
            &self.value,
            write_present,
            quote! {
                unsafe {
                    *#output = #storage {
                        invoke: None,
                        context: ::core::ptr::null_mut(),
                        release: None,
                    };
                }
            },
        )?;

        let items = parameter_items
            .into_iter()
            .chain([quote! {
                #[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
                unsafe extern "C" fn #call(
                    __boltffi_context: *mut ::core::ffi::c_void,
                    #(#ffi_parameters),*
                ) #return_type {
                    let mut __boltffi_closure = unsafe { #context_binding };
                    #(#conversions)*
                    #call_body
                }

                #[cfg(any(not(target_arch = "wasm32"), target_os = "wasi"))]
                unsafe extern "C" fn #release(__boltffi_context: *mut ::core::ffi::c_void) {
                    if !__boltffi_context.is_null() {
                        unsafe {
                            drop(Box::from_raw(__boltffi_context as *mut #context_type));
                        }
                    }
                }
            }])
            .collect();

        Ok(WriteTokens {
            items,
            ffi_parameters: vec![quote! { #output: *mut ::core::ffi::c_void }],
            body: quote! {
                #[repr(C)]
                struct #storage {
                    invoke: Option<unsafe extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type>,
                    context: *mut ::core::ffi::c_void,
                    release: Option<unsafe extern "C" fn(*mut ::core::ffi::c_void)>,
                }

                if #output.is_null() {
                    ::boltffi::__private::set_last_error("closure return out pointer is null".to_string());
                    return ::boltffi::__private::FfiStatus::INVALID_ARG;
                }
                let #output = #output as *mut #storage;
                #write_body
            },
        })
    }
}

impl<'expansion, 'lowered> WriteInput<'expansion, 'lowered, Wasm32> {
    fn render_wasm32(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(&self.source, self.closure)?;
        let invoke = wrapper::closure::Invoke::<Wasm32>::new(
            self.closure.invoke(),
            self.source.signature(),
            &returned_closure.signature,
            self.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items().to_vec();
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let registration = self.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), self.span);
        let release = Ident::new(registration.free().name().as_str(), self.span);
        let output = names::Locals::new(self.span).return_out();
        let ffi_parameters = invoke_parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions();
        let arguments = invoke_parameters.arguments();
        let return_type = return_tokens.return_type();
        let invocation = returned_closure.invocation();
        let call_body = return_tokens.body(quote! { #invocation(#(#arguments),*) });
        let context_type = returned_closure.context_type();
        let context_binding = returned_closure.context_binding(quote! {
            __boltffi_context as usize as *mut #context_type
        });
        let context_value = returned_closure.context_value(&self.value)?;
        let write_present = quote! {
            unsafe {
                *#output = Box::into_raw(Box::new(#context_value)) as usize as u32;
            }
        };
        let write_body = returned_closure.write_body(
            &self.value,
            write_present,
            quote! {
                unsafe {
                    *#output = 0;
                }
            },
        )?;

        let items = parameter_items
            .into_iter()
            .chain([quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #call(
                    __boltffi_context: u32,
                    #(#ffi_parameters),*
                ) #return_type {
                    let mut __boltffi_closure = unsafe { #context_binding };
                    #(#conversions)*
                    #call_body
                }

                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #release(__boltffi_context: u32) {
                    if __boltffi_context != 0 {
                        unsafe {
                            drop(Box::from_raw(__boltffi_context as usize as *mut #context_type));
                        }
                    }
                }
            }])
            .collect();

        Ok(WriteTokens {
            items,
            ffi_parameters: vec![quote! { #output: *mut u32 }],
            body: quote! {
                if #output.is_null() {
                    ::boltffi::__private::set_last_error("closure return out pointer is null".to_string());
                    return ::boltffi::__private::FfiStatus::INVALID_ARG;
                }
                #write_body
            },
        })
    }
}

struct ReturnedClosure {
    kind: ReturnedClosureKind,
    form: ClosureForm,
    signature: wrapper::closure::Signature,
}

impl ReturnedClosure {
    fn new<S: boltffi_binding::SurfaceLower>(
        source: &rust_api::Closure,
        closure: &ClosureReturn<S, OutOfRust>,
    ) -> Result<Self, Error> {
        if source.function() != closure.form() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure return form does not match binding closure",
            ));
        }

        let kind = match (closure.presence(), source.form()) {
            (HandlePresence::Required, rust_api::ClosureSourceForm::FunctionPointer) => {
                ReturnedClosureKind::FunctionPointer
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::BoxedDyn) => {
                ReturnedClosureKind::Boxed
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::ImplTrait) => {
                ReturnedClosureKind::ImplTrait
            }
            (HandlePresence::Nullable, rust_api::ClosureSourceForm::NullableBoxedDyn) => {
                ReturnedClosureKind::NullableBoxed
            }
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source closure return form does not match binding closure",
                ));
            }
        };
        let signature = wrapper::closure::Signature::from_source(source.signature())?;

        Ok(Self {
            kind,
            form: closure.form(),
            signature,
        })
    }

    fn invocation(&self) -> TokenStream {
        match self.form {
            ClosureForm::Fn | ClosureForm::FnMut => quote! { __boltffi_closure },
            ClosureForm::FnOnce => quote! { __boltffi_closure },
            _ => quote! { __boltffi_closure },
        }
    }

    fn context_type(&self) -> TokenStream {
        let trait_object = self.trait_object();
        match self.form {
            ClosureForm::Fn | ClosureForm::FnMut => trait_object,
            ClosureForm::FnOnce => quote! { Option<#trait_object> },
            _ => trait_object,
        }
    }

    fn context_value(&self, value: &Ident) -> Result<TokenStream, Error> {
        let trait_object = self.trait_object();
        Ok(match (self.kind, self.form) {
            (ReturnedClosureKind::ImplTrait, ClosureForm::Fn | ClosureForm::FnMut) => {
                quote! { Box::new(#value) as #trait_object }
            }
            (ReturnedClosureKind::ImplTrait, ClosureForm::FnOnce) => {
                quote! { Some(Box::new(#value) as #trait_object) }
            }
            (ReturnedClosureKind::FunctionPointer, ClosureForm::FunctionPointer) => {
                quote! { Box::new(#value) as #trait_object }
            }
            (ReturnedClosureKind::Boxed, ClosureForm::Fn | ClosureForm::FnMut) => {
                quote! { #value }
            }
            (ReturnedClosureKind::Boxed, ClosureForm::FnOnce) => quote! { Some(#value) },
            (ReturnedClosureKind::NullableBoxed, _) => quote! { #value },
            (_, _) => return Err(Error::UnsupportedExpansion("closure return form")),
        })
    }

    fn write_body(
        &self,
        value: &Ident,
        present: TokenStream,
        absent: TokenStream,
    ) -> Result<TokenStream, Error> {
        match self.kind {
            ReturnedClosureKind::ImplTrait
            | ReturnedClosureKind::FunctionPointer
            | ReturnedClosureKind::Boxed => Ok(present),
            ReturnedClosureKind::NullableBoxed => {
                let context_type = self.context_type();
                let present_value = match self.form {
                    ClosureForm::Fn | ClosureForm::FnMut => quote! { #value },
                    ClosureForm::FnOnce => quote! { Some(#value) },
                    _ => return Err(Error::UnsupportedExpansion("closure return form")),
                };
                Ok(quote! {
                    match #value {
                        Some(#value) => {
                            let #value: #context_type = #present_value;
                            #present
                        }
                        None => {
                            #absent
                        }
                    }
                })
            }
        }
    }

    fn context_binding(&self, context: TokenStream) -> TokenStream {
        match self.form {
            ClosureForm::Fn => quote! { &*(#context) },
            ClosureForm::FnMut => quote! { &mut *(#context) },
            ClosureForm::FnOnce => quote! {
                (&mut *(#context)).take().expect("closure already invoked")
            },
            _ => quote! { &*(#context) },
        }
    }

    fn trait_object(&self) -> TokenStream {
        let trait_ident = self.form.trait_ident();
        let parameters = self.signature.parameters();
        let return_type = self.signature.return_tokens();
        quote! { Box<dyn #trait_ident(#(#parameters),*) #return_type + 'static> }
    }
}

#[derive(Clone, Copy)]
enum ReturnedClosureKind {
    ImplTrait,
    FunctionPointer,
    Boxed,
    NullableBoxed,
}

trait ClosureFormTokens {
    fn trait_ident(self) -> Ident;
}

impl ClosureFormTokens for ClosureForm {
    fn trait_ident(self) -> Ident {
        match self {
            ClosureForm::Fn => format_ident!("Fn"),
            ClosureForm::FnMut => format_ident!("FnMut"),
            ClosureForm::FnOnce => format_ident!("FnOnce"),
            _ => format_ident!("Fn"),
        }
    }
}
