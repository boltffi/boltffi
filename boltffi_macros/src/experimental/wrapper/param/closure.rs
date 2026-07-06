use boltffi_ast::{FnSig, ReturnDef, TypeExpr};
use boltffi_binding::{
    ClosureForm, ClosureParameter, ClosureRegistration, ClosureReturn, DirectValueType, ErrorDecl,
    HandlePresence, ImportedCallable, IntoRust, Native, OutgoingParam, ParamPlan, ReturnPlan,
    Wasm32, WritePlan, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, encoded, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    closure: ForeignClosure<'lowered, S>,
    source: rust_api::Closure,
    ident: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
    pub fn new(
        closure: &'lowered ClosureParameter<S, IntoRust>,
        source: rust_api::Closure,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            closure: ForeignClosure::Parameter(closure),
            source,
            ident,
            failure,
            expansion,
        }
    }

    pub fn returned(
        closure: &'lowered ClosureReturn<S, IntoRust>,
        source: rust_api::Closure,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            closure: ForeignClosure::Return(closure),
            source,
            ident,
            failure,
            expansion,
        }
    }
}

#[derive(Clone, Copy)]
enum ForeignClosure<'lowered, S: RenderSurface> {
    Parameter(&'lowered ClosureParameter<S, IntoRust>),
    Return(&'lowered ClosureReturn<S, IntoRust>),
}

impl<'lowered, S: RenderSurface> ForeignClosure<'lowered, S> {
    fn form(self) -> ClosureForm {
        match self {
            Self::Parameter(closure) => closure.form(),
            Self::Return(closure) => closure.form(),
        }
    }

    fn presence(self) -> HandlePresence {
        match self {
            Self::Parameter(closure) => closure.presence(),
            Self::Return(closure) => closure.presence(),
        }
    }

    fn registration(self) -> &'lowered ClosureRegistration<S, IntoRust> {
        match self {
            Self::Parameter(closure) => closure.registration(),
            Self::Return(closure) => closure.registration(),
        }
    }

    fn invoke(self) -> &'lowered ImportedCallable<S> {
        match self {
            Self::Parameter(closure) => closure.invoke(),
            Self::Return(closure) => closure.invoke(),
        }
    }
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        input.render_native()
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        input.render_wasm32()
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Native> {
    fn render_native(self) -> Result<Tokens, Error> {
        match self.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => self.invoke_context(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure registration",
            )),
        }
    }

    fn invoke_context(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let closure_binding =
            ClosureBinding::new(&self.source, self.closure.form(), self.closure.presence())?;
        let invoke = ClosureInvoke::<Native>::new(
            self.closure.invoke(),
            self.source.signature(),
            &closure_binding,
            self.expansion,
        )?;
        let invoke_parameters = invoke.parameters()?;
        let names = names::ClosureRegistration::new(ident);
        let callback = names.call();
        let context = names.context();
        let release = names.release();
        let owner = names.owner();
        let return_tokens = invoke.return_tokens()?;
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let return_call_arguments = return_tokens.call_arguments();
        let return_type = return_tokens.ffi_return_type();
        let setup = &invoke_parameters.setup;
        let call_arguments = &invoke_parameters.call_arguments;
        let call = quote! {
            #(#setup)*
            #callback(#owner.context() #(, #call_arguments)* #(, #return_call_arguments)*)
        };
        let body = return_tokens.body(call);
        let closure = closure_binding.native_binding(NativeBinding {
            ident: ident.clone(),
            callback: callback.clone(),
            context: context.clone(),
            release: release.clone(),
            owner: owner.clone(),
            rust_parameters: invoke_parameters.rust_parameters.clone(),
            body,
            failure: self.failure.clone(),
        })?;
        let function_pointer_type = closure_binding.native_function_pointer_type(
            &invoke_parameters
                .ffi_parameter_types
                .iter()
                .cloned()
                .chain(return_ffi_parameter_types)
                .collect::<Vec<_>>(),
            return_type.clone(),
        )?;
        let release_type = closure_binding.native_release_function_type();

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![
                quote! { #callback: #function_pointer_type },
                quote! { #context: *mut ::core::ffi::c_void },
                quote! { #release: #release_type },
            ],
            ffi_parameter_types: vec![
                function_pointer_type,
                quote! { *mut ::core::ffi::c_void },
                release_type,
            ],
            conversions: vec![closure],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

impl ClosureBinding {
    fn native_release_function_type(&self) -> TokenStream {
        match self {
            Self::NullableBoxed(_, _) => quote! {
                Option<unsafe extern "C" fn(*mut ::core::ffi::c_void)>
            },
            _ => quote! {
                unsafe extern "C" fn(*mut ::core::ffi::c_void)
            },
        }
    }
}

impl ClosureBinding {
    fn native_function_pointer_type(
        &self,
        ffi_parameter_types: &[TokenStream],
        return_type: TokenStream,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::NullableBoxed(_, _) => Ok(quote! {
                Option<unsafe extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type>
            }),
            _ => Ok(quote! {
                unsafe extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type
            }),
        }
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Wasm32> {
    fn render_wasm32(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let closure_binding =
            ClosureBinding::new(&self.source, self.closure.form(), self.closure.presence())?;
        let invoke = ClosureInvoke::<Wasm32>::new(
            self.closure.invoke(),
            self.source.signature(),
            &closure_binding,
            self.expansion,
        )?;
        let invoke_parameters = invoke.parameters()?;
        let return_tokens = invoke.return_tokens()?;
        let registration = self.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), ident.span());
        let free = Ident::new(registration.free().name().as_str(), ident.span());
        let names = names::ClosureRegistration::new(ident);
        let owner = names.owner();
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let return_call_arguments = return_tokens.call_arguments();
        let return_type = return_tokens.ffi_return_type();
        let setup = &invoke_parameters.setup;
        let call_arguments = &invoke_parameters.call_arguments;
        let call_body = quote! {
            #(#setup)*
            #call(#owner.handle() #(, #call_arguments)* #(, #return_call_arguments)*)
        };
        let body = return_tokens.body(call_body);
        let closure = closure_binding.wasm_binding(
            ident,
            &owner,
            &free,
            &invoke_parameters.rust_parameters,
            body,
            &self.failure,
        )?;

        let ffi_parameter_types = invoke_parameters
            .ffi_parameter_types
            .iter()
            .cloned()
            .chain(return_ffi_parameter_types)
            .collect::<Vec<_>>();
        let ffi_parameters = ffi_parameter_types
            .iter()
            .enumerate()
            .map(|(index, parameter_type)| {
                let name = names::ClosureArgument::new(index).ffi();
                quote! { #name: #parameter_type }
            })
            .collect::<Vec<_>>();

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: u32 }],
            ffi_parameter_types: vec![quote! { u32 }],
            conversions: vec![quote! {
                unsafe extern "C" {
                    fn #call(handle: u32 #(, #ffi_parameters)*) #return_type;
                    fn #free(handle: u32);
                }
                #closure
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

struct ClosureInvoke<'expansion, 'lowered, 'rust, S: RenderSurface> {
    callable: &'lowered ImportedCallable<S>,
    source: &'lowered FnSig,
    closure_binding: &'rust ClosureBinding,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, 'rust, S: RenderSurface> ClosureInvoke<'expansion, 'lowered, 'rust, S> {
    fn new(
        callable: &'lowered ImportedCallable<S>,
        source: &'lowered FnSig,
        closure_binding: &'rust ClosureBinding,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        if callable.params().len() != source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter count does not match binding invoke parameter count",
            ));
        }
        Ok(Self {
            callable,
            source,
            closure_binding,
            expansion,
        })
    }

    fn parameters(&self) -> Result<InvokeParameters, Error> {
        let tokens = self
            .callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.closure_binding.parameters().iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                InvokeParameterInput::new(index, param.payload(), source, rust_type, self.expansion)
                    .tokens()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(InvokeParameters::from(tokens))
    }

    fn return_tokens(&self) -> Result<ForeignClosureReturnTokens, Error>
    where
        ForeignClosureReturnRenderer: Render<
                S,
                ForeignClosureReturn<'expansion, 'lowered, S>,
                Output = ForeignClosureReturnTokens,
            >,
    {
        <ForeignClosureReturnRenderer as Render<
            S,
            ForeignClosureReturn<'expansion, 'lowered, S>,
        >>::render(
            ForeignClosureReturnRenderer,
            ForeignClosureReturn::new(
                self.callable.returns().plan(),
                self.callable.error(),
                &self.source.returns,
                self.closure_binding.return_type(),
                self.expansion,
            ),
        )
    }
}

struct InvokeParameterInput<'expansion, 'lowered, 'rust, S: RenderSurface> {
    index: usize,
    payload: &'lowered OutgoingParam<S>,
    source: &'lowered TypeExpr,
    rust_type: &'rust Type,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, 'rust, S: RenderSurface>
    InvokeParameterInput<'expansion, 'lowered, 'rust, S>
{
    fn new(
        index: usize,
        payload: &'lowered OutgoingParam<S>,
        source: &'lowered TypeExpr,
        rust_type: &'rust Type,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            index,
            payload,
            source,
            rust_type,
            expansion,
        }
    }

    fn tokens(self) -> Result<InvokeParameterTokens, Error> {
        let argument = names::ClosureArgument::new(self.index).value();
        let rust_type = self.rust_type;
        match self.payload {
            OutgoingParam::Value(ParamPlan::Direct {
                ty: DirectValueType::Primitive(primitive),
                ..
            }) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(InvokeParameterTokens {
                    rust_parameter: quote! { #argument: #rust_type },
                    ffi_parameter_types: vec![ffi_type],
                    setup: Vec::new(),
                    call_arguments: vec![quote! { #argument }],
                })
            }
            OutgoingParam::Value(ParamPlan::Direct { .. }) => Ok(InvokeParameterTokens {
                rust_parameter: quote! { #argument: #rust_type },
                ffi_parameter_types: vec![quote! {
                    <#rust_type as ::boltffi::__private::Passable>::Out
                }],
                setup: Vec::new(),
                call_arguments: vec![quote! {
                    <#rust_type as ::boltffi::__private::Passable>::pack(#argument)
                }],
            }),
            OutgoingParam::Value(ParamPlan::Encoded { codec, .. }) => {
                let locals = names::ClosureArgument::new(self.index);
                let wire = locals.wire();
                let pointer = locals.pointer();
                let length = locals.length();
                let buffer = encoded::outgoing::Value::new(codec.root(), self.expansion)
                    .buffer(quote! { #argument })?;
                Ok(InvokeParameterTokens {
                    rust_parameter: quote! { #argument: #rust_type },
                    ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
                    setup: vec![quote! {
                        let #wire = #buffer;
                        let #pointer = #wire.as_ptr();
                        let #length = #wire.len();
                    }],
                    call_arguments: vec![quote! { #pointer }, quote! { #length }],
                })
            }
            OutgoingParam::Value(_) => Err(Error::UnsupportedExpansion(
                "closure invoke parameter shape",
            )),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedExpansion(
                "nested closure invoke parameter",
            )),
        }
    }
}

struct InvokeParameterTokens {
    rust_parameter: TokenStream,
    ffi_parameter_types: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    call_arguments: Vec<TokenStream>,
}

struct InvokeParameters {
    rust_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    call_arguments: Vec<TokenStream>,
}

impl From<Vec<InvokeParameterTokens>> for InvokeParameters {
    fn from(tokens: Vec<InvokeParameterTokens>) -> Self {
        InvokeParameters {
            rust_parameters: tokens
                .iter()
                .map(|token| token.rust_parameter.clone())
                .collect(),
            ffi_parameter_types: tokens
                .iter()
                .flat_map(|token| token.ffi_parameter_types.iter().cloned())
                .collect(),
            setup: tokens
                .iter()
                .flat_map(|token| token.setup.iter().cloned())
                .collect(),
            call_arguments: tokens
                .iter()
                .flat_map(|token| token.call_arguments.iter().cloned())
                .collect(),
        }
    }
}

struct ForeignClosureReturn<'expansion, 'lowered, S: RenderSurface> {
    plan: &'lowered ReturnPlan<S, IntoRust>,
    error: &'lowered ErrorDecl<S, IntoRust>,
    source: &'lowered ReturnDef,
    rust_type: Option<Type>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> ForeignClosureReturn<'expansion, 'lowered, S> {
    fn new(
        plan: &'lowered ReturnPlan<S, IntoRust>,
        error: &'lowered ErrorDecl<S, IntoRust>,
        source: &'lowered ReturnDef,
        rust_type: Option<&Type>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            plan,
            error,
            source,
            rust_type: rust_type.cloned(),
            expansion,
        }
    }

    fn direct_tokens(&self) -> Result<Option<ForeignClosureReturnTokens>, Error> {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return Ok(None);
        }

        match self.plan {
            ReturnPlan::Void => {
                if !matches!(self.source, ReturnDef::Void) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                Ok(Some(ForeignClosureReturnTokens::Void))
            }
            ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::Primitive(primitive),
            } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(Some(ForeignClosureReturnTokens::DirectPrimitive {
                    ffi_type,
                }))
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let rust_type = self.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke direct return requires source return type",
                ))?;
                Ok(Some(ForeignClosureReturnTokens::DirectPassable {
                    rust_type: rust_type.clone(),
                }))
            }
            ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. } => Ok(None),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }

    fn rust_fallible_return(&self) -> Result<RustFallibleReturn, Error> {
        let fallible = rust_api::Return::new(self.source).fallible()?;
        Ok(RustFallibleReturn {
            ok_type: fallible.ok_written_type()?,
            error_type: fallible.error_written_type()?,
            ok_source: fallible.ok().clone(),
            error_source: fallible.error().clone(),
        })
    }

    fn encoded_expression(
        &self,
        codec: &'lowered WritePlan,
        rust_type: &Type,
        source_type: &TypeExpr,
        bytes: TokenStream,
    ) -> Result<TokenStream, Error> {
        encoded::incoming::Value::new(codec.root(), self.expansion).expression(
            encoded::incoming::Bytes::new(
                rust_type,
                source_type,
                bytes,
                quote! { panic!("closure encoded return conversion failed: {:?}", error) },
            ),
        )
    }

    fn packed_expression(
        &self,
        codec: &'lowered WritePlan,
        rust_type: &Type,
        source_type: &TypeExpr,
        packed: TokenStream,
    ) -> Result<TokenStream, Error> {
        encoded::incoming::Value::new(codec.root(), self.expansion).packed_expression(
            rust_type,
            source_type,
            packed,
            quote! { panic!("closure encoded return conversion failed: {:?}", error) },
        )
    }
}

struct ForeignClosureReturnRenderer;

impl<'expansion, 'lowered> Render<Native, ForeignClosureReturn<'expansion, 'lowered, Native>>
    for ForeignClosureReturnRenderer
{
    type Output = ForeignClosureReturnTokens;

    fn render(
        self,
        input: ForeignClosureReturn<'expansion, 'lowered, Native>,
    ) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (ReturnPlan::ScalarOptionViaReturnSlot { primitive }, ErrorDecl::None(_)) => {
                rust_api::Return::new(input.source).scalar_option(*primitive)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "closure optional scalar return requires source return type",
                ))?;
                let value =
                    <wrapper::returns::scalar_option::Incoming as Render<Native, _>>::render(
                        wrapper::returns::scalar_option::Incoming,
                        wrapper::returns::scalar_option::IncomingInput::new(
                            *primitive,
                            rust_type.clone(),
                            quote! { __boltffi_result_buf },
                        ),
                    )?;
                Ok(ForeignClosureReturnTokens::NativeScalarOption { value })
            }
            (ReturnPlan::DirectVecViaReturnSlot { .. }, ErrorDecl::None(_)) => {
                let element = rust_api::Return::new(input.source).direct_vec_element_type()?;
                let value = <wrapper::returns::direct_vec::Incoming as Render<Native, _>>::render(
                    wrapper::returns::direct_vec::Incoming,
                    wrapper::returns::direct_vec::IncomingInput::new(
                        element,
                        quote! { __boltffi_result_buf },
                    ),
                )?;
                Ok(ForeignClosureReturnTokens::NativeDirectVec { value })
            }
            (
                ReturnPlan::DirectViaOutPointer {
                    ty: DirectValueType::Primitive(primitive),
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                let result = input.rust_fallible_return()?;
                let error = input.encoded_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPrimitive { ffi_type, error })
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let error = input.encoded_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPassable {
                    ok_type: result.ok_type,
                    error,
                })
            }
            (
                ReturnPlan::EncodedViaOutPointer {
                    codec: ok_codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec: error_codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let ok = input.encoded_expression(
                    ok_codec,
                    &result.ok_type,
                    &result.ok_source,
                    quote! { __boltffi_success_bytes },
                )?;
                let error = input.encoded_expression(
                    error_codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleEncoded {
                    ok_type: result.ok_type,
                    ok,
                    error,
                })
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let error = input.encoded_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleVoid { error })
            }
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke encoded return requires source return type",
                ))?;
                let ReturnDef::Value(source_type) = input.source else {
                    return Err(Error::SourceSyntaxMismatch(
                        "closure encoded return requires source return type",
                    ));
                };
                let value = input.encoded_expression(
                    codec,
                    rust_type,
                    source_type,
                    quote! { __boltffi_result_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeEncoded { value })
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "native closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }
}

impl<'expansion, 'lowered> Render<Wasm32, ForeignClosureReturn<'expansion, 'lowered, Wasm32>>
    for ForeignClosureReturnRenderer
{
    type Output = ForeignClosureReturnTokens;

    fn render(
        self,
        input: ForeignClosureReturn<'expansion, 'lowered, Wasm32>,
    ) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (ReturnPlan::ScalarOptionViaReturnSlot { primitive }, ErrorDecl::None(_)) => {
                rust_api::Return::new(input.source).scalar_option(*primitive)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "closure optional scalar return requires source return type",
                ))?;
                let value =
                    <wrapper::returns::scalar_option::Incoming as Render<Wasm32, _>>::render(
                        wrapper::returns::scalar_option::Incoming,
                        wrapper::returns::scalar_option::IncomingInput::new(
                            *primitive,
                            rust_type.clone(),
                            quote! { __boltffi_result_value },
                        ),
                    )?;
                Ok(ForeignClosureReturnTokens::WasmScalarOption { value })
            }
            (ReturnPlan::DirectVecViaReturnSlot { .. }, ErrorDecl::None(_)) => {
                let element = rust_api::Return::new(input.source).direct_vec_element_type()?;
                let value = <wrapper::returns::direct_vec::Incoming as Render<Wasm32, _>>::render(
                    wrapper::returns::direct_vec::Incoming,
                    wrapper::returns::direct_vec::IncomingInput::new(
                        element,
                        quote! { __boltffi_result_value },
                    ),
                )?;
                Ok(ForeignClosureReturnTokens::WasmDirectVec { value })
            }
            (
                ReturnPlan::DirectViaOutPointer {
                    ty: DirectValueType::Primitive(primitive),
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                let result = input.rust_fallible_return()?;
                let error = input.packed_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_packed },
                )?;
                Ok(ForeignClosureReturnTokens::WasmFallibleDirectPrimitive { ffi_type, error })
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let error = input.packed_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_packed },
                )?;
                Ok(ForeignClosureReturnTokens::WasmFallibleDirectPassable {
                    ok_type: result.ok_type,
                    error,
                })
            }
            (
                ReturnPlan::EncodedViaOutPointer {
                    codec: ok_codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec: error_codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let ok = input.packed_expression(
                    ok_codec,
                    &result.ok_type,
                    &result.ok_source,
                    quote! { __boltffi_success.assume_init() },
                )?;
                let error = input.packed_expression(
                    error_codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_packed },
                )?;
                Ok(ForeignClosureReturnTokens::WasmFallibleEncoded { ok, error })
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let error = input.packed_expression(
                    codec,
                    &result.error_type,
                    &result.error_source,
                    quote! { __boltffi_error_packed },
                )?;
                Ok(ForeignClosureReturnTokens::WasmFallibleVoid { error })
            }
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke encoded return requires source return type",
                ))?;
                let ReturnDef::Value(source_type) = input.source else {
                    return Err(Error::SourceSyntaxMismatch(
                        "closure encoded return requires source return type",
                    ));
                };
                let value = input.packed_expression(
                    codec,
                    rust_type,
                    source_type,
                    quote! { __boltffi_result_packed },
                )?;
                Ok(ForeignClosureReturnTokens::WasmEncoded { value })
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "wasm closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }
}

enum ForeignClosureReturnTokens {
    Void,
    DirectPrimitive {
        ffi_type: TokenStream,
    },
    DirectPassable {
        rust_type: Type,
    },
    NativeEncoded {
        value: TokenStream,
    },
    WasmEncoded {
        value: TokenStream,
    },
    NativeScalarOption {
        value: TokenStream,
    },
    WasmScalarOption {
        value: TokenStream,
    },
    NativeDirectVec {
        value: TokenStream,
    },
    WasmDirectVec {
        value: TokenStream,
    },
    NativeFallibleVoid {
        error: TokenStream,
    },
    NativeFallibleDirectPrimitive {
        ffi_type: TokenStream,
        error: TokenStream,
    },
    NativeFallibleDirectPassable {
        ok_type: Type,
        error: TokenStream,
    },
    NativeFallibleEncoded {
        ok_type: Type,
        ok: TokenStream,
        error: TokenStream,
    },
    WasmFallibleVoid {
        error: TokenStream,
    },
    WasmFallibleDirectPrimitive {
        ffi_type: TokenStream,
        error: TokenStream,
    },
    WasmFallibleDirectPassable {
        ok_type: Type,
        error: TokenStream,
    },
    WasmFallibleEncoded {
        ok: TokenStream,
        error: TokenStream,
    },
}

impl ForeignClosureReturnTokens {
    fn ffi_return_type(&self) -> TokenStream {
        match self {
            Self::Void => TokenStream::new(),
            Self::DirectPrimitive { ffi_type } => quote! { -> #ffi_type },
            Self::DirectPassable { rust_type } => {
                quote! { -> <#rust_type as ::boltffi::__private::Passable>::In }
            }
            Self::NativeEncoded { .. } => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmEncoded { .. } => quote! { -> u64 },
            Self::NativeScalarOption { .. } | Self::NativeDirectVec { .. } => {
                quote! { -> ::boltffi::__private::FfiBuf }
            }
            Self::WasmScalarOption { .. } => quote! { -> f64 },
            Self::WasmDirectVec { .. } => TokenStream::new(),
            Self::NativeFallibleVoid { .. }
            | Self::NativeFallibleDirectPrimitive { .. }
            | Self::NativeFallibleDirectPassable { .. }
            | Self::NativeFallibleEncoded { .. } => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmFallibleVoid { .. }
            | Self::WasmFallibleDirectPrimitive { .. }
            | Self::WasmFallibleDirectPassable { .. }
            | Self::WasmFallibleEncoded { .. } => quote! { -> u64 },
        }
    }

    fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { ffi_type, .. }
            | Self::WasmFallibleDirectPrimitive { ffi_type, .. } => {
                vec![quote! { *mut #ffi_type }]
            }
            Self::NativeFallibleDirectPassable { ok_type, .. }
            | Self::WasmFallibleDirectPassable { ok_type, .. } => {
                vec![quote! { *mut <#ok_type as ::boltffi::__private::Passable>::In }]
            }
            Self::NativeFallibleEncoded { .. } => {
                vec![quote! { *mut ::boltffi::__private::FfiBuf }]
            }
            Self::WasmFallibleEncoded { .. } => vec![quote! { *mut u64 }],
            _ => Vec::new(),
        }
    }

    fn setup(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { ffi_type, .. }
            | Self::WasmFallibleDirectPrimitive { ffi_type, .. } => vec![quote! {
                let mut __boltffi_success = ::core::mem::MaybeUninit::<#ffi_type>::uninit();
            }],
            Self::NativeFallibleDirectPassable { ok_type, .. }
            | Self::WasmFallibleDirectPassable { ok_type, .. } => vec![quote! {
                let mut __boltffi_success =
                    ::core::mem::MaybeUninit::<<#ok_type as ::boltffi::__private::Passable>::In>::uninit();
            }],
            Self::NativeFallibleEncoded { .. } => vec![quote! {
                let mut __boltffi_success =
                    ::core::mem::MaybeUninit::<::boltffi::__private::FfiBuf>::uninit();
            }],
            Self::WasmFallibleEncoded { .. } => vec![quote! {
                let mut __boltffi_success = ::core::mem::MaybeUninit::<u64>::uninit();
            }],
            _ => Vec::new(),
        }
    }

    fn call_arguments(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { .. }
            | Self::NativeFallibleDirectPassable { .. }
            | Self::NativeFallibleEncoded { .. }
            | Self::WasmFallibleDirectPrimitive { .. }
            | Self::WasmFallibleDirectPassable { .. }
            | Self::WasmFallibleEncoded { .. } => {
                vec![quote! { __boltffi_success.as_mut_ptr() }]
            }
            _ => Vec::new(),
        }
    }

    fn body(&self, call: TokenStream) -> TokenStream {
        match self {
            Self::Void => quote! {
                unsafe {
                    #call;
                }
            },
            Self::DirectPrimitive { .. } => quote! { unsafe { #call } },
            Self::DirectPassable { rust_type } => quote! {
                unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#call)
                }
            },
            Self::NativeEncoded { value } => quote! {
                {
                    let __boltffi_result_buf = unsafe { #call };
                    let __boltffi_result_bytes = unsafe {
                        __boltffi_result_buf.as_byte_slice()
                    };
                    #value
                }
            },
            Self::WasmEncoded { value } => quote! {
                {
                    let __boltffi_result_packed = unsafe { #call };
                    #value
                }
            },
            Self::NativeScalarOption { value } | Self::NativeDirectVec { value } => quote! {
                {
                    let __boltffi_result_buf = unsafe { #call };
                    #value
                }
            },
            Self::WasmScalarOption { value } | Self::WasmDirectVec { value } => quote! {
                {
                    let __boltffi_result_value = unsafe { #call };
                    #value
                }
            },
            Self::NativeFallibleVoid { error } => quote! {
                {
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(())
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            },
            Self::NativeFallibleDirectPrimitive { error, .. } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(unsafe { __boltffi_success.assume_init() })
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::NativeFallibleDirectPassable { ok_type, error } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(unsafe {
                            <#ok_type as ::boltffi::__private::Passable>::unpack(
                                __boltffi_success.assume_init()
                            )
                        })
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::NativeFallibleEncoded {
                ok_type: _,
                ok,
                error,
            } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        let __boltffi_success_buf = unsafe {
                            __boltffi_success.assume_init()
                        };
                        let __boltffi_success_bytes = unsafe {
                            __boltffi_success_buf.as_byte_slice()
                        };
                        Ok(#ok)
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::WasmFallibleVoid { error } => quote! {
                {
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(())
                    } else {
                        Err(#error)
                    }
                }
            },
            Self::WasmFallibleDirectPrimitive { error, .. } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(unsafe { __boltffi_success.assume_init() })
                    } else {
                        Err(#error)
                    }
                }
            }
            Self::WasmFallibleDirectPassable { ok_type, error } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(unsafe {
                            <#ok_type as ::boltffi::__private::Passable>::unpack(
                                __boltffi_success.assume_init()
                            )
                        })
                    } else {
                        Err(#error)
                    }
                }
            }
            Self::WasmFallibleEncoded { ok, error } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(#ok)
                    } else {
                        Err(#error)
                    }
                }
            }
        }
    }
}

struct RustFallibleReturn {
    ok_type: Type,
    error_type: Type,
    ok_source: TypeExpr,
    error_source: TypeExpr,
}

enum ClosureBinding {
    ImplTrait(ClosureSignature),
    Boxed(ClosureSignature, Type),
    NullableBoxed(ClosureSignature, Type),
}

impl ClosureBinding {
    fn new(
        source: &rust_api::Closure,
        closure_form: ClosureForm,
        closure_presence: HandlePresence,
    ) -> Result<Self, Error> {
        if source.function() != closure_form {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter form does not match binding closure",
            ));
        }
        let signature = ClosureSignature::from_source(source.signature(), closure_form)?;
        let closure_binding = match (closure_presence, source.form()) {
            (HandlePresence::Required, rust_api::ClosureSourceForm::BoxedDyn) => {
                Ok(Self::Boxed(signature, source.ty()?))
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::ImplTrait) => {
                Ok(Self::ImplTrait(signature))
            }
            (HandlePresence::Nullable, rust_api::ClosureSourceForm::NullableBoxedDyn) => {
                Ok(Self::NullableBoxed(signature, source.ty()?))
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::FunctionPointer) => Err(
                Error::UnsupportedExpansion("function-pointer closure parameter"),
            ),
            _ => Err(Error::SourceSyntaxMismatch(
                "source closure parameter form does not match binding closure",
            )),
        }?;
        Ok(closure_binding)
    }

    fn parameters(&self) -> &[Type] {
        match self {
            Self::ImplTrait(signature)
            | Self::Boxed(signature, _)
            | Self::NullableBoxed(signature, _) => &signature.parameters,
        }
    }

    fn return_type(&self) -> Option<&Type> {
        match self {
            Self::ImplTrait(signature)
            | Self::Boxed(signature, _)
            | Self::NullableBoxed(signature, _) => signature.return_type.as_ref(),
        }
    }

    fn native_binding(&self, input: NativeBinding) -> Result<TokenStream, Error> {
        let NativeBinding {
            ident,
            callback,
            context,
            release,
            owner,
            rust_parameters,
            body,
            failure,
        } = input;
        match self {
            Self::ImplTrait(_) => Ok(quote! {
                let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                let #ident = move |#(#rust_parameters),*| {
                    #body
                };
            }),
            Self::Boxed(_, ty) => Ok(quote! {
                let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                let #ident: #ty = Box::new(move |#(#rust_parameters),*| {
                    #body
                });
            }),
            Self::NullableBoxed(_, ty) => Ok(quote! {
                let #ident: #ty = match (#callback, #release) {
                    (Some(#callback), Some(#release)) => {
                        let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                        Some(Box::new(move |#(#rust_parameters),*| {
                            #body
                        }) as _)
                    }
                    (None, None) => None,
                    _ => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: invalid nullable closure registration",
                            stringify!(#ident)
                        ));
                        #failure
                    }
                };
            }),
        }
    }

    fn wasm_binding(
        &self,
        ident: &Ident,
        owner: &Ident,
        free: &Ident,
        rust_parameters: &[TokenStream],
        body: TokenStream,
        failure: &TokenStream,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::ImplTrait(_) => Ok(quote! {
                if #ident == 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null closure handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                let #ident = move |#(#rust_parameters),*| {
                    #body
                };
            }),
            Self::Boxed(_, ty) => Ok(quote! {
                if #ident == 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null closure handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                let #ident: #ty = Box::new(move |#(#rust_parameters),*| {
                    #body
                });
            }),
            Self::NullableBoxed(_, ty) => Ok(quote! {
                let #ident: #ty = if #ident == 0 {
                    None
                } else {
                    let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                    Some(Box::new(move |#(#rust_parameters),*| {
                        #body
                    }) as _)
                };
            }),
        }
    }
}

struct NativeBinding {
    ident: Ident,
    callback: Ident,
    context: Ident,
    release: Ident,
    owner: Ident,
    rust_parameters: Vec<TokenStream>,
    body: TokenStream,
    failure: TokenStream,
}

struct ClosureSignature {
    form: ClosureForm,
    parameters: Vec<Type>,
    return_type: Option<Type>,
}

impl ClosureSignature {
    fn from_source(source: &FnSig, form: ClosureForm) -> Result<Self, Error> {
        let parameters = source
            .parameters
            .iter()
            .map(|type_expr| {
                rust_api::TypeTokens::new(type_expr).map(rust_api::TypeTokens::into_type)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = match &source.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(type_expr) => Some(rust_api::TypeTokens::new(type_expr)?.into_type()),
        };
        Ok(Self {
            form,
            parameters,
            return_type,
        })
    }
}
