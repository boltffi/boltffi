use boltffi_ast::{ClassDef, MethodDef};
use boltffi_binding::{
    ClassDecl, ClassId, ClassThreadSafety, Decl, ExecutionDecl, ExportedCallable, HandleTarget,
    IncomingParam, IntoRust, NativeSymbol, OutOfRust, ParamPlan, Receive, ReturnPlan,
};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::Ident;

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::RenderSurface,
    wrapper::{self, Render, associated_fn, export, names},
};

pub struct Renderer<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, ClassDef, ClassDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
    rust_type: Option<TokenStream>,
}

struct ClassOwner<'lowered, C> {
    source: &'lowered ClassDef,
    class: TokenStream,
    handle_type: Ident,
    handle: C,
}

#[derive(Clone, Copy, Default)]
struct ClassHandleOperations {
    new: bool,
    take: bool,
    shared: bool,
    mutable: bool,
    retained_shared: bool,
    retained_mutable: bool,
}

impl<'expansion, 'lowered, S: RenderSurface> Renderer<'expansion, 'lowered, S> {
    pub fn new(
        pair: DeclarationPair<'lowered, ClassDef, ClassDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            pair,
            expansion,
            rust_type: None,
        }
    }

    pub fn with_rust_type(mut self, rust_type: TokenStream) -> Self {
        self.rust_type = Some(rust_type);
        self
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
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
    {
        let source = self.pair.source();
        let binding = self.pair.binding();
        let class = names::SourceSpelling::new(&source.name)
            .ident("source class name is not a Rust identifier")?;
        let class_type = self.rust_type.clone().unwrap_or_else(|| quote! { #class });
        let class_names = names::Class::new(&class);
        let handle_type = class_names.handle();
        let retained_handle_type = class_names.retained_handle();
        let operations = ClassHandleOperations::new(binding, self.expansion);
        let handle = self.handle(&class_type, &handle_type, &retained_handle_type, operations);
        let thread_safety = self.thread_safety(binding, &class, &class_type);
        let release = self.release(binding.release(), binding.handle(), &handle_type)?;
        let exports = associated_fn::Renderer::new(
            ClassOwner {
                source,
                class: class_type,
                handle_type,
                handle: binding.handle(),
            },
            binding.initializers(),
            binding.methods(),
            self.expansion,
        )
        .render()?;

        Ok(quote! {
            #handle
            #thread_safety
            #release
            #exports
        })
    }

    fn handle(
        &self,
        class: &TokenStream,
        handle_type: &Ident,
        retained_handle_type: &Ident,
        operations: ClassHandleOperations,
    ) -> TokenStream {
        let new = operations.new.then(|| {
            quote! {
                fn new(value: #class) -> *mut Self {
                    Box::into_raw(Box::new(Self {
                        value: ::core::cell::UnsafeCell::new(value),
                        references: ::std::sync::atomic::AtomicUsize::new(1),
                        released: ::std::sync::atomic::AtomicBool::new(false),
                    }))
                }
            }
        });
        let take = operations.take.then(|| {
            quote! {
                unsafe fn take(handle: *mut Self) -> Option<#class> {
                    let state = unsafe { handle.as_ref()? };
                    state
                        .released
                        .store(true, ::std::sync::atomic::Ordering::Release);
                    if state
                        .references
                        .compare_exchange(
                            1,
                            0,
                            ::std::sync::atomic::Ordering::AcqRel,
                            ::std::sync::atomic::Ordering::Acquire,
                        )
                        .is_err()
                    {
                        return None;
                    }
                    let state = unsafe { *Box::from_raw(handle) };
                    Some(state.value.into_inner())
                }
            }
        });
        let shared = operations.shared().then(|| {
            quote! {
                #[inline(always)]
                unsafe fn shared<'class>(handle: *mut Self) -> &'class #class {
                    unsafe { &*(*handle).value.get() }
                }
            }
        });
        let mutable = operations.mutable().then(|| {
            quote! {
                #[inline(always)]
                unsafe fn mutable<'class>(handle: *mut Self) -> &'class mut #class {
                    unsafe { &mut *(*handle).value.get() }
                }
            }
        });
        let retain = operations.retained().then(|| {
            quote! {
                unsafe fn retain(handle: *mut Self) -> Option<#retained_handle_type> {
                    let state = unsafe { handle.as_ref()? };
                    if state.released.load(::std::sync::atomic::Ordering::Acquire) {
                        return None;
                    }

                    let mut references =
                        state.references.load(::std::sync::atomic::Ordering::Acquire);
                    loop {
                        if references == 0
                            || state.released.load(::std::sync::atomic::Ordering::Acquire)
                        {
                            return None;
                        }

                        match state.references.compare_exchange_weak(
                            references,
                            references + 1,
                            ::std::sync::atomic::Ordering::AcqRel,
                            ::std::sync::atomic::Ordering::Acquire,
                        ) {
                            Ok(_) => {
                                let handle = unsafe { ::core::ptr::NonNull::new_unchecked(handle) };
                                return Some(#retained_handle_type { handle });
                            }
                            Err(current) => references = current,
                        }
                    }
                }
            }
        });
        let retained_shared = operations.retained_shared.then(|| {
            quote! {
                fn shared(&self) -> &#class {
                    unsafe { #handle_type::shared(self.handle.as_ptr()) }
                }
            }
        });
        let retained_mutable = operations.retained_mutable.then(|| {
            quote! {
                fn mutable(&mut self) -> &mut #class {
                    unsafe { #handle_type::mutable(self.handle.as_ptr()) }
                }
            }
        });
        let retained_handle = operations.retained().then(|| {
            quote! {
                struct #retained_handle_type {
                    handle: ::core::ptr::NonNull<#handle_type>,
                }

                unsafe impl Send for #retained_handle_type {}

                impl #retained_handle_type {
                    #retained_shared
                    #retained_mutable
                }

                impl Drop for #retained_handle_type {
                    fn drop(&mut self) {
                        unsafe {
                            #handle_type::release_reference(self.handle.as_ptr());
                        }
                    }
                }
            }
        });
        quote! {
            struct #handle_type {
                value: ::core::cell::UnsafeCell<#class>,
                references: ::std::sync::atomic::AtomicUsize,
                released: ::std::sync::atomic::AtomicBool,
            }

            unsafe impl Send for #handle_type {}
            unsafe impl Sync for #handle_type {}

            impl #handle_type {
                unsafe fn release(handle: *mut Self) {
                    let Some(state) = (unsafe { handle.as_ref() }) else {
                        return;
                    };
                    state
                        .released
                        .store(true, ::std::sync::atomic::Ordering::Release);
                    unsafe {
                        Self::release_reference(handle);
                    }
                }

                #new
                #retain
                #take
                #shared
                #mutable

                unsafe fn release_reference(handle: *mut Self) {
                    let state = unsafe { handle.as_ref().expect("BoltFFI class handle is null") };
                    if state
                        .references
                        .fetch_sub(1, ::std::sync::atomic::Ordering::AcqRel)
                        == 1
                    {
                        ::std::sync::atomic::fence(::std::sync::atomic::Ordering::Acquire);
                        unsafe {
                            let state = *Box::from_raw(handle);
                            state.value.into_inner();
                        }
                    }
                }
            }

            #retained_handle
        }
    }

    fn thread_safety(
        &self,
        binding: &ClassDecl<S>,
        class: &Ident,
        class_type: &TokenStream,
    ) -> TokenStream {
        if binding.thread_safety() == ClassThreadSafety::UnsafeSingleThreaded {
            return TokenStream::new();
        }

        quote_spanned! {class.span()=>
            #[allow(dead_code)]
            const _: () = {
                #[diagnostic::on_unimplemented(
                    message = "BoltFFI: `{Self}` must be thread-safe (Send + Sync)",
                    note = "exported types can be accessed from any thread in the foreign language",
                    note = "add #[export(single_threaded)] if you guarantee single-threaded access"
                )]
                trait BoltFFIThreadSafe: Send + Sync {}
                impl<T: Send + Sync> BoltFFIThreadSafe for T {}
                fn _assert<T: BoltFFIThreadSafe>() {}
                fn _check() { _assert::<#class_type>(); }
            };
        }
    }

    fn release(
        &self,
        symbol: &'lowered NativeSymbol,
        handle: S::HandleCarrier,
        handle_type: &Ident,
    ) -> Result<TokenStream, Error>
    where
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
    {
        let cfg = S::cfg_attr();
        let symbol = names::Symbol::new(symbol).ident();
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(handle),
        )?;
        let ty = carrier.ty();
        let zero = carrier.zero();
        Ok(quote! {
            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #symbol(handle: #ty) {
                if handle != #zero {
                    unsafe {
                        #handle_type::release(handle as usize as *mut #handle_type);
                    }
                }
            }
        })
    }
}

impl<'expansion, 'lowered, S> associated_fn::Owner<'expansion, 'lowered, S>
    for ClassOwner<'lowered, S::HandleCarrier>
where
    'lowered: 'expansion,
    S: RenderSurface,
    wrapper::handle::Carrier: Render<
            S,
            wrapper::handle::CarrierInput<S::HandleCarrier>,
            Output = wrapper::handle::CarrierTokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::class(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::class_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let class = &self.class;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #class }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver_tokens::<S>(
                receive,
                export.method().clone(),
                export.callable().execution(),
                export.failure(),
            ),
        }
    }
}

impl ClassHandleOperations {
    fn new<'lowered, S: RenderSurface>(
        class: &ClassDecl<S>,
        expansion: &Expansion<'lowered, S>,
    ) -> Self {
        expansion
            .bindings()
            .decls()
            .iter()
            .flat_map(|declaration| declaration.exported_callables())
            .fold(Self::default(), |operations, callable| {
                operations.with_callable(class.id(), callable)
            })
            .with_class_receivers(class)
            .with_class_streams(class, expansion)
    }

    const fn shared(self) -> bool {
        self.shared || self.retained_shared
    }

    const fn mutable(self) -> bool {
        self.mutable || self.retained_mutable
    }

    const fn retained(self) -> bool {
        self.retained_shared || self.retained_mutable
    }

    fn with_callable<S: RenderSurface>(
        self,
        class_id: ClassId,
        callable: &ExportedCallable<S>,
    ) -> Self {
        callable.params().iter().fold(
            self.with_return(class_id, callable.returns().plan()),
            |operations, param| match param.payload() {
                IncomingParam::Value(plan) => operations.with_param(class_id, plan),
                IncomingParam::Closure(_) => operations,
            },
        )
    }

    fn with_class_receivers<S: RenderSurface>(self, class: &ClassDecl<S>) -> Self {
        class.methods().iter().fold(self, |operations, method| {
            operations.with_receiver(method.callable())
        })
    }

    fn with_class_streams<'lowered, S: RenderSurface>(
        mut self,
        class: &ClassDecl<S>,
        expansion: &Expansion<'lowered, S>,
    ) -> Self {
        if expansion.bindings().decls().iter().any(|declaration| {
            matches!(declaration, Decl::Stream(stream) if stream.owner() == Some(class.id()))
        }) {
            self.shared = true;
        }
        self
    }

    fn with_receiver<S: RenderSurface>(mut self, callable: &ExportedCallable<S>) -> Self {
        match (callable.execution(), callable.receiver()) {
            (ExecutionDecl::Synchronous(_), Some(Receive::ByRef)) => self.shared = true,
            (ExecutionDecl::Synchronous(_), Some(Receive::ByMutRef)) => self.mutable = true,
            (ExecutionDecl::Asynchronous(_), Some(Receive::ByRef)) => self.retained_shared = true,
            (ExecutionDecl::Asynchronous(_), Some(Receive::ByMutRef)) => {
                self.retained_mutable = true
            }
            _ => {}
        }
        self
    }

    fn with_param<S: RenderSurface>(
        mut self,
        class_id: ClassId,
        plan: &ParamPlan<S, IntoRust>,
    ) -> Self {
        let ParamPlan::Handle {
            target, receive, ..
        } = plan
        else {
            return self;
        };
        if !matches!(target, HandleTarget::Class(id) if *id == class_id) {
            return self;
        }
        match receive {
            Receive::ByValue => self.take = true,
            Receive::ByRef => self.shared = true,
            Receive::ByMutRef => self.mutable = true,
            _ => {}
        }
        self
    }

    fn with_return<S: RenderSurface>(
        mut self,
        class_id: ClassId,
        plan: &ReturnPlan<S, OutOfRust>,
    ) -> Self {
        match plan {
            ReturnPlan::HandleViaReturnSlot { target, .. }
            | ReturnPlan::HandleViaOutPointer { target, .. }
                if matches!(target, HandleTarget::Class(id) if *id == class_id) =>
            {
                self.new = true;
            }
            _ => {}
        }
        self
    }
}

impl<'lowered, C: Copy> ClassOwner<'lowered, C> {
    fn receiver_tokens<'expansion, S>(
        &self,
        receive: Receive,
        method: Ident,
        execution: &ExecutionDecl<S>,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: RenderSurface<HandleCarrier = C>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.handle),
        )?;
        let receiver = names::Locals::new(method.span()).receiver();
        let receiver_handle = names::Parameter::new(&receiver).handle();
        let ffi_type = carrier.ty();
        let failure = failure.render()?;
        let conversion = self.conversion(
            &receiver,
            &receiver_handle,
            execution,
            carrier.zero(),
            failure,
        );
        let binding = self.binding(&receiver_handle, execution);

        Ok((
            export::ReceiverTokens::new(
                vec![quote! { #receiver: #ffi_type }],
                vec![conversion],
                Vec::new(),
                false,
            ),
            export::RustCall::class_method(self.class.clone(), receiver, binding, receive, method)?,
        ))
    }

    fn conversion(
        &self,
        receiver: &Ident,
        receiver_handle: &Ident,
        execution: &ExecutionDecl<impl RenderSurface<HandleCarrier = C>>,
        zero: &TokenStream,
        failure: TokenStream,
    ) -> TokenStream {
        let handle_type = &self.handle_type;
        let retain = match execution {
            ExecutionDecl::Synchronous(_) => TokenStream::new(),
            ExecutionDecl::Asynchronous(_) => quote! {
                let #receiver_handle = match unsafe { #handle_type::retain(#receiver_handle) } {
                    Some(handle) => handle,
                    None => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: released class handle",
                            stringify!(#receiver)
                        ));
                        #failure
                    }
                };
            },
            _ => quote! {
                compile_error!("BoltFFI: unknown class method execution mode");
            },
        };

        quote! {
            if #receiver == #zero {
                ::boltffi::__private::set_last_error(format!(
                    "{}: null class handle",
                    stringify!(#receiver)
                ));
                #failure
            }
            let #receiver_handle = #receiver as usize as *mut #handle_type;
            #retain
        }
    }

    fn binding(
        &self,
        receiver_handle: &Ident,
        execution: &ExecutionDecl<impl RenderSurface<HandleCarrier = C>>,
    ) -> export::ClassReceiverBinding {
        match execution {
            ExecutionDecl::Synchronous(_) => {
                export::ClassReceiverBinding::Raw(self.handle_type.clone())
            }
            ExecutionDecl::Asynchronous(_) => {
                export::ClassReceiverBinding::Retained(receiver_handle.clone())
            }
            _ => export::ClassReceiverBinding::Raw(self.handle_type.clone()),
        }
    }
}
