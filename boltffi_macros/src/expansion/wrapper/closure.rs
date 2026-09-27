use boltffi_ast::{FnSig, ReturnDef, TypeExpr};
use boltffi_binding::{
    DirectValueType, ErrorDecl, ExportedCallable, IncomingParam, Native, OutOfRust, ParamPlan,
    ReadPlan, Receive, ReturnPlan, Wasm32, WritePlan, native, wasm32,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{self, names, returns},
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

pub struct Invoke<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    callable: &'lowered ExportedCallable<S>,
    source: &'lowered FnSig,
    signature: &'lowered Signature,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Invoke<'expansion, 'lowered, S> {
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
}

impl<'expansion, 'lowered> Invoke<'expansion, 'lowered, Native> {
    pub fn parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error> {
        self.callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.signature.parameters.iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                Parameter {
                    index,
                    payload: param.payload(),
                    source,
                    rust_type,
                    failure: failure.clone(),
                    expansion: self.expansion,
                }
                .render()
            })
            .collect::<Result<Vec<_>, _>>()
            .map(InvokeParameters::from)
    }

    pub fn return_tokens(&self) -> Result<InvokeReturn, Error> {
        Return::new(
            self.callable.returns().plan(),
            self.callable.error(),
            &self.source.returns,
            self.signature.return_type.as_ref(),
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered> Invoke<'expansion, 'lowered, Wasm32> {
    pub fn parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error> {
        self.callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.signature.parameters.iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                Parameter {
                    index,
                    payload: param.payload(),
                    source,
                    rust_type,
                    failure: failure.clone(),
                    expansion: self.expansion,
                }
                .render()
            })
            .collect::<Result<Vec<_>, _>>()
            .map(InvokeParameters::from)
    }

    pub fn return_tokens(&self) -> Result<InvokeReturn, Error> {
        Return::new(
            self.callable.returns().plan(),
            self.callable.error(),
            &self.source.returns,
            self.signature.return_type.as_ref(),
            self.expansion,
        )
        .render()
    }
}

struct Parameter<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    index: usize,
    payload: &'lowered IncomingParam<S>,
    source: &'lowered TypeExpr,
    rust_type: &'lowered Type,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Parameter<'expansion, 'lowered, S> {
    fn finish(&self, tokens: wrapper::param::Tokens) -> Result<ParameterTokens, Error> {
        if !tokens.writebacks().is_empty() {
            return Err(Error::UnsupportedExpansion(
                "mutable rust closure invoke direct parameter",
            ));
        }
        let conversions = tokens.conversions();
        Ok(ParameterTokens {
            owned_values: tokens.owned_values().to_vec(),
            items: tokens.items().to_vec(),
            ffi_parameters: tokens.ffi_parameters().to_vec(),
            ffi_parameter_types: tokens.ffi_parameter_types().to_vec(),
            conversion: quote! { #(#conversions)* },
            argument: tokens.argument().clone(),
        })
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
            owned_values: Vec::new(),
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversion,
            argument: quote! { #argument },
        })
    }
}

impl<'expansion, 'lowered> Parameter<'expansion, 'lowered, Native> {
    fn render(self) -> Result<ParameterTokens, Error> {
        let argument = names::ClosureArgument::new(self.index).value();
        match self.payload {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                let tokens = wrapper::param::direct::Input::new(
                    ty,
                    *receive,
                    self.rust_type.clone(),
                    argument,
                    self.failure.clone(),
                )
                .native()?;
                self.finish(tokens)
            }
            IncomingParam::Closure(closure) => {
                let source_closure = rust_api::Closure::new(self.source, closure.presence())?;
                let tokens = wrapper::param::closure::Input::new(
                    closure,
                    source_closure,
                    argument,
                    self.failure.clone(),
                    self.expansion,
                )
                .render()?;
                self.finish(tokens)
            }
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: native::BufferShape::Slice,
                ..
            }) => self.encoded_tokens(codec, *receive),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedExpansion(
                "native rust closure invoke encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "rust closure invoke parameter shape",
            )),
        }
    }
}

impl<'expansion, 'lowered> Parameter<'expansion, 'lowered, Wasm32> {
    fn render(self) -> Result<ParameterTokens, Error> {
        let argument = names::ClosureArgument::new(self.index).value();
        match self.payload {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                let tokens = wrapper::param::direct::Input::new(
                    ty,
                    *receive,
                    self.rust_type.clone(),
                    argument,
                    self.failure.clone(),
                )
                .wasm32()?;
                self.finish(tokens)
            }
            IncomingParam::Closure(closure) => {
                let source_closure = rust_api::Closure::new(self.source, closure.presence())?;
                let tokens = wrapper::param::closure::Input::new(
                    closure,
                    source_closure,
                    argument,
                    self.failure.clone(),
                    self.expansion,
                )
                .render()?;
                self.finish(tokens)
            }
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: wasm32::BufferShape::Slice,
                ..
            }) => self.encoded_tokens(codec, *receive),
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
    owned_values: Vec<TokenStream>,
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversion: TokenStream,
    argument: TokenStream,
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
                .flat_map(|token| token.owned_values.iter().cloned())
                .chain(tokens.iter().map(|token| token.conversion.clone()))
                .collect(),
            arguments: tokens.iter().map(|token| token.argument.clone()).collect(),
        }
    }
}

struct Return<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
    plan: &'lowered ReturnPlan<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: &'lowered ReturnDef,
    rust_type: Option<&'lowered Type>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Return<'expansion, 'lowered, S> {
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

    fn direct_tokens(&self) -> Result<Option<InvokeReturn>, Error> {
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
                let ffi_type = wrapper::type_ref::primitive(*primitive)?;
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

    fn finish_encoded_error(
        &self,
        error: returns::encoded::Tokens,
        empty: returns::encoded::Tokens,
    ) -> EncodedError {
        EncodedError {
            return_type: error.return_type().clone(),
            value: error.value().clone(),
            empty_value: empty.value().clone(),
        }
    }
}

impl<'expansion, 'lowered> Return<'expansion, 'lowered, Native> {
    fn encoded_error(
        &self,
        error_codec: &'lowered ReadPlan,
        error_shape: native::BufferShape,
    ) -> Result<EncodedError, Error> {
        let error_ident = names::Locals::new(Span::call_site()).error();
        let error =
            returns::encoded::Input::new(error_codec, error_shape, error_ident, self.expansion)
                .render()?;
        let empty = returns::encoded::Empty::<Native>::new(error_shape).render()?;
        Ok(self.finish_encoded_error(error, empty))
    }
}

impl<'expansion, 'lowered> Return<'expansion, 'lowered, Wasm32> {
    fn encoded_error(
        &self,
        error_codec: &'lowered ReadPlan,
        error_shape: wasm32::BufferShape,
    ) -> Result<EncodedError, Error> {
        let error_ident = names::Locals::new(Span::call_site()).error();
        let error =
            returns::encoded::Input::new(error_codec, error_shape, error_ident, self.expansion)
                .render()?;
        let empty = returns::encoded::Empty::<Wasm32>::new(error_shape).render()?;
        Ok(self.finish_encoded_error(error, empty))
    }
}

impl<'expansion, 'lowered> Return<'expansion, 'lowered, Native> {
    fn render(self) -> Result<InvokeReturn, Error> {
        if let Some(tokens) = self.direct_tokens()? {
            return Ok(tokens);
        }

        match (self.plan, self.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let value = wrapper::encoded::outgoing::Value::new(codec.root(), self.expansion)
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
                self.encoded_error(codec, native::BufferShape::Buffer)?,
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
                let ffi_type = wrapper::type_ref::primitive(*primitive)?;
                Ok(InvokeReturn::fallible(
                    self.encoded_error(codec, native::BufferShape::Buffer)?,
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
                self.encoded_error(codec, native::BufferShape::Buffer)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(self.rust_fallible_return()?.ok),
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
                let success = returns::encoded::Input::new(
                    ok_codec,
                    native::BufferShape::Buffer,
                    success_ident,
                    self.expansion,
                )
                .render()?;
                Ok(InvokeReturn::fallible(
                    self.encoded_error(error_codec, native::BufferShape::Buffer)?,
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

impl<'expansion, 'lowered> Return<'expansion, 'lowered, Wasm32> {
    fn render(self) -> Result<InvokeReturn, Error> {
        if let Some(tokens) = self.direct_tokens()? {
            return Ok(tokens);
        }

        match (self.plan, self.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let value = wrapper::encoded::outgoing::Value::new(codec.root(), self.expansion)
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
                self.encoded_error(codec, wasm32::BufferShape::Packed)?,
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
                let ffi_type = wrapper::type_ref::primitive(*primitive)?;
                Ok(InvokeReturn::fallible(
                    self.encoded_error(codec, wasm32::BufferShape::Packed)?,
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
                self.encoded_error(codec, wasm32::BufferShape::Packed)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(self.rust_fallible_return()?.ok),
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
                let success = returns::encoded::Input::new(
                    codec,
                    wasm32::BufferShape::Packed,
                    success_ident,
                    self.expansion,
                )
                .render()?;
                Ok(InvokeReturn::fallible(
                    self.encoded_error(error_codec, wasm32::BufferShape::Packed)?,
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
                return ::boltffi::__private::FfiBuf::EMPTY_PACKED;
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
