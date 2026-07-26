use boltffi_binding::{
    DirectValueType, DirectVectorElementType, IncomingParam, IntoRust, ParamDecl, ParamPlan,
    Receive, TypeRef,
};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::{DirectRecordCrossing, RenderSurface},
    wrapper::Render,
};

pub mod closure;
pub mod direct;
mod direct_vec;
pub mod encoded;
mod handle;
mod scalar_option;

pub struct Renderer;

pub fn requires_failure_return<S: RenderSurface>(param: &ParamDecl<S, IntoRust>) -> bool {
    match param.payload() {
        IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
            matches!(ty, DirectValueType::Record(_))
                && (matches!(S::DIRECT_RECORD_PARAMS, DirectRecordCrossing::Pointer)
                    || (S::BORROWED_DIRECT_RECORD_PARAMS
                        && matches!(receive, Receive::ByRef | Receive::ByMutRef)))
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

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    param: &'lowered ParamDecl<S, IntoRust>,
    source: rust_api::Parameter<'lowered>,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
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
        }
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

impl<'expansion, 'lowered, S> Render<S, Input<'expansion, 'lowered, S>> for Renderer
where
    S: RenderSurface,
    direct::Renderer: Render<S, direct::Input, Output = Tokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input, Output = Tokens>,
    closure::Renderer: Render<S, closure::Input<'expansion, 'lowered, S>, Output = Tokens>,
    encoded::Renderer: Render<S, encoded::Input<'expansion, 'lowered, S>, Output = Tokens>,
    handle::Renderer: Render<S, handle::Input<'lowered, S::HandleCarrier>, Output = Tokens>,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, S>) -> Result<Self::Output, Error> {
        let ident = input.source.ident()?;
        match input.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                <direct::Renderer as Render<S, _>>::render(
                    direct::Renderer,
                    direct::Input::new(
                        ty,
                        *receive,
                        input.source.written_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
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
                    input.source.decode_target(*receive)?,
                    ident,
                    input.failure,
                    input.expansion,
                );
                let encoded_input = match (receive, ty) {
                    (Receive::ByMutRef, TypeRef::Bytes) => encoded_input.into_mutable_bytes(),
                    (Receive::ByMutRef, _) => encoded_input.with_writeback(),
                    _ => encoded_input,
                };
                <encoded::Renderer as Render<S, _>>::render(encoded::Renderer, encoded_input)
            }
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                input.source.scalar_option(*primitive)?;
                <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(
                        *primitive,
                        input.source.written_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
            IncomingParam::Value(ParamPlan::DirectVec { element, receive }) => {
                <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Input::new(
                        element,
                        *receive,
                        input.source.direct_vec_element_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
            IncomingParam::Value(ParamPlan::Handle {
                target,
                carrier,
                presence,
                receive,
            }) => <handle::Renderer as Render<S, _>>::render(
                handle::Renderer,
                handle::Input::new(
                    handle::Plan::new(target, *carrier, *presence, *receive),
                    input.source,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Closure(closure) => <closure::Renderer as Render<S, _>>::render(
                closure::Renderer,
                closure::Input::new(
                    closure,
                    input.source.closure(closure.presence())?,
                    ident,
                    input.failure,
                    input.expansion,
                ),
            ),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
        }
    }
}
