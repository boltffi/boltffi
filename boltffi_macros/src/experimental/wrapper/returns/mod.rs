use boltffi_binding::{CodecNode, DirectValueType, ErrorDecl, OutOfRust, ReturnDecl, ReturnPlan};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Type;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::{InfallibleVoidReturn, RenderSurface},
    wrapper::{self, Render, names},
};

pub struct Renderer;
pub struct Failure;

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

    pub fn function(
        function: syn::Ident,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        arguments: Vec<TokenStream>,
    ) -> Self {
        let call = quote! { #function(#(#arguments),*) };
        Self::new(
            function,
            call,
            conversions,
            writebacks,
            !arguments.is_empty(),
        )
    }
}

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
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

pub struct FailureInput<'expansion, 'lowered, S: RenderSurface> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> FailureInput<'expansion, 'lowered, S> {
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

impl<'expansion, 'lowered, S> Render<S, Input<'expansion, 'lowered, S>> for Renderer
where
    S: RenderSurface,
    closure::Renderer: Render<S, closure::Input<'expansion, 'lowered, S>, Output = Tokens>,
    encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input, Output = Tokens>,
    fallible::Renderer: Render<S, fallible::Input<'expansion, 'lowered, S>, Output = Tokens>,
    handle::Value: Render<
            S,
            handle::ValueInput<'expansion, 'lowered, S, S::HandleCarrier>,
            Output = handle::ValueTokens,
        >,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return <fallible::Renderer as Render<S, _>>::render(
                fallible::Renderer,
                fallible::Input::new(
                    input.returns,
                    input.error,
                    input.source,
                    input.invocation,
                    input.expansion,
                ),
            );
        }

        if let ReturnPlan::ClosureViaOutPointer(closure) = input.returns.plan() {
            return <closure::Renderer as Render<S, _>>::render(
                closure::Renderer,
                closure::Input::new(
                    closure,
                    input.source.closure(closure.presence())?,
                    input.invocation,
                    input.expansion,
                ),
            );
        }

        let RustInvocation {
            span,
            call,
            conversions,
            writebacks,
            has_ffi_parameters,
            ..
        } = input.invocation;
        let locals = names::Locals::new(span);
        match input.returns.plan() {
            ReturnPlan::Void
                if S::INFALLIBLE_VOID_RETURN == InfallibleVoidReturn::Direct
                    && !has_ffi_parameters
                    && conversions.is_empty()
                    && writebacks.is_empty() =>
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
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
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
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
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
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = locals.result();
                let encoded_input = match input.source.borrowed_value()? {
                    true => {
                        encoded::Input::borrowed(codec, *shape, result.clone(), input.expansion)
                    }
                    false => encoded::Input::new(codec, *shape, result.clone(), input.expansion),
                };
                let encoded =
                    <encoded::Renderer as Render<S, _>>::render(encoded::Renderer, encoded_input)?;
                let type_annotation =
                    match wrapper::encoded::Outgoing::new(codec.root(), input.expansion)
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
                let handle_return = input.source.handle_return(target, *presence)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding handle return requires a source return type",
                ))?;
                let result = locals.result();
                let handle = <handle::Value as Render<S, _>>::render(
                    handle::Value,
                    handle::ValueInput::new(
                        input.expansion,
                        target,
                        *carrier,
                        *presence,
                        result.clone(),
                        handle_return,
                    ),
                )?;
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
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                input.source.scalar_option(*primitive)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding scalar option return requires a source return type",
                ))?;
                let result = locals.result();
                let optional = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(*primitive, result.clone()),
                )?;
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
                let element = input.source.direct_vec_element_type()?;
                let result = locals.result();
                let sequence = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Input::new(result.clone(), element),
                )?;
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
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
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
            _ => Err(Error::UnsupportedExpansion("unknown return")),
        }
    }
}

impl<'expansion, 'lowered, S> Render<S, FailureInput<'expansion, 'lowered, S>> for Failure
where
    S: RenderSurface,
    direct_vec::Failure: Render<S, direct_vec::FailureInput, Output = TokenStream>,
    encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    handle::Failure: Render<S, handle::FailureInput<S::HandleCarrier>, Output = TokenStream>,
    scalar_option::Failure: Render<S, scalar_option::FailureInput, Output = TokenStream>,
{
    type Output = TokenStream;

    fn render(self, input: FailureInput<'expansion, 'lowered, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return ErrorFailure::new(input.error, input.source, input.expansion).tokens();
        }

        match input.returns.plan() {
            ReturnPlan::Void => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(quote! {
                    return <#ty as ::core::default::Default>::default();
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = input
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
                let empty = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Empty::new(*shape),
                )?;
                let value = empty.value();
                Ok(quote! {
                    return #value;
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                <scalar_option::Failure as Render<S, _>>::render(
                    scalar_option::Failure,
                    scalar_option::FailureInput::new(*primitive),
                )
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                <direct_vec::Failure as Render<S, _>>::render(
                    direct_vec::Failure,
                    direct_vec::FailureInput,
                )
            }
            ReturnPlan::HandleViaReturnSlot {
                target, carrier, ..
            } => <handle::Failure as Render<S, _>>::render(
                handle::Failure,
                handle::FailureInput::new(target.clone(), *carrier),
            ),
            ReturnPlan::DirectViaOutPointer { .. } => {
                let rust_type = input
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
            _ => Err(Error::UnsupportedExpansion("return failure")),
        }
    }
}

struct ErrorFailure<'expansion, 'lowered, S: RenderSurface> {
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> ErrorFailure<'expansion, 'lowered, S> {
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

    fn tokens(self) -> Result<TokenStream, Error>
    where
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    {
        match self.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. }
                if matches!(codec.root(), CodecNode::String) =>
            {
                let error = names::Locals::new(proc_macro2::Span::call_site()).error();
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::string(codec, *shape, error.clone(), self.expansion),
                )?;
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

    fn typed_encoded_error(self, shape: S::BufferShape) -> Result<TokenStream, Error>
    where
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    {
        self.source.fallible()?;
        let empty = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Empty::new(shape),
        )?;
        let value = empty.value();
        Ok(quote! {
            return #value;
        })
    }
}
