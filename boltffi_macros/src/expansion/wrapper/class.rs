use boltffi_ast::{ClassDef, MethodDef};
use boltffi_binding::{
    ClassDecl, ClassId, ClassThreadSafety, Decl, ExecutionDecl, ExportedCallable, HandleTarget,
    IncomingParam, IntoRust, Native, NativeSymbol, OutOfRust, ParamPlan, Receive, ReturnPlan,
    Wasm32, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::Ident;

use crate::expansion::{
    contract::{DeclarationPair, Expansion},
    error::Error,
    rust_api,
    wrapper::{self, associated_fn, export, names},
};

pub struct Class<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> {
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

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Class<'expansion, 'lowered, S> {
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
}

impl<'expansion, 'lowered> Class<'expansion, 'lowered, Native> {
    pub fn render(self) -> Result<TokenStream, Error> {
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
        let exports = associated_fn::AssociatedFunctions::new(
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

    fn release(
        &self,
        symbol: &'lowered NativeSymbol,
        handle: native::HandleCarrier,
        handle_type: &Ident,
    ) -> Result<TokenStream, Error> {
        let symbol = names::Symbol::new(symbol).ident();
        let carrier = wrapper::handle::CarrierTokens::native(handle)?;
        let ty = carrier.ty();
        let zero = carrier.zero();
        Ok(quote! {
            #[cfg(not(target_arch = "wasm32"))]
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

impl<'expansion, 'lowered> Class<'expansion, 'lowered, Wasm32> {
    pub fn render(self) -> Result<TokenStream, Error> {
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
        let exports = associated_fn::AssociatedFunctions::new(
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

    fn release(
        &self,
        symbol: &'lowered NativeSymbol,
        handle: wasm32::HandleCarrier,
        handle_type: &Ident,
    ) -> Result<TokenStream, Error> {
        let symbol = names::Symbol::new(symbol).ident();
        let carrier = wrapper::handle::CarrierTokens::wasm32(handle)?;
        let ty = carrier.ty();
        let zero = carrier.zero();
        Ok(quote! {
            #[cfg(target_arch = "wasm32")]
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

impl<'expansion, 'lowered, S: boltffi_binding::SurfaceLower> Class<'expansion, 'lowered, S> {
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
}

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Native>
    for ClassOwner<'lowered, native::HandleCarrier>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::class(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::class_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let class = &self.class;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #class }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver_tokens_native(
                receive,
                export.method().clone(),
                export.callable().execution(),
                export.failure(),
            ),
        }
    }
}

impl<'expansion, 'lowered> associated_fn::Owner<'expansion, 'lowered, Wasm32>
    for ClassOwner<'lowered, wasm32::HandleCarrier>
where
    'lowered: 'expansion,
{
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered> {
        rust_api::MethodDeclarations::class(self.source)
    }

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered> {
        rust_api::Callable::class_method(method, self.source)
    }

    fn receiver(
        &self,
        export: associated_fn::ReceiverExport<'expansion, 'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        match export.callable().receiver() {
            None => {
                let class = &self.class;
                Ok((
                    export::ReceiverTokens::none(),
                    export::RustCall::associated(quote! { #class }, export.method().clone()),
                ))
            }
            Some(receive) => self.receiver_tokens_wasm32(
                receive,
                export.method().clone(),
                export.callable().execution(),
                export.failure(),
            ),
        }
    }
}

impl ClassHandleOperations {
    fn new<'lowered, S: boltffi_binding::SurfaceLower>(
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

    fn with_callable<S: boltffi_binding::SurfaceLower>(
        self,
        class_id: ClassId,
        callable: &ExportedCallable<S>,
    ) -> Self {
        let asynchronous = matches!(callable.execution(), ExecutionDecl::Asynchronous(_));
        callable.params().iter().fold(
            self.with_return(class_id, callable.returns().plan()),
            |operations, param| match param.payload() {
                IncomingParam::Value(plan) => operations.with_param(class_id, plan, asynchronous),
                IncomingParam::Closure(_) => operations,
            },
        )
    }

    fn with_class_receivers<S: boltffi_binding::SurfaceLower>(self, class: &ClassDecl<S>) -> Self {
        class.methods().iter().fold(self, |operations, method| {
            operations.with_receiver(method.callable())
        })
    }

    fn with_class_streams<'lowered, S: boltffi_binding::SurfaceLower>(
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

    fn with_receiver<S: boltffi_binding::SurfaceLower>(
        mut self,
        callable: &ExportedCallable<S>,
    ) -> Self {
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

    fn with_param<S: boltffi_binding::SurfaceLower>(
        mut self,
        class_id: ClassId,
        plan: &ParamPlan<S, IntoRust>,
        asynchronous: bool,
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
            Receive::ByRef if asynchronous => self.retained_shared = true,
            Receive::ByRef => self.shared = true,
            Receive::ByMutRef => self.mutable = true,
            _ => {}
        }
        self
    }

    fn with_return<S: boltffi_binding::SurfaceLower>(
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

impl<'lowered> ClassOwner<'lowered, native::HandleCarrier> {
    fn receiver_tokens_native<'expansion>(
        &self,
        receive: Receive,
        method: Ident,
        execution: &ExecutionDecl<Native>,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Native>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let carrier = wrapper::handle::CarrierTokens::native(self.handle)?;
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
}

impl<'lowered> ClassOwner<'lowered, wasm32::HandleCarrier> {
    fn receiver_tokens_wasm32<'expansion>(
        &self,
        receive: Receive,
        method: Ident,
        execution: &ExecutionDecl<Wasm32>,
        failure: associated_fn::ReceiverFailure<'expansion, 'lowered, Wasm32>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error> {
        let carrier = wrapper::handle::CarrierTokens::wasm32(self.handle)?;
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
}

impl<'lowered, C: Copy> ClassOwner<'lowered, C> {
    fn conversion(
        &self,
        receiver: &Ident,
        receiver_handle: &Ident,
        execution: &ExecutionDecl<impl boltffi_binding::SurfaceLower>,
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
                        ::boltffi::__private::set_last_error(concat!(stringify!(#receiver), ": released class handle"));
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
                ::boltffi::__private::set_last_error(concat!(stringify!(#receiver), ": null class handle"));
                #failure
            }
            let #receiver_handle = #receiver as usize as *mut #handle_type;
            #retain
        }
    }

    fn binding(
        &self,
        receiver_handle: &Ident,
        execution: &ExecutionDecl<impl boltffi_binding::SurfaceLower>,
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
