use boltffi_ast::{ClassDef, MethodDef};
use boltffi_binding::{
    ClassDecl, ClassThreadSafety, ExecutionDecl, Native, NativeSymbol, Receive, Wasm32, native,
    wasm32,
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
        let handle = self.handle(&class_type, &handle_type, &retained_handle_type);
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
        let handle = self.handle(&class_type, &handle_type, &retained_handle_type);
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
    ) -> TokenStream {
        let new = Some({
            quote! {
                pub fn new(value: #class) -> *mut Self {
                    Box::into_raw(Box::new(Self {
                        value: ::core::cell::UnsafeCell::new(value),
                        references: ::std::sync::atomic::AtomicUsize::new(1),
                        released: ::std::sync::atomic::AtomicBool::new(false),
                    }))
                }
            }
        });
        let take = Some({
            quote! {
                pub unsafe fn take(handle: *mut Self) -> Option<#class> {
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
        let shared = Some({
            quote! {
                #[inline(always)]
                pub unsafe fn shared<'class>(handle: *mut Self) -> &'class #class {
                    unsafe { &*(*handle).value.get() }
                }
            }
        });
        let mutable = Some({
            quote! {
                #[inline(always)]
                pub unsafe fn mutable<'class>(handle: *mut Self) -> &'class mut #class {
                    unsafe { &mut *(*handle).value.get() }
                }
            }
        });
        let retain = Some({
            quote! {
                pub unsafe fn retain(handle: *mut Self) -> Option<#retained_handle_type> {
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
        let retained_shared = Some({
            quote! {
                pub fn shared(&self) -> &#class {
                    unsafe { #handle_type::shared(self.handle.as_ptr()) }
                }
            }
        });
        let retained_mutable = Some({
            quote! {
                pub fn mutable(&mut self) -> &mut #class {
                    unsafe { #handle_type::mutable(self.handle.as_ptr()) }
                }
            }
        });
        let retained_handle = Some({
            quote! {
                #[doc(hidden)]
                pub struct #retained_handle_type {
                    handle: ::core::ptr::NonNull<#handle_type>,
                }

                unsafe impl Send for #retained_handle_type {}

                #[allow(dead_code, clippy::missing_safety_doc)]
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
            #[doc(hidden)]
            pub struct #handle_type {
                value: ::core::cell::UnsafeCell<#class>,
                references: ::std::sync::atomic::AtomicUsize,
                released: ::std::sync::atomic::AtomicBool,
            }

            unsafe impl Send for #handle_type {}
            unsafe impl Sync for #handle_type {}

            impl ::boltffi::__private::ClassHandle for #class {
                type Handle = #handle_type;
            }

            #[allow(dead_code, clippy::missing_safety_doc)]
            impl #handle_type {
                pub unsafe fn release(handle: *mut Self) {
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

                pub unsafe fn release_reference(handle: *mut Self) {
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
            #[allow(dead_code, clippy::missing_safety_doc)]
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
