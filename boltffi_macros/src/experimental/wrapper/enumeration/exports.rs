use boltffi_ast::{EnumDef, MethodDef, Path as SourcePath, TypeExpr};
use boltffi_binding::{
    DirectValueType, ExportedMethodDecl, InitializerDecl, NativeSymbol, Receive, SurfaceLower,
    WritePlan,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, associated_fn, export, names},
};

pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered EnumDef,
    enumeration: TokenStream,
    rust_type: Type,
    receiver: Receiver<'lowered>,
    initializers: &'lowered [InitializerDecl<S>],
    methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EnumOwner<'lowered> {
    source: &'lowered EnumDef,
    enumeration: TokenStream,
    rust_type: Type,
    receiver: Receiver<'lowered>,
}

#[derive(Clone)]
pub enum Receiver<'lowered> {
    Direct { ty: DirectValueType },
    Encoded { codec: &'lowered WritePlan },
}

impl<'expansion, 'lowered, S: RenderSurface> Renderer<'expansion, 'lowered, S> {
    pub fn new(
        source: &'lowered EnumDef,
        enumeration: TokenStream,
        rust_type: Type,
        receiver: Receiver<'lowered>,
        initializers: &'lowered [InitializerDecl<S>],
        methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            source,
            enumeration,
            rust_type,
            receiver,
            initializers,
            methods,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        S: RenderSurface,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::param::direct::Renderer:
            Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        associated_fn::Renderer::new(
            EnumOwner {
                source: self.source,
                enumeration: self.enumeration,
                rust_type: self.rust_type,
                receiver: self.receiver,
            },
            self.initializers,
            self.methods,
            self.expansion,
        )
        .render()
    }
}

impl<'expansion, 'lowered, S> associated_fn::Owner<'expansion, 'lowered, S> for EnumOwner<'lowered>
where
    'lowered: 'expansion,
    S: RenderSurface,
    wrapper::param::direct::Renderer:
        Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::enumeration(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::enum_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let enumeration = &self.enumeration;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #enumeration }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver.clone().render(
                self.source,
                &self.rust_type,
                receive,
                export.method().clone(),
                export.failure(),
                export.expansion(),
            ),
        }
    }
}

impl<'lowered> Receiver<'lowered> {
    fn render<'expansion, S>(
        self,
        source: &'lowered EnumDef,
        rust_type: &Type,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, S>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: RenderSurface,
        wrapper::param::direct::Renderer:
            Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        match self {
            Self::Direct { ty } => Self::render_direct::<S>(rust_type, &ty, receive, method),
            Self::Encoded { codec } => {
                Self::render_encoded::<S>(source, codec, receive, method, failure, expansion)
            }
        }
    }

    fn render_direct<S>(
        rust_type: &Type,
        ty: &DirectValueType,
        receive: Receive,
        method: Ident,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: RenderSurface,
        wrapper::param::direct::Renderer:
            Render<S, wrapper::param::direct::Input, Output = wrapper::param::Tokens>,
    {
        if receive == Receive::ByMutRef {
            return Err(Error::UnsupportedExpansion(
                "mutable enum receiver without writeback",
            ));
        }
        let receiver = names::Locals::new(method.span()).receiver();
        let tokens = <wrapper::param::direct::Renderer as Render<S, _>>::render(
            wrapper::param::direct::Renderer,
            wrapper::param::direct::Input::new(
                ty,
                receive,
                rust_type.clone(),
                receiver.clone(),
                TokenStream::new(),
            ),
        )?;
        Ok((
            export::ReceiverTokens::new(
                tokens.ffi_parameters().to_vec(),
                tokens.conversions().to_vec(),
                tokens.writebacks().to_vec(),
                false,
            ),
            export::RustCall::method(receiver, method),
        ))
    }

    fn render_encoded<'expansion, S>(
        source: &'lowered EnumDef,
        codec: &'lowered WritePlan,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, S>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: RenderSurface,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        if receive == Receive::ByMutRef {
            return Err(Error::UnsupportedExpansion(
                "mutable encoded enum receiver without writeback",
            ));
        }
        let receiver = names::Locals::new(method.span()).receiver();
        let source_type = TypeExpr::enumeration(
            source.id.clone(),
            SourcePath::single(source.name.spelling()),
        );
        let tokens = <wrapper::param::encoded::Renderer as Render<S, _>>::render(
            wrapper::param::encoded::Renderer,
            wrapper::param::encoded::Input::new(
                codec,
                <S as SurfaceLower>::encoded_param_shape(),
                rust_api::DecodeTarget::by_value(&source_type)?,
                receiver.clone(),
                failure.render()?,
                expansion,
            ),
        )?;
        Ok((
            export::ReceiverTokens::new(
                tokens.ffi_parameters().to_vec(),
                tokens.conversions().to_vec(),
                tokens.writebacks().to_vec(),
                true,
            ),
            export::RustCall::method(receiver, method),
        ))
    }
}
