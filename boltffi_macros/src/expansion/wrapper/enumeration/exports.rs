use boltffi_ast::{EnumDef, MethodDef, Path as SourcePath, TypeExpr};
use boltffi_binding::{
    DirectValueType, ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, Receive, Wasm32,
    WritePlan,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    contract::Expansion,
    error::Error,
    rust_api,
    wrapper::{self, associated_fn, export, names},
};

pub struct Exports<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
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

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Exports<'expansion, 'lowered, S> {
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
}

impl<'expansion, 'lowered> Exports<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<TokenStream, Error> {
        associated_fn::AssociatedFunctions::new(
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

impl<'expansion, 'lowered> Exports<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<TokenStream, Error> {
        associated_fn::AssociatedFunctions::new(
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

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Native>
    for EnumOwner<'lowered>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::enumeration(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::enum_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let enumeration = &self.enumeration;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #enumeration }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver.clone().render_native(
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

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Wasm32>
    for EnumOwner<'lowered>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::enumeration(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::enum_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let enumeration = &self.enumeration;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #enumeration }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver.clone().render_wasm32(
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
    fn render_native<'expansion>(
        self,
        source: &'lowered EnumDef,
        rust_type: &Type,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Native>,
        expansion: &'expansion Expansion<'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match self {
            Self::Direct { ty } => {
                Self::render_direct_native(rust_type, &ty, receive, method, failure.render()?)
            }
            Self::Encoded { codec } => {
                Self::render_encoded_native(source, codec, receive, method, failure, expansion)
            }
        }
    }

    fn render_wasm32<'expansion>(
        self,
        source: &'lowered EnumDef,
        rust_type: &Type,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Wasm32>,
        expansion: &'expansion Expansion<'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match self {
            Self::Direct { ty } => {
                Self::render_direct_wasm32(rust_type, &ty, receive, method, failure.render()?)
            }
            Self::Encoded { codec } => {
                Self::render_encoded_wasm32(source, codec, receive, method, failure, expansion)
            }
        }
    }

    fn render_direct_native(
        rust_type: &Type,
        ty: &DirectValueType,
        receive: Receive,
        method: Ident,
        failure: TokenStream,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receiver = names::Locals::new(method.span()).receiver();
        let tokens = wrapper::param::direct::Input::new(
            ty,
            receive,
            rust_type.clone(),
            receiver.clone(),
            TokenStream::new(),
        )
        .native()?;
        let tokens = if receive == Receive::ByMutRef {
            tokens.with_direct_writeback(rust_type, &receiver, &failure)
        } else {
            tokens
        };
        Ok((
            export::ReceiverTokens::new(
                tokens.ffi_parameters().to_vec(),
                tokens.conversions().to_vec(),
                tokens.writebacks().to_vec(),
                receive == Receive::ByMutRef,
            ),
            export::RustCall::method(receiver, method),
        ))
    }

    fn render_direct_wasm32(
        rust_type: &Type,
        ty: &DirectValueType,
        receive: Receive,
        method: Ident,
        failure: TokenStream,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receiver = names::Locals::new(method.span()).receiver();
        let tokens = wrapper::param::direct::Input::new(
            ty,
            receive,
            rust_type.clone(),
            receiver.clone(),
            TokenStream::new(),
        )
        .wasm32()?;
        let tokens = if receive == Receive::ByMutRef {
            tokens.with_direct_writeback(rust_type, &receiver, &failure)
        } else {
            tokens
        };
        Ok((
            export::ReceiverTokens::new(
                tokens.ffi_parameters().to_vec(),
                tokens.conversions().to_vec(),
                tokens.writebacks().to_vec(),
                receive == Receive::ByMutRef,
            ),
            export::RustCall::method(receiver, method),
        ))
    }

    fn render_encoded_native<'expansion>(
        source: &'lowered EnumDef,
        codec: &'lowered WritePlan,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Native>,
        expansion: &'expansion Expansion<'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receiver = names::Locals::new(method.span()).receiver();
        let source_type = TypeExpr::enumeration(
            source.id.clone(),
            SourcePath::single(source.name.spelling()),
        );
        let failure = failure.render()?;
        let target = if receive == Receive::ByMutRef {
            rust_api::DecodeTarget::received(receive, &source_type)?
        } else {
            rust_api::DecodeTarget::by_value(&source_type)?
        };
        let tokens = wrapper::param::encoded::Input::new(
            codec,
            <Native as boltffi_binding::SurfaceLower>::encoded_param_shape(),
            target,
            receiver.clone(),
            failure.clone(),
            expansion,
        )
        .render()?;
        let tokens = if receive == Receive::ByMutRef {
            tokens.with_encoded_writeback(codec, &receiver, &failure, expansion)?
        } else {
            tokens
        };
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

    fn render_encoded_wasm32<'expansion>(
        source: &'lowered EnumDef,
        codec: &'lowered WritePlan,
        receive: Receive,
        method: Ident,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Wasm32>,
        expansion: &'expansion Expansion<'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let receiver = names::Locals::new(method.span()).receiver();
        let source_type = TypeExpr::enumeration(
            source.id.clone(),
            SourcePath::single(source.name.spelling()),
        );
        let failure = failure.render()?;
        let target = if receive == Receive::ByMutRef {
            rust_api::DecodeTarget::received(receive, &source_type)?
        } else {
            rust_api::DecodeTarget::by_value(&source_type)?
        };
        let tokens = wrapper::param::encoded::Input::new(
            codec,
            <Wasm32 as boltffi_binding::SurfaceLower>::encoded_param_shape(),
            target,
            receiver.clone(),
            failure.clone(),
            expansion,
        )
        .render()?;
        let tokens = if receive == Receive::ByMutRef {
            tokens.with_encoded_writeback(codec, &receiver, &failure, expansion)?
        } else {
            tokens
        };
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
