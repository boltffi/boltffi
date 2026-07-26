use boltffi_ast::{FnSig, ReturnDef, TypeExpr};
use boltffi_binding::{
    DirectValueType, ErrorDecl, ExportedCallable, IncomingParam, Native, OutOfRust, ParamPlan,
    ReadPlan, Receive, ReturnPlan, TypeRef, Wasm32, WritePlan, native, wasm32,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, names, returns},
};

pub struct Signature {
    parameters: Vec<Type>,
    return_type: Option<Type>,
}

impl Signature {
    pub fn from_source(source: &FnSig) -> Result<Self, Error> {
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
            parameters,
            return_type,
        })
    }

    pub fn return_tokens(&self) -> TokenStream {
        match &self.return_type {
            Some(ty) => quote! { -> #ty },
            None => TokenStream::new(),
        }
    }

    pub fn parameters(&self) -> &[Type] {
        &self.parameters
    }
}

pub struct Invoke<'expansion, 'lowered, S: RenderSurface> {
    callable: &'lowered ExportedCallable<S>,
    source: &'lowered FnSig,
    signature: &'lowered Signature,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Invoke<'expansion, 'lowered, S> {
    pub fn new(
        callable: &'lowered ExportedCallable<S>,
        source: &'lowered FnSig,
        signature: &'lowered Signature,
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
            signature,
            expansion,
        })
    }

    fn render_parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error>
    where
        ParameterRenderer: Render<S, Parameter<'expansion, 'lowered, S>, Output = ParameterTokens>,
    {
        self.callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.signature.parameters.iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                <ParameterRenderer as Render<S, _>>::render(
                    ParameterRenderer,
                    Parameter {
                        index,
                        payload: param.payload(),
                        source,
                        rust_type,
                        failure: failure.clone(),
                        expansion: self.expansion,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(InvokeParameters::from)
    }

    fn render_return(&self) -> Result<InvokeReturn, Error>
    where
        ReturnRenderer: Render<S, Return<'expansion, 'lowered, S>, Output = InvokeReturn>,
    {
        <ReturnRenderer as Render<S, _>>::render(
            ReturnRenderer,
            Return::new(
                self.callable.returns().plan(),
                self.callable.error(),
                &self.source.returns,
                self.signature.return_type.as_ref(),
                self.expansion,
            ),
        )
    }
}

impl<'expansion, 'lowered> Invoke<'expansion, 'lowered, Native> {
    pub fn parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error> {
        self.render_parameters(failure)
    }

    pub fn return_tokens(&self) -> Result<InvokeReturn, Error> {
        self.render_return()
    }
}

impl<'expansion, 'lowered> Invoke<'expansion, 'lowered, Wasm32> {
    pub fn parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error> {
        self.render_parameters(failure)
    }

    pub fn return_tokens(&self) -> Result<InvokeReturn, Error> {
        self.render_return()
    }
}

struct ParameterRenderer;

struct Parameter<'expansion, 'lowered, S: RenderSurface> {
    index: usize,
    payload: &'lowered IncomingParam<S>,
    source: &'lowered TypeExpr,
    rust_type: &'lowered Type,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Parameter<'expansion, 'lowered, S> {
    fn direct_tokens(&self) -> Result<Option<ParameterTokens>, Error>
    where
        wrapper::param::direct::Renderer:
            Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
        wrapper::param::closure::Renderer: Render<
                S,
                wrapper::param::closure::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        let argument = names::ClosureArgument::new(self.index).value();
        match self.payload {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                let tokens = <wrapper::param::direct::Renderer as Render<S, _>>::render(
                    wrapper::param::direct::Renderer,
                    wrapper::param::direct::Input::new(
                        ty,
                        *receive,
                        self.rust_type.clone(),
                        argument,
                        self.failure.clone(),
                    ),
                )?;
                if !tokens.writebacks().is_empty() {
                    return Err(Error::UnsupportedExpansion(
                        "mutable rust closure invoke direct parameter",
                    ));
                }
                let conversions = tokens.conversions();
                Ok(Some(ParameterTokens {
                    items: tokens.items().to_vec(),
                    ffi_parameters: tokens.ffi_parameters().to_vec(),
                    ffi_parameter_types: tokens.ffi_parameter_types().to_vec(),
                    conversion: quote! { #(#conversions)* },
                    argument: tokens.argument().clone(),
                }))
            }
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Ok(None),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion(
                "rust closure invoke parameter shape",
            )),
            IncomingParam::Closure(closure) => {
                let source_closure = rust_api::Closure::new(self.source, closure.presence())?;
                let tokens = <wrapper::param::closure::Renderer as Render<S, _>>::render(
                    wrapper::param::closure::Renderer,
                    wrapper::param::closure::Input::new(
                        closure,
                        source_closure,
                        argument.clone(),
                        self.failure.clone(),
                        self.expansion,
                    ),
                )?;
                let conversions = tokens.conversions();
                Ok(Some(ParameterTokens {
                    items: tokens.items().to_vec(),
                    ffi_parameters: tokens.ffi_parameters().to_vec(),
                    ffi_parameter_types: tokens.ffi_parameter_types().to_vec(),
                    conversion: quote! { #(#conversions)* },
                    argument: tokens.argument().clone(),
                }))
            }
        }
    }

    fn encoded_tokens(
        &self,
        codec: &WritePlan,
        receive: Receive,
    ) -> Result<ParameterTokens, Error> {
        let locals = names::ClosureArgument::new(self.index);
        let argument = locals.value();
        let pointer = locals.pointer();
        let length = locals.length();
        let target = rust_api::DecodeTarget::received(receive, self.source)?;
        let conversion = wrapper::encoded::incoming::Value::new(codec.root(), self.expansion)
            .decode(wrapper::encoded::incoming::Input::new(
                &target,
                &argument,
                &pointer,
                &length,
                &self.failure,
            ))?;

        Ok(ParameterTokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversion,
            argument: quote! { #argument },
        })
    }
}

impl<'expansion, 'lowered> Render<Native, Parameter<'expansion, 'lowered, Native>>
    for ParameterRenderer
{
    type Output = ParameterTokens;

    fn render(self, input: Parameter<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match input.payload {
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: native::BufferShape::Slice,
                ..
            }) => input.encoded_tokens(codec, *receive),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedExpansion(
                "native rust closure invoke encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke parameter shape",
            )),
        }
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Parameter<'expansion, 'lowered, Wasm32>>
    for ParameterRenderer
{
    type Output = ParameterTokens;

    fn render(self, input: Parameter<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match input.payload {
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: wasm32::BufferShape::Slice,
                ..
            }) => input.encoded_tokens(codec, *receive),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedExpansion(
                "wasm rust closure invoke encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke parameter shape",
            )),
        }
    }
}

struct ParameterTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversion: TokenStream,
    argument: TokenStream,
}

impl ParameterTokens {
    fn single(
        ffi_parameter: TokenStream,
        ffi_parameter_type: TokenStream,
        conversion: TokenStream,
        argument: TokenStream,
    ) -> Self {
        Self {
            items: Vec::new(),
            ffi_parameters: vec![ffi_parameter],
            ffi_parameter_types: vec![ffi_parameter_type],
            conversion,
            argument,
        }
    }
}

pub struct InvokeParameters {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl InvokeParameters {
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn ffi_parameter_types(&self) -> &[TokenStream] {
        &self.ffi_parameter_types
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn arguments(&self) -> &[TokenStream] {
        &self.arguments
    }
}

impl From<Vec<ParameterTokens>> for InvokeParameters {
    fn from(tokens: Vec<ParameterTokens>) -> Self {
        Self {
            items: tokens
                .iter()
                .flat_map(|token| token.items.iter().cloned())
                .collect(),
            ffi_parameters: tokens
                .iter()
                .flat_map(|token| token.ffi_parameters.iter().cloned())
                .collect(),
            ffi_parameter_types: tokens
                .iter()
                .flat_map(|token| token.ffi_parameter_types.iter().cloned())
                .collect(),
            conversions: tokens
                .iter()
                .map(|token| token.conversion.clone())
                .collect(),
            arguments: tokens.iter().map(|token| token.argument.clone()).collect(),
        }
    }
}

struct ReturnRenderer;

struct Return<'expansion, 'lowered, S: RenderSurface> {
    plan: &'lowered ReturnPlan<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: &'lowered ReturnDef,
    rust_type: Option<&'lowered Type>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Return<'expansion, 'lowered, S> {
    fn new(
        plan: &'lowered ReturnPlan<S, OutOfRust>,
        error: &'lowered ErrorDecl<S, OutOfRust>,
        source: &'lowered ReturnDef,
        rust_type: Option<&'lowered Type>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            plan,
            error,
            source,
            rust_type,
            expansion,
        }
    }

    fn direct_tokens<T: RenderSurface>(&self) -> Result<Option<InvokeReturn>, Error>
    where
        for<'ty> wrapper::type_ref::Renderer: Render<T, &'ty TypeRef, Output = TokenStream>,
    {
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
                Ok(Some(InvokeReturn::void()))
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
                Ok(Some(InvokeReturn::direct_primitive(ffi_type)))
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let rust_type = self.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke direct return requires source return type",
                ))?;
                Ok(Some(InvokeReturn::direct_passable(Box::new(
                    rust_type.clone(),
                ))))
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let rust_type = self.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke direct return requires source return type",
                ))?;
                Ok(Some(InvokeReturn::direct_passable_out(Box::new(
                    rust_type.clone(),
                ))))
            }
            ReturnPlan::EncodedViaReturnSlot { .. } => Ok(None),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke return shape",
            )),
        }
    }

    fn rust_fallible_return(&self) -> Result<RustFallibleReturn, Error> {
        let ok = self.source_fallible()?.ok_written_type()?;
        Ok(RustFallibleReturn { ok })
    }

    fn source_fallible(&self) -> Result<rust_api::Fallible<'lowered>, Error> {
        rust_api::Return::new(self.source).fallible()
    }

    fn encoded_error(
        &self,
        error_codec: &'lowered ReadPlan,
        error_shape: S::BufferShape,
    ) -> Result<EncodedError, Error>
    where
        returns::encoded::Renderer: Render<
                S,
                returns::encoded::Input<'expansion, 'lowered, 'lowered, S>,
                Output = returns::encoded::Tokens,
            > + Render<S, returns::encoded::Empty<S>, Output = returns::encoded::Tokens>,
    {
        let error_ident = names::Locals::new(Span::call_site()).error();
        let error = <returns::encoded::Renderer as Render<S, _>>::render(
            returns::encoded::Renderer,
            returns::encoded::Input::new(error_codec, error_shape, error_ident, self.expansion),
        )?;
        let empty = <returns::encoded::Renderer as Render<S, _>>::render(
            returns::encoded::Renderer,
            returns::encoded::Empty::new(error_shape),
        )?;

        Ok(EncodedError {
            return_type: error.return_type().clone(),
            value: error.value().clone(),
            empty_value: empty.value().clone(),
        })
    }
}

impl<'expansion, 'lowered> Render<Native, Return<'expansion, 'lowered, Native>> for ReturnRenderer {
    type Output = InvokeReturn;

    fn render(self, input: Return<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens::<Native>()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let value = wrapper::encoded::outgoing::Value::new(codec.root(), input.expansion)
                    .buffer(quote! { __boltffi_result })?;
                Ok(InvokeReturn::native_encoded(value))
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => Ok(InvokeReturn::fallible(
                input.encoded_error(codec, native::BufferShape::Buffer)?,
                FallibleSuccess::Void,
            )),
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
                Ok(InvokeReturn::fallible(
                    input.encoded_error(codec, native::BufferShape::Buffer)?,
                    FallibleSuccess::DirectPrimitive { ffi_type },
                ))
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => Ok(InvokeReturn::fallible(
                input.encoded_error(codec, native::BufferShape::Buffer)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(input.rust_fallible_return()?.ok),
                },
            )),
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
                let success_ident = names::Locals::new(Span::call_site()).success();
                let success = <returns::encoded::Renderer as Render<Native, _>>::render(
                    returns::encoded::Renderer,
                    returns::encoded::Input::new(
                        ok_codec,
                        native::BufferShape::Buffer,
                        success_ident,
                        input.expansion,
                    ),
                )?;
                Ok(InvokeReturn::fallible(
                    input.encoded_error(error_codec, native::BufferShape::Buffer)?,
                    FallibleSuccess::Encoded {
                        out_type: success.return_type_without_arrow(),
                        value: success.value().clone(),
                    },
                ))
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "native rust closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke return shape",
            )),
        }
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Return<'expansion, 'lowered, Wasm32>> for ReturnRenderer {
    type Output = InvokeReturn;

    fn render(self, input: Return<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens::<Wasm32>()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let value = wrapper::encoded::outgoing::Value::new(codec.root(), input.expansion)
                    .buffer(quote! { __boltffi_result })?;
                Ok(InvokeReturn::wasm_encoded(value))
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(InvokeReturn::fallible(
                input.encoded_error(codec, wasm32::BufferShape::Packed)?,
                FallibleSuccess::Void,
            )),
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
                Ok(InvokeReturn::fallible(
                    input.encoded_error(codec, wasm32::BufferShape::Packed)?,
                    FallibleSuccess::DirectPrimitive { ffi_type },
                ))
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(InvokeReturn::fallible(
                input.encoded_error(codec, wasm32::BufferShape::Packed)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(input.rust_fallible_return()?.ok),
                },
            )),
            (
                ReturnPlan::EncodedViaOutPointer {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec: error_codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let success_ident = names::Locals::new(Span::call_site()).success();
                let success = <returns::encoded::Renderer as Render<Wasm32, _>>::render(
                    returns::encoded::Renderer,
                    returns::encoded::Input::new(
                        codec,
                        wasm32::BufferShape::Packed,
                        success_ident,
                        input.expansion,
                    ),
                )?;
                Ok(InvokeReturn::fallible(
                    input.encoded_error(error_codec, wasm32::BufferShape::Packed)?,
                    FallibleSuccess::Encoded {
                        out_type: success.return_type_without_arrow(),
                        value: success.value().clone(),
                    },
                ))
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "wasm rust closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke return shape",
            )),
        }
    }
}

pub struct InvokeReturn {
    kind: InvokeReturnKind,
}

enum InvokeReturnKind {
    Void,
    DirectPrimitive { ffi_type: TokenStream },
    DirectPassable { rust_type: Box<Type> },
    DirectPassableOut { rust_type: Box<Type> },
    NativeEncoded { value: TokenStream },
    WasmEncoded { value: TokenStream },
    Fallible(Box<FallibleClosureReturn>),
}

impl InvokeReturn {
    fn void() -> Self {
        Self {
            kind: InvokeReturnKind::Void,
        }
    }

    fn direct_primitive(ffi_type: TokenStream) -> Self {
        Self {
            kind: InvokeReturnKind::DirectPrimitive { ffi_type },
        }
    }

    fn direct_passable(rust_type: Box<Type>) -> Self {
        Self {
            kind: InvokeReturnKind::DirectPassable { rust_type },
        }
    }

    fn direct_passable_out(rust_type: Box<Type>) -> Self {
        Self {
            kind: InvokeReturnKind::DirectPassableOut { rust_type },
        }
    }

    fn native_encoded(value: TokenStream) -> Self {
        Self {
            kind: InvokeReturnKind::NativeEncoded { value },
        }
    }

    fn wasm_encoded(value: TokenStream) -> Self {
        Self {
            kind: InvokeReturnKind::WasmEncoded { value },
        }
    }

    fn fallible(error: EncodedError, success: FallibleSuccess) -> Self {
        Self {
            kind: InvokeReturnKind::Fallible(Box::new(FallibleClosureReturn { error, success })),
        }
    }

    pub fn return_type(&self) -> TokenStream {
        match &self.kind {
            InvokeReturnKind::Void => TokenStream::new(),
            InvokeReturnKind::DirectPrimitive { ffi_type } => quote! { -> #ffi_type },
            InvokeReturnKind::DirectPassable { rust_type } => {
                quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out }
            }
            InvokeReturnKind::DirectPassableOut { .. } => TokenStream::new(),
            InvokeReturnKind::NativeEncoded { .. } => quote! { -> ::boltffi::__private::FfiBuf },
            InvokeReturnKind::WasmEncoded { .. } => quote! { -> u64 },
            InvokeReturnKind::Fallible(fallible) => fallible.error.return_type.clone(),
        }
    }

    pub fn ffi_parameters(&self) -> Vec<TokenStream> {
        match &self.kind {
            InvokeReturnKind::DirectPassableOut { rust_type } => {
                let output = names::Locals::new(Span::call_site()).return_out();
                vec![quote! {
                    #output: *mut <#rust_type as ::boltffi::__private::Passable>::Out
                }]
            }
            InvokeReturnKind::Fallible(fallible) => fallible.success.ffi_parameters(),
            _ => Vec::new(),
        }
    }

    pub fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match &self.kind {
            InvokeReturnKind::DirectPassableOut { rust_type } => vec![quote! {
                *mut <#rust_type as ::boltffi::__private::Passable>::Out
            }],
            InvokeReturnKind::Fallible(fallible) => fallible.success.ffi_parameter_types(),
            _ => Vec::new(),
        }
    }

    pub fn body(&self, call: TokenStream) -> TokenStream {
        match &self.kind {
            InvokeReturnKind::Void => quote! {
                #call;
            },
            InvokeReturnKind::DirectPrimitive { .. } => quote! { #call },
            InvokeReturnKind::DirectPassable { rust_type } => quote! {
                <#rust_type as ::boltffi::__private::Passable>::pack(#call)
            },
            InvokeReturnKind::DirectPassableOut { rust_type } => {
                let output = names::Locals::new(Span::call_site()).return_out();
                quote! {
                    {
                        let __boltffi_result: #rust_type = #call;
                        if !#output.is_null() {
                            unsafe {
                                ::core::ptr::write(
                                    #output,
                                    <#rust_type as ::boltffi::__private::Passable>::pack(
                                        __boltffi_result
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            InvokeReturnKind::NativeEncoded { value } => quote! {
                {
                    let __boltffi_result = #call;
                    #value
                }
            },
            InvokeReturnKind::WasmEncoded { value } => quote! {
                {
                    let __boltffi_result = #call;
                    #value.into_packed()
                }
            },
            InvokeReturnKind::Fallible(fallible) => fallible.success.body(&fallible.error, call),
        }
    }

    pub fn failure(&self) -> TokenStream {
        match &self.kind {
            InvokeReturnKind::Void => quote! { return; },
            InvokeReturnKind::DirectPrimitive { ffi_type } => quote! {
                return <#ffi_type as ::core::default::Default>::default();
            },
            InvokeReturnKind::DirectPassable { rust_type } => quote! {
                return unsafe {
                    ::core::mem::MaybeUninit::<
                        <#rust_type as ::boltffi::__private::Passable>::Out
                    >::zeroed().assume_init()
                };
            },
            InvokeReturnKind::DirectPassableOut { rust_type } => {
                let output = names::Locals::new(Span::call_site()).return_out();
                quote! {
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
                }
            }
            InvokeReturnKind::NativeEncoded { .. } => quote! {
                return ::boltffi::__private::FfiBuf::default();
            },
            InvokeReturnKind::WasmEncoded { .. } => quote! {
                return ::boltffi::__private::FfiBuf::default().into_packed();
            },
            InvokeReturnKind::Fallible(fallible) => fallible.error.failure(),
        }
    }
}

struct FallibleClosureReturn {
    error: EncodedError,
    success: FallibleSuccess,
}

struct EncodedError {
    return_type: TokenStream,
    value: TokenStream,
    empty_value: TokenStream,
}

impl EncodedError {
    fn failure(&self) -> TokenStream {
        let value = &self.value;
        quote! {
            {
                let __boltffi_error = ::boltffi::__private::take_last_error()
                    .unwrap_or_else(|| "closure invoke argument conversion failed".to_string());
                return #value;
            }
        }
    }
}

enum FallibleSuccess {
    Void,
    DirectPrimitive {
        ffi_type: TokenStream,
    },
    DirectPassable {
        rust_type: Box<Type>,
    },
    Encoded {
        out_type: TokenStream,
        value: TokenStream,
    },
}

impl FallibleSuccess {
    fn ffi_parameters(&self) -> Vec<TokenStream> {
        let out = names::Locals::new(Span::call_site()).success_out();
        self.ffi_parameter_types()
            .into_iter()
            .map(|ty| quote! { #out: #ty })
            .collect()
    }

    fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match self {
            Self::Void => Vec::new(),
            Self::DirectPrimitive { ffi_type } => vec![quote! { *mut #ffi_type }],
            Self::DirectPassable { rust_type } => vec![quote! {
                *mut <#rust_type as ::boltffi::__private::Passable>::Out
            }],
            Self::Encoded { out_type, .. } => vec![quote! { *mut #out_type }],
        }
    }

    fn body(&self, error: &EncodedError, call: TokenStream) -> TokenStream {
        let locals = names::Locals::new(Span::call_site());
        let success_out = locals.success_out();
        let success_ident = locals.success();
        let empty_error = &error.empty_value;
        let error_value = &error.value;
        let pattern = self.pattern(&success_ident);
        let write_success = self.write_success(&success_ident, &success_out);
        quote! {
            match #call {
                Ok(#pattern) => {
                    #write_success
                    #empty_error
                }
                Err(__boltffi_error) => {
                    #error_value
                }
            }
        }
    }

    fn pattern(&self, success: &Ident) -> TokenStream {
        match self {
            Self::Void => quote! { () },
            _ => quote! { #success },
        }
    }

    fn write_success(&self, success: &Ident, out: &Ident) -> TokenStream {
        match self {
            Self::Void => TokenStream::new(),
            Self::DirectPrimitive { .. } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = #success;
                    }
                }
            },
            Self::DirectPassable { rust_type } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = <#rust_type as ::boltffi::__private::Passable>::pack(#success);
                    }
                }
            },
            Self::Encoded { value, .. } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = #value;
                    }
                }
            },
        }
    }
}

struct RustFallibleReturn {
    ok: Type,
}
