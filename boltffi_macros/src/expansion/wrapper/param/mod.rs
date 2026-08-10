use boltffi_binding::{
    DirectValueType, DirectVectorElementType, IncomingParam, IntoRust, Native, ParamDecl,
    ParamPlan, Receive, SurfaceLower, TypeRef, Wasm32,
};
use proc_macro2::TokenStream;

use crate::expansion::{contract::Expansion, error::Error, rust_api};

pub mod closure;
pub mod direct;
mod direct_vec;
pub mod encoded;
mod handle;
mod scalar_option;

pub fn native_requires_failure_return(param: &ParamDecl<Native, IntoRust>) -> bool {
    match param.payload() {
        IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
            matches!(ty, DirectValueType::Record(_))
                && matches!(receive, Receive::ByRef | Receive::ByMutRef)
        }
        IncomingParam::Value(ParamPlan::Encoded { .. })
        | IncomingParam::Value(ParamPlan::Handle { .. })
        | IncomingParam::Value(ParamPlan::ScalarOption { .. })
        | IncomingParam::Closure(_) => true,
        IncomingParam::Value(ParamPlan::DirectVec {
            element: DirectVectorElementType::Record(_),
            ..
        }) => true,
        IncomingParam::Value(ParamPlan::DirectVec {
            element: DirectVectorElementType::Primitive(_),
            ..
        }) => false,
        IncomingParam::Value(_) => true,
    }
}

pub fn wasm32_requires_failure_return(param: &ParamDecl<Wasm32, IntoRust>) -> bool {
    match param.payload() {
        IncomingParam::Value(ParamPlan::Direct { ty, .. }) => {
            matches!(ty, DirectValueType::Record(_))
        }
        IncomingParam::Value(ParamPlan::Encoded { .. })
        | IncomingParam::Value(ParamPlan::Handle { .. })
        | IncomingParam::Value(ParamPlan::ScalarOption { .. })
        | IncomingParam::Closure(_) => true,
        IncomingParam::Value(ParamPlan::DirectVec {
            element: DirectVectorElementType::Record(_),
            ..
        }) => true,
        IncomingParam::Value(ParamPlan::DirectVec {
            element: DirectVectorElementType::Primitive(_),
            ..
        }) => false,
        IncomingParam::Value(_) => true,
    }
}

/// Whether the wrapper hands a parameter straight to the call or has to keep it
/// alive past the wrapper's own stack frame (async exports).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Capture {
    #[default]
    Borrowed,
    Retained,
}

pub struct Input<'expansion, 'lowered, S: SurfaceLower> {
    param: &'lowered ParamDecl<S, IntoRust>,
    source: rust_api::Parameter<'lowered>,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    capture: Capture,
}

impl<'expansion, 'lowered, S: SurfaceLower> Input<'expansion, 'lowered, S> {
    pub fn new(
        param: &'lowered ParamDecl<S, IntoRust>,
        source: rust_api::Parameter<'lowered>,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            param,
            source,
            failure,
            expansion,
            capture: Capture::Borrowed,
        }
    }

    pub fn with_capture(mut self, capture: Capture) -> Self {
        self.capture = capture;
        self
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    argument: TokenStream,
}

impl Tokens {
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

    pub fn writebacks(&self) -> &[TokenStream] {
        &self.writebacks
    }

    pub fn argument(&self) -> &TokenStream {
        &self.argument
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<Tokens, Error> {
        let ident = self.source.ident()?;
        match self.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => direct::Input::new(
                ty,
                *receive,
                self.source.written_type()?,
                ident,
                self.failure,
            )
            .native(),
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                shape,
                receive,
                ty,
                ..
            }) => {
                let encoded_input = encoded::Input::new(
                    codec,
                    *shape,
                    self.source.decode_target(*receive)?,
                    ident,
                    self.failure,
                    self.expansion,
                );
                let encoded_input = match (receive, ty) {
                    (Receive::ByMutRef, TypeRef::Bytes) => encoded_input.into_mutable_bytes(),
                    (Receive::ByMutRef, _) => encoded_input.with_writeback(),
                    _ => encoded_input,
                };
                encoded_input.render()
            }
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                self.source.scalar_option(*primitive)?;
                scalar_option::Input::new(
                    *primitive,
                    self.source.written_type()?,
                    ident,
                    self.failure,
                )
                .native()
            }
            IncomingParam::Value(ParamPlan::DirectVec { element, receive }) => {
                direct_vec::Input::new(
                    element,
                    *receive,
                    self.source.direct_vec_element_type()?,
                    ident,
                    self.failure,
                )
                .render()
            }
            IncomingParam::Value(ParamPlan::Handle {
                target,
                carrier,
                presence,
                receive,
            }) => handle::Input::new(
                handle::Plan::new(target, *carrier, *presence, *receive),
                self.source,
                ident,
                self.failure,
                self.capture,
            )
            .render(),
            IncomingParam::Closure(closure) => closure::Input::new(
                closure,
                self.source.closure(closure.presence())?,
                ident,
                self.failure,
                self.expansion,
            )
            .render(),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
        }
    }
}

impl<'expansion, 'lowered> Input<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<Tokens, Error> {
        let ident = self.source.ident()?;
        match self.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => direct::Input::new(
                ty,
                *receive,
                self.source.written_type()?,
                ident,
                self.failure,
            )
            .wasm32(),
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                shape,
                receive,
                ty,
                ..
            }) => {
                let encoded_input = encoded::Input::new(
                    codec,
                    *shape,
                    self.source.decode_target(*receive)?,
                    ident,
                    self.failure,
                    self.expansion,
                );
                let encoded_input = match (receive, ty) {
                    (Receive::ByMutRef, TypeRef::Bytes) => encoded_input.into_mutable_bytes(),
                    (Receive::ByMutRef, _) => encoded_input.with_writeback(),
                    _ => encoded_input,
                };
                encoded_input.render()
            }
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                self.source.scalar_option(*primitive)?;
                scalar_option::Input::new(
                    *primitive,
                    self.source.written_type()?,
                    ident,
                    self.failure,
                )
                .wasm32()
            }
            IncomingParam::Value(ParamPlan::DirectVec { element, receive }) => {
                direct_vec::Input::new(
                    element,
                    *receive,
                    self.source.direct_vec_element_type()?,
                    ident,
                    self.failure,
                )
                .render()
            }
            IncomingParam::Value(ParamPlan::Handle {
                target,
                carrier,
                presence,
                receive,
            }) => handle::Input::new(
                handle::Plan::new(target, *carrier, *presence, *receive),
                self.source,
                ident,
                self.failure,
                self.capture,
            )
            .render(),
            IncomingParam::Closure(closure) => closure::Input::new(
                closure,
                self.source.closure(closure.presence())?,
                ident,
                self.failure,
                self.expansion,
            )
            .render(),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
        }
    }
}
