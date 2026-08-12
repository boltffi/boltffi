use boltffi_binding::{CodecNode, DirectValueType, ErrorDecl, OutOfRust, ReturnDecl, ReturnPlan};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Type;

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{self, names},
};

pub mod closure;
pub mod direct_vec;
pub mod encoded;
pub mod fallible;
pub mod handle;
pub mod scalar_option;

pub struct RustInvocation {
    owner: syn::Ident,
    span: Span,
    call: TokenStream,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    has_ffi_parameters: bool,
}

impl RustInvocation {
    pub fn new(
        owner: syn::Ident,
        call: TokenStream,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        has_ffi_parameters: bool,
    ) -> Self {
        let span = owner.span();
        Self {
            owner,
            span,
            call,
            conversions,
            writebacks,
            has_ffi_parameters,
        }
    }
}

pub struct Input<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Input<'expansion, 'lowered, S> {
    pub fn new(
        returns: &'lowered ReturnDecl<S, OutOfRust>,
        error: &'lowered ErrorDecl<S, OutOfRust>,
        source: rust_api::Return<'lowered>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            returns,
            error,
            source,
            rust_type,
            invocation,
            expansion,
        }
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    body: TokenStream,
}

pub struct FailureInput<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> FailureInput<'expansion, 'lowered, S> {
    pub fn new(
        returns: &'lowered ReturnDecl<S, OutOfRust>,
        error: &'lowered ErrorDecl<S, OutOfRust>,
        source: rust_api::Return<'lowered>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            returns,
            error,
            source,
            expansion,
        }
    }
}

impl Tokens {
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn return_type(&self) -> &TokenStream {
        &self.return_type
    }

    pub fn body(&self) -> &TokenStream {
        &self.body
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, boltffi_binding::Native> {
    pub fn render(self) -> Result<Tokens, Error> {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return fallible::Input::new(
                self.returns,
                self.error,
                self.source,
                self.invocation,
                self.expansion,
            )
            .render();
        }

        if let ReturnPlan::ClosureViaOutPointer(closure) = self.returns.plan() {
            return closure::Input::new(
                closure,
                self.source.closure(closure.presence())?,
                self.invocation,
                self.expansion,
            )
            .render();
        }

        let RustInvocation {
            span,
            call,
            conversions,
            writebacks,
            has_ffi_parameters,
            ..
        } = self.invocation;
        let locals = names::Locals::new(span);
        match self.returns.plan() {
            ReturnPlan::Void
                if !has_ffi_parameters && conversions.is_empty() && writebacks.is_empty() =>
            {
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: TokenStream::new(),
                    body: quote! {
                        #(#conversions)*
                        #call;
                    },
                })
            }
            ReturnPlan::Void => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> ::boltffi::__private::FfiStatus },
                body: quote! {
                    #(#conversions)*
                    #call;
                    #(#writebacks)*
                    ::boltffi::__private::FfiStatus::OK
                },
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::primitive(*primitive)?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        #call
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #result
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #ty },
                    body,
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct return requires a source return type",
                ))?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        <#rust_type as ::boltffi::__private::Passable>::pack(#call)
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        <#rust_type as ::boltffi::__private::Passable>::pack(#result)
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out },
                    body,
                })
            }
            ReturnPlan::EncodedViaReturnSlot { codec, shape, .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = locals.result();
                let encoded_input = match self.source.borrowed_value()? {
                    true => encoded::Input::borrowed(codec, *shape, result.clone(), self.expansion),
                    false => encoded::Input::new(codec, *shape, result.clone(), self.expansion),
                };
                let encoded = encoded_input.render()?;
                let type_annotation =
                    match wrapper::encoded::Outgoing::new(codec.root(), self.expansion)
                        .has_custom_conversion()
                    {
                        true => TokenStream::new(),
                        false => quote! { : #rust_type },
                    };
                let return_type = encoded.return_type().clone();
                let value = encoded.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result #type_annotation = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => {
                let handle_return = self.source.handle_return(target, *presence)?;
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding handle return requires a source return type",
                ))?;
                let result = locals.result();
                let handle = handle::ValueInput::new(
                    self.expansion,
                    target,
                    *carrier,
                    *presence,
                    result.clone(),
                    handle_return,
                )
                .render()?;
                let return_type = handle.ty();
                let value = handle.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #return_type },
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot {
                primitive,
                enum_target,
            } => {
                self.source.scalar_option(*primitive)?;
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding scalar option return requires a source return type",
                ))?;
                let result = locals.result();
                let optional = match enum_target {
                    Some(_) => scalar_option::Input::enum_payload(*primitive, result.clone()),
                    None => scalar_option::Input::new(*primitive, result.clone()),
                }
                .native()?;
                let return_type = optional.return_type;
                let body = optional.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                let element = self.source.direct_vec_element_type()?;
                let result = locals.result();
                let sequence = direct_vec::Input::new(result.clone(), element).native()?;
                let return_type = sequence.return_type;
                let body = sequence.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct out-pointer return requires a source return type",
                ))?;
                let result = locals.result();
                let out = locals.return_out();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! {
                        #out: *mut <#rust_type as ::boltffi::__private::Passable>::Out
                    }],
                    return_type: TokenStream::new(),
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        if !#out.is_null() {
                            unsafe {
                                ::core::ptr::write(
                                    #out,
                                    <#rust_type as ::boltffi::__private::Passable>::pack(#result),
                                );
                            }
                        }
                    },
                })
            }
            ReturnPlan::EncodedViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("encoded out-pointer return"))
            }
            ReturnPlan::HandleViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("handle out-pointer return"))
            }
            ReturnPlan::ClosureViaOutPointer(_) => {
                Err(Error::UnsupportedExpansion("closure out-pointer return"))
            }
            ReturnPlan::NativeOpaqueRecord { .. } => {
                // The record is boxed and handed to the host as a `*mut c_void`
                // handle. The host reads fields through the generated accessor
                // exports and releases the box through the generated destructor.
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "native opaque record return requires a source return type",
                ))?;
                let result = locals.result();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> *mut ::core::ffi::c_void },
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        ::std::boxed::Box::into_raw(::std::boxed::Box::new(#result))
                            .cast::<::core::ffi::c_void>()
                    },
                })
            }
            _ => Err(Error::UnsupportedExpansion("unknown return")),
        }
    }
}
impl<'expansion, 'lowered> Input<'expansion, 'lowered, boltffi_binding::Wasm32> {
    pub fn render(self) -> Result<Tokens, Error> {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return fallible::Input::new(
                self.returns,
                self.error,
                self.source,
                self.invocation,
                self.expansion,
            )
            .render();
        }

        if let ReturnPlan::ClosureViaOutPointer(closure) = self.returns.plan() {
            return closure::Input::new(
                closure,
                self.source.closure(closure.presence())?,
                self.invocation,
                self.expansion,
            )
            .render();
        }

        let RustInvocation {
            span,
            call,
            conversions,
            writebacks,
            ..
        } = self.invocation;
        let locals = names::Locals::new(span);
        match self.returns.plan() {
            ReturnPlan::Void => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> ::boltffi::__private::FfiStatus },
                body: quote! {
                    #(#conversions)*
                    #call;
                    #(#writebacks)*
                    ::boltffi::__private::FfiStatus::OK
                },
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::primitive(*primitive)?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        #call
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #result
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #ty },
                    body,
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct return requires a source return type",
                ))?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        <#rust_type as ::boltffi::__private::Passable>::pack(#call)
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        <#rust_type as ::boltffi::__private::Passable>::pack(#result)
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out },
                    body,
                })
            }
            ReturnPlan::EncodedViaReturnSlot { codec, shape, .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = locals.result();
                let encoded_input = match self.source.borrowed_value()? {
                    true => encoded::Input::borrowed(codec, *shape, result.clone(), self.expansion),
                    false => encoded::Input::new(codec, *shape, result.clone(), self.expansion),
                };
                let encoded = encoded_input.render()?;
                let type_annotation =
                    match wrapper::encoded::Outgoing::new(codec.root(), self.expansion)
                        .has_custom_conversion()
                    {
                        true => TokenStream::new(),
                        false => quote! { : #rust_type },
                    };
                let return_type = encoded.return_type().clone();
                let value = encoded.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result #type_annotation = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => {
                let handle_return = self.source.handle_return(target, *presence)?;
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding handle return requires a source return type",
                ))?;
                let result = locals.result();
                let handle = handle::ValueInput::new(
                    self.expansion,
                    target,
                    *carrier,
                    *presence,
                    result.clone(),
                    handle_return,
                )
                .render()?;
                let return_type = handle.ty();
                let value = handle.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #return_type },
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot {
                primitive,
                enum_target,
            } => {
                self.source.scalar_option(*primitive)?;
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding scalar option return requires a source return type",
                ))?;
                let result = locals.result();
                let optional = match enum_target {
                    Some(_) => scalar_option::Input::enum_payload(*primitive, result.clone()),
                    None => scalar_option::Input::new(*primitive, result.clone()),
                }
                .wasm32()?;
                let return_type = optional.return_type;
                let body = optional.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                let element = self.source.direct_vec_element_type()?;
                let result = locals.result();
                let sequence = direct_vec::Input::new(result.clone(), element).wasm32()?;
                let return_type = sequence.return_type;
                let body = sequence.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct out-pointer return requires a source return type",
                ))?;
                let result = locals.result();
                let out = locals.return_out();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! {
                        #out: *mut <#rust_type as ::boltffi::__private::Passable>::Out
                    }],
                    return_type: TokenStream::new(),
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        if !#out.is_null() {
                            unsafe {
                                ::core::ptr::write(
                                    #out,
                                    <#rust_type as ::boltffi::__private::Passable>::pack(#result),
                                );
                            }
                        }
                    },
                })
            }
            ReturnPlan::EncodedViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("encoded out-pointer return"))
            }
            ReturnPlan::HandleViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("handle out-pointer return"))
            }
            ReturnPlan::ClosureViaOutPointer(_) => {
                Err(Error::UnsupportedExpansion("closure out-pointer return"))
            }
            ReturnPlan::NativeOpaqueRecord { .. } => {
                // The host binding is gated by the `NativeOpaqueRecords`
                // capability, which no wasm host advertises, but the Rust crate
                // still has to compile for wasm32. Box the record the same way
                // so the expanded crate builds for every target.
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "native opaque record return requires a source return type",
                ))?;
                let result = locals.result();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> *mut ::core::ffi::c_void },
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        ::std::boxed::Box::into_raw(::std::boxed::Box::new(#result))
                            .cast::<::core::ffi::c_void>()
                    },
                })
            }
            _ => Err(Error::UnsupportedExpansion("unknown return")),
        }
    }
}
impl<'expansion, 'lowered> FailureInput<'expansion, 'lowered, boltffi_binding::Native> {
    pub fn render(self) -> Result<TokenStream, Error> {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return ErrorFailure::new(self.error, self.source, self.expansion).tokens();
        }

        match self.returns.plan() {
            ReturnPlan::Void => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::primitive(*primitive)?;
                Ok(quote! {
                    return <#ty as ::core::default::Default>::default();
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = self
                    .source
                    .written_type()?
                    .ok_or(Error::SourceSyntaxMismatch("direct return type is missing"))?;
                Ok(quote! {
                    return unsafe {
                        ::core::mem::MaybeUninit::<
                            <#rust_type as ::boltffi::__private::Passable>::Out
                        >::zeroed().assume_init()
                    };
                })
            }
            ReturnPlan::EncodedViaReturnSlot { shape, .. } => {
                let empty = encoded::Empty::<boltffi_binding::Native>::new(*shape).render()?;
                let value = empty.value();
                Ok(quote! {
                    return #value;
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
                scalar_option::FailureInput::new(*primitive).native()
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => direct_vec::FailureInput.native(),
            ReturnPlan::HandleViaReturnSlot {
                target, carrier, ..
            } => handle::FailureInput::new(target.clone(), *carrier).render(),
            ReturnPlan::DirectViaOutPointer { .. } => {
                let rust_type = self
                    .source
                    .written_type()?
                    .ok_or(Error::SourceSyntaxMismatch("direct return type is missing"))?;
                let output = names::Locals::new(proc_macro2::Span::call_site()).return_out();
                Ok(quote! {
                    if !#output.is_null() {
                        unsafe {
                            ::core::ptr::write(
                                #output,
                                ::core::mem::MaybeUninit::<
                                    <#rust_type as ::boltffi::__private::Passable>::Out
                                >::zeroed().assume_init(),
                            );
                        }
                    }
                    return;
                })
            }
            ReturnPlan::ClosureViaOutPointer(_) => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::NativeOpaqueRecord { .. } => Ok(quote! {
                return ::core::ptr::null_mut();
            }),
            _ => Err(Error::UnsupportedExpansion("return failure")),
        }
    }
}

impl<'expansion, 'lowered> FailureInput<'expansion, 'lowered, boltffi_binding::Wasm32> {
    pub fn render(self) -> Result<TokenStream, Error> {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return ErrorFailure::new(self.error, self.source, self.expansion).tokens();
        }

        match self.returns.plan() {
            ReturnPlan::Void => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::primitive(*primitive)?;
                Ok(quote! {
                    return <#ty as ::core::default::Default>::default();
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = self
                    .source
                    .written_type()?
                    .ok_or(Error::SourceSyntaxMismatch("direct return type is missing"))?;
                Ok(quote! {
                    return unsafe {
                        ::core::mem::MaybeUninit::<
                            <#rust_type as ::boltffi::__private::Passable>::Out
                        >::zeroed().assume_init()
                    };
                })
            }
            ReturnPlan::EncodedViaReturnSlot { shape, .. } => {
                let empty = encoded::Empty::<boltffi_binding::Wasm32>::new(*shape).render()?;
                let value = empty.value();
                Ok(quote! {
                    return #value;
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive, .. } => {
                scalar_option::FailureInput::new(*primitive).wasm32()
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => direct_vec::FailureInput.wasm32(),
            ReturnPlan::HandleViaReturnSlot {
                target, carrier, ..
            } => handle::FailureInput::new(target.clone(), *carrier).render(),
            ReturnPlan::DirectViaOutPointer { .. } => {
                let rust_type = self
                    .source
                    .written_type()?
                    .ok_or(Error::SourceSyntaxMismatch("direct return type is missing"))?;
                let output = names::Locals::new(proc_macro2::Span::call_site()).return_out();
                Ok(quote! {
                    if !#output.is_null() {
                        unsafe {
                            ::core::ptr::write(
                                #output,
                                ::core::mem::MaybeUninit::<
                                    <#rust_type as ::boltffi::__private::Passable>::Out
                                >::zeroed().assume_init(),
                            );
                        }
                    }
                    return;
                })
            }
            ReturnPlan::ClosureViaOutPointer(_) => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::NativeOpaqueRecord { .. } => Ok(quote! {
                return ::core::ptr::null_mut();
            }),
            _ => Err(Error::UnsupportedExpansion("return failure")),
        }
    }
}

struct ErrorFailure<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> ErrorFailure<'expansion, 'lowered, S> {
    fn new(
        error: &'lowered ErrorDecl<S, OutOfRust>,
        source: rust_api::Return<'lowered>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            error,
            source,
            expansion,
        }
    }
}

impl<'expansion, 'lowered> ErrorFailure<'expansion, 'lowered, boltffi_binding::Native> {
    fn tokens(self) -> Result<TokenStream, Error> {
        match self.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. }
                if matches!(codec.root(), CodecNode::String) =>
            {
                let error = names::Locals::new(proc_macro2::Span::call_site()).error();
                let encoded = encoded::Input::string(codec, *shape, error.clone(), self.expansion)
                    .render()?;
                let value = encoded.value();
                Ok(quote! {
                    let #error = String::from("invalid argument");
                    return #value;
                })
            }
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.typed_encoded_error(*shape),
            ErrorDecl::StatusViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("status error failure"))
            }
            _ => Err(Error::UnsupportedExpansion("error failure")),
        }
    }

    fn typed_encoded_error(
        self,
        shape: boltffi_binding::native::BufferShape,
    ) -> Result<TokenStream, Error> {
        self.source.fallible()?;
        let empty = encoded::Empty::<boltffi_binding::Native>::new(shape).render()?;
        let value = empty.value();
        Ok(quote! {
            return #value;
        })
    }
}

impl<'expansion, 'lowered> ErrorFailure<'expansion, 'lowered, boltffi_binding::Wasm32> {
    fn tokens(self) -> Result<TokenStream, Error> {
        match self.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. }
                if matches!(codec.root(), CodecNode::String) =>
            {
                let error = names::Locals::new(proc_macro2::Span::call_site()).error();
                let encoded = encoded::Input::string(codec, *shape, error.clone(), self.expansion)
                    .render()?;
                let value = encoded.value();
                Ok(quote! {
                    let #error = String::from("invalid argument");
                    return #value;
                })
            }
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.typed_encoded_error(*shape),
            ErrorDecl::StatusViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("status error failure"))
            }
            _ => Err(Error::UnsupportedExpansion("error failure")),
        }
    }

    fn typed_encoded_error(
        self,
        shape: boltffi_binding::wasm32::BufferShape,
    ) -> Result<TokenStream, Error> {
        self.source.fallible()?;
        let empty = encoded::Empty::<boltffi_binding::Wasm32>::new(shape).render()?;
        let value = empty.value();
        Ok(quote! {
            return #value;
        })
    }
}
