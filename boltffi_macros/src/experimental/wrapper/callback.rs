use boltffi_ast::{
    AttributeInput, ExecutionKind, MethodDef, ParameterDef, ParameterPassing, Receiver, ReturnDef,
    TraitDef, TypeExpr, UserAttr,
};
use boltffi_binding::{
    CallbackDecl, CallbackLocalFunction, CallbackLocalMethodDecl, CallbackLocalProtocol,
    CanonicalName, ClosureForm, ClosureParameter, ClosureReturn, CodecNode, DirectValueType,
    DirectVectorElementType, Direction, ErrorDecl, ExecutionDecl, ExportedCallable, HandlePresence,
    HandleTarget, ImportSymbol, ImportedCallable, ImportedMethodDecl, IntoRust, Native, OutOfRust,
    OutgoingParam, ParamDecl, ParamDirection, ParamPlan, Primitive, ReadPlan, ReturnPlan,
    ReturnValueSlot, TypeRef, VTableSlot, Wasm32, native, wasm32,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, LitStr, Type, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    surface::{DirectRecordCrossing, RenderSurface},
    wrapper::{self, Render},
};

/// A callback trait declaration prepared for wrapper rendering.
pub struct Trait<'expansion, 'lowered, S: RenderSurface> {
    pair: DeclarationPair<'lowered, TraitDef, CallbackDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
    path: Option<TokenStream>,
    trait_object_impls: bool,
}

impl<'expansion, 'lowered, S: RenderSurface> Trait<'expansion, 'lowered, S> {
    /// Creates a callback trait from a paired source and lowered declaration.
    pub fn new(
        pair: DeclarationPair<'lowered, TraitDef, CallbackDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            pair,
            expansion,
            path: None,
            trait_object_impls: true,
        }
    }

    pub fn with_path(mut self, path: Option<TokenStream>, trait_object_impls: bool) -> Self {
        self.path = path;
        self.trait_object_impls = trait_object_impls;
        self
    }
}

/// A callback trait wrapper renderer.
pub struct Renderer;

impl<'expansion, 'lowered> Render<Native, Trait<'expansion, 'lowered, Native>> for Renderer {
    type Output = TokenStream;

    fn render(self, wrapper: Trait<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        NativeProtocol::new(
            wrapper.pair.source(),
            wrapper.pair.binding(),
            wrapper.path,
            wrapper.trait_object_impls,
            wrapper.expansion,
        )
        .tokens()
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Trait<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = TokenStream;

    fn render(self, wrapper: Trait<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        WasmProtocol::new(
            wrapper.pair.source(),
            wrapper.pair.binding(),
            wrapper.path,
            wrapper.trait_object_impls,
            wrapper.expansion,
        )
        .tokens()
    }
}

struct NativeProtocol<'expansion, 'lowered> {
    source: &'lowered TraitDef,
    binding: &'lowered CallbackDecl<Native>,
    path: Option<TokenStream>,
    trait_object_impls: bool,
    expansion: &'expansion Expansion<'lowered, Native>,
}

impl<'expansion, 'lowered> NativeProtocol<'expansion, 'lowered> {
    fn new(
        source: &'lowered TraitDef,
        binding: &'lowered CallbackDecl<Native>,
        path: Option<TokenStream>,
        trait_object_impls: bool,
        expansion: &'expansion Expansion<'lowered, Native>,
    ) -> Self {
        Self {
            source,
            binding,
            path,
            trait_object_impls,
            expansion,
        }
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        let local_protocol = self.binding.local_protocol();
        let names = CallbackNames::new(self.source, local_protocol)?;
        let protocol = self.binding.protocol();
        let vtable = protocol.vtable();
        let methods = CallbackMethods::new(
            self.source.methods.as_slice(),
            vtable.methods(),
            local_protocol.map(CallbackLocalProtocol::methods),
        )?;
        let local_names = names.local();
        let supports_trait_object = methods.supports_trait_object();
        let method_abis = methods
            .iter()
            .map(|method| {
                MethodAbi::new(
                    method.source,
                    method.binding.callable(),
                    method.local.map(CallbackLocalMethodDecl::callable),
                    self.expansion,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let method_fields = methods
            .iter()
            .zip(method_abis.iter())
            .map(|(method, abi)| abi.native_vtable_field(method.binding.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let parameter_items = method_abis
            .iter()
            .map(MethodAbi::parameter_items)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let foreign_methods = methods
            .iter()
            .zip(method_abis.iter())
            .map(|(method, abi)| abi.native_foreign_method(method.binding.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let local_methods = methods
            .iter()
            .zip(method_abis.iter())
            .filter_map(|(method, abi)| match (method.local, local_names) {
                (Some(local), Some(local_names)) => Some(abi.native_local_method(
                    method.binding.target(),
                    local.target(),
                    local_names,
                )),
                (Some(_), None) => Some(Err(Error::SourceSyntaxMismatch(
                    "callback local method exists without local protocol names",
                ))),
                (None, _) => None,
            })
            .collect::<Result<Vec<_>, _>>()?;
        let local_method_fields = local_methods
            .iter()
            .map(|method| method.vtable_field.clone())
            .collect::<Vec<_>>();
        let local_method_functions = local_methods
            .iter()
            .map(|method| method.function.clone())
            .collect::<Vec<_>>();
        let trait_ident = &names.trait_ident;
        let trait_path = self.path.unwrap_or_else(|| quote! { #trait_ident });
        let foreign_ident = &names.foreign_ident;
        let async_trait = AsyncTraitAttribute::from_trait(self.source)?;
        let vtable_ident = &names.vtable_ident;
        let foreign_vtable_static = &names.foreign_vtable_static;
        let register_ident = RustIdent::new(protocol.register().name().as_str())?;
        let create_ident = RustIdent::new(protocol.create_handle().name().as_str())?;
        let free_slot = RustIdent::new(vtable.free_slot().as_str())?;
        let clone_slot = RustIdent::new(vtable.clone_slot().as_str())?;
        let cfg = Native::cfg_attr();
        let local_protocol_tokens = local_names
            .map(|local_names| {
                let local_vtable_static = &local_names.vtable_static;
                let local_state_ident = &local_names.state_ident;
                let local_free_ident = &local_names.free_ident;
                let local_clone_ident = &local_names.clone_ident;
                let local_handle_ident = &local_names.handle_ident;
                quote! {
                    #cfg
                    type #local_state_ident = ::std::sync::Arc<dyn #trait_path>;

                    #cfg
                    extern "C" fn #local_free_ident(handle: u64) {
                        if handle == 0 {
                            return;
                        }
                        unsafe {
                            drop(Box::from_raw(handle as *mut #local_state_ident));
                        }
                    }

                    #cfg
                    extern "C" fn #local_clone_ident(handle: u64) -> u64 {
                        if handle == 0 {
                            return 0;
                        }
                        let callback = unsafe { &*(handle as *const #local_state_ident) };
                        Box::into_raw(Box::new(::std::sync::Arc::clone(callback))) as u64
                    }

                    #(#local_method_functions)*

                    #cfg
                    static #local_vtable_static: #vtable_ident = #vtable_ident {
                        #free_slot: #local_free_ident,
                        #clone_slot: #local_clone_ident,
                        #(#local_method_fields),*
                    };

                    #cfg
                    pub(crate) fn #local_handle_ident(
                        callback: ::std::sync::Arc<dyn #trait_path>,
                    ) -> ::boltffi::__private::CallbackHandle {
                        ::boltffi::__private::CallbackHandle::new(
                            Box::into_raw(Box::new(callback)) as u64,
                            &#local_vtable_static as *const #vtable_ident as *const ::core::ffi::c_void,
                        )
                    }
                }
            })
            .unwrap_or_default();
        let trait_object_tokens = if supports_trait_object && self.trait_object_impls {
            quote! {
                #cfg
                impl ::boltffi::__private::ArcFromCallbackHandle for dyn #trait_path {
                    unsafe fn arc_from_callback_handle(
                        handle: ::boltffi::__private::CallbackHandle,
                    ) -> ::std::sync::Arc<Self> {
                        debug_assert!(!handle.is_null());
                        ::std::sync::Arc::new(#foreign_ident {
                            vtable: handle.vtable() as *const #vtable_ident,
                            handle: handle.handle(),
                        })
                    }
                }

                #cfg
                impl ::boltffi::__private::BoxFromCallbackHandle for dyn #trait_path {
                    unsafe fn box_from_callback_handle(
                        handle: ::boltffi::__private::CallbackHandle,
                    ) -> Box<Self> {
                        debug_assert!(!handle.is_null());
                        Box::new(#foreign_ident {
                            vtable: handle.vtable() as *const #vtable_ident,
                            handle: handle.handle(),
                        })
                    }
                }

                #cfg
                impl ::boltffi::__private::CallbackForeignType for dyn #trait_path {
                    type Foreign = #foreign_ident;
                }
            }
        } else {
            TokenStream::new()
        };

        Ok(quote! {
            #cfg
            #[repr(C)]
            pub struct #vtable_ident {
                pub #free_slot: extern "C" fn(handle: u64),
                pub #clone_slot: extern "C" fn(handle: u64) -> u64,
                #(#method_fields),*
            }

            #cfg
            #[derive(Debug)]
            pub struct #foreign_ident {
                vtable: *const #vtable_ident,
                handle: u64,
            }

            #cfg
            unsafe impl Send for #foreign_ident {}

            #cfg
            unsafe impl Sync for #foreign_ident {}

            #cfg
            impl Drop for #foreign_ident {
                fn drop(&mut self) {
                    unsafe { ((*self.vtable).#free_slot)(self.handle) };
                }
            }

            #cfg
            impl Clone for #foreign_ident {
                fn clone(&self) -> Self {
                    let handle = unsafe { ((*self.vtable).#clone_slot)(self.handle) };
                    Self {
                        vtable: self.vtable,
                        handle,
                    }
                }
            }

            #cfg
            #async_trait
            impl #trait_path for #foreign_ident {
                #(#foreign_methods)*
            }

            #(#parameter_items)*

            #cfg
            static #foreign_vtable_static: ::std::sync::atomic::AtomicPtr<#vtable_ident> =
                ::std::sync::atomic::AtomicPtr::new(::core::ptr::null_mut());

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #register_ident(vtable: *const #vtable_ident) {
                #foreign_vtable_static.store(vtable as *mut _, ::std::sync::atomic::Ordering::Release);
            }

            #cfg
            #[unsafe(no_mangle)]
            pub extern "C" fn #create_ident(handle: u64) -> ::boltffi::__private::CallbackHandle {
                let vtable = #foreign_vtable_static.load(::std::sync::atomic::Ordering::Acquire);
                if vtable.is_null() {
                    return ::boltffi::__private::CallbackHandle::NULL;
                }
                ::boltffi::__private::CallbackHandle::new(handle, vtable as *const ::core::ffi::c_void)
            }

            #cfg
            impl ::boltffi::__private::ArcFromCallbackHandle for #foreign_ident {
                unsafe fn arc_from_callback_handle(
                    handle: ::boltffi::__private::CallbackHandle,
                ) -> ::std::sync::Arc<Self> {
                    debug_assert!(!handle.is_null());
                    ::std::sync::Arc::new(Self {
                        vtable: handle.vtable() as *const #vtable_ident,
                        handle: handle.handle(),
                    })
                }
            }

            #cfg
            impl ::boltffi::__private::BoxFromCallbackHandle for #foreign_ident {
                unsafe fn box_from_callback_handle(
                    handle: ::boltffi::__private::CallbackHandle,
                ) -> Box<Self> {
                    debug_assert!(!handle.is_null());
                    Box::new(Self {
                        vtable: handle.vtable() as *const #vtable_ident,
                        handle: handle.handle(),
                    })
                }
            }

            #trait_object_tokens

            #local_protocol_tokens
        })
    }
}

struct WasmProtocol<'expansion, 'lowered> {
    source: &'lowered TraitDef,
    binding: &'lowered CallbackDecl<Wasm32>,
    path: Option<TokenStream>,
    trait_object_impls: bool,
    expansion: &'expansion Expansion<'lowered, Wasm32>,
}

impl<'expansion, 'lowered> WasmProtocol<'expansion, 'lowered> {
    fn new(
        source: &'lowered TraitDef,
        binding: &'lowered CallbackDecl<Wasm32>,
        path: Option<TokenStream>,
        trait_object_impls: bool,
        expansion: &'expansion Expansion<'lowered, Wasm32>,
    ) -> Self {
        Self {
            source,
            binding,
            path,
            trait_object_impls,
            expansion,
        }
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        let local_protocol = self.binding.local_protocol();
        let names = CallbackNames::new(self.source, local_protocol)?;
        let protocol = self.binding.protocol();
        let methods = CallbackMethods::new(
            self.source.methods.as_slice(),
            protocol.methods(),
            local_protocol.map(CallbackLocalProtocol::methods),
        )?;
        let local_names = names.local();
        let supports_trait_object = methods.supports_trait_object();
        let method_abis = methods
            .iter()
            .map(|method| {
                MethodAbi::new(
                    method.source,
                    method.binding.callable(),
                    method.local.map(CallbackLocalMethodDecl::callable),
                    self.expansion,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let imports = methods
            .iter()
            .zip(method_abis.iter())
            .map(|(method, abi)| abi.wasm_import(method.binding.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let parameter_items = method_abis
            .iter()
            .map(MethodAbi::parameter_items)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let completions = method_abis
            .iter()
            .filter_map(MethodAbi::wasm_complete_export)
            .collect::<Result<Vec<_>, _>>()?;
        let foreign_methods = methods
            .iter()
            .zip(method_abis.iter())
            .map(|(method, abi)| abi.wasm_foreign_method(method.binding.target()))
            .collect::<Result<Vec<_>, _>>()?;
        let local_proxy_methods = methods
            .iter()
            .zip(method_abis.iter())
            .filter_map(|(method, abi)| {
                method
                    .local
                    .map(|local| abi.wasm_local_proxy_method(local.target()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let local_methods = methods
            .iter()
            .zip(method_abis.iter())
            .filter_map(|(method, abi)| match (method.local, local_names) {
                (Some(local), Some(local_names)) => {
                    Some(abi.wasm_local_method(local.target(), local_names))
                }
                (Some(_), None) => Some(Err(Error::SourceSyntaxMismatch(
                    "callback local method exists without local protocol names",
                ))),
                (None, _) => None,
            })
            .collect::<Result<Vec<_>, _>>()?;
        let trait_ident = &names.trait_ident;
        let trait_path = self.path.unwrap_or_else(|| quote! { #trait_ident });
        let foreign_ident = &names.foreign_ident;
        let create_ident = RustIdent::new(protocol.create_handle().name().as_str())?;
        let free_import = WasmImport::new(protocol.free())?;
        let foreign_free_ident = free_import.ident();
        let free_import = free_import.declaration(quote! {
            fn #foreign_free_ident(handle: u32);
        });
        let clone_import = WasmImport::new(protocol.clone_import())?;
        let clone_import_ident = clone_import.ident();
        let clone_import = clone_import.declaration(quote! {
            fn #clone_import_ident(handle: u32) -> u32;
        });
        let cfg = Wasm32::cfg_attr();
        let wasm_foreign_callback_handle_start = wasm32::FOREIGN_CALLBACK_HANDLE_START;
        let async_trait = AsyncTraitAttribute::from_trait(self.source)?;
        let trait_object_tokens = if supports_trait_object && self.trait_object_impls {
            match local_names {
                Some(local_names) => {
                    let local_proxy_ident = &local_names.proxy_ident;
                    let local_clone_ident = &local_names.clone_ident;
                    quote! {
                        #cfg
                        impl ::boltffi::__private::ArcFromCallbackHandle for dyn #trait_path {
                            unsafe fn arc_from_callback_handle(
                                handle: ::boltffi::__private::CallbackHandle,
                            ) -> ::std::sync::Arc<Self> {
                                debug_assert!(!handle.is_null());
                                let handle = handle.handle() as u32;
                                if handle < #wasm_foreign_callback_handle_start {
                                    let handle = #local_clone_ident(handle);
                                    return ::std::sync::Arc::new(#local_proxy_ident { handle });
                                }
                                ::std::sync::Arc::new(#foreign_ident { handle })
                            }
                        }

                        #cfg
                        impl ::boltffi::__private::BoxFromCallbackHandle for dyn #trait_path {
                            unsafe fn box_from_callback_handle(
                                handle: ::boltffi::__private::CallbackHandle,
                            ) -> Box<Self> {
                                debug_assert!(!handle.is_null());
                                let handle = handle.handle() as u32;
                                if handle < #wasm_foreign_callback_handle_start {
                                    let handle = #local_clone_ident(handle);
                                    return Box::new(#local_proxy_ident { handle });
                                }
                                Box::new(#foreign_ident { handle })
                            }
                        }

                        #cfg
                        impl ::boltffi::__private::CallbackForeignType for dyn #trait_path {
                            type Foreign = #foreign_ident;
                        }
                    }
                }
                None => quote! {
                    #cfg
                    impl ::boltffi::__private::ArcFromCallbackHandle for dyn #trait_path {
                        unsafe fn arc_from_callback_handle(
                            handle: ::boltffi::__private::CallbackHandle,
                            ) -> ::std::sync::Arc<Self> {
                                debug_assert!(!handle.is_null());
                                let handle = handle.handle() as u32;
                                if handle < #wasm_foreign_callback_handle_start {
                                    panic!("local callback handle cannot be converted to foreign callback");
                                }
                                ::std::sync::Arc::new(#foreign_ident { handle })
                            }
                        }

                    #cfg
                    impl ::boltffi::__private::BoxFromCallbackHandle for dyn #trait_path {
                        unsafe fn box_from_callback_handle(
                            handle: ::boltffi::__private::CallbackHandle,
                        ) -> Box<Self> {
                            debug_assert!(!handle.is_null());
                            let handle = handle.handle() as u32;
                            if handle < #wasm_foreign_callback_handle_start {
                                panic!("local callback handle cannot be converted to foreign callback");
                            }
                            Box::new(#foreign_ident { handle })
                        }
                    }

                    #cfg
                    impl ::boltffi::__private::CallbackForeignType for dyn #trait_path {
                        type Foreign = #foreign_ident;
                    }
                },
            }
        } else {
            TokenStream::new()
        };
        let local_protocol_tokens = local_names
            .map(|local_names| {
                let local_proxy_ident = &local_names.proxy_ident;
                let local_state_ident = &local_names.state_ident;
                let local_registry_ident = &local_names.registry_ident;
                let local_next_ident = &local_names.next_ident;
                let local_lookup_ident = &local_names.lookup_ident;
                let local_free_ident = &local_names.free_ident;
                let local_clone_ident = &local_names.clone_ident;
                let local_handle_ident = &local_names.handle_ident;
                quote! {
                    #cfg
                    type #local_state_ident = ::std::sync::Arc<dyn #trait_path>;

                    #cfg
                    #[derive(Debug)]
                    pub struct #local_proxy_ident {
                        handle: u32,
                    }

                    #cfg
                    impl Drop for #local_proxy_ident {
                        fn drop(&mut self) {
                            #local_free_ident(self.handle);
                        }
                    }

                    #cfg
                    #async_trait
                    impl #trait_path for #local_proxy_ident {
                        #(#local_proxy_methods)*
                    }

                    #cfg
                    ::std::thread_local! {
                        static #local_registry_ident: ::std::cell::RefCell<::std::collections::BTreeMap<u32, #local_state_ident>> =
                            ::std::cell::RefCell::new(::std::collections::BTreeMap::new());
                        static #local_next_ident: ::std::cell::Cell<u32> = const { ::std::cell::Cell::new(1) };
                    }

                    #cfg
                    fn #local_lookup_ident(handle: u32) -> #local_state_ident {
                        #local_registry_ident.with(|registry| {
                            registry
                                .borrow()
                                .get(&handle)
                                .cloned()
                                .unwrap_or_else(|| panic!("callback handle {} not found", handle))
                        })
                    }

                    #cfg
                    #[unsafe(no_mangle)]
                    pub extern "C" fn #local_free_ident(handle: u32) {
                        if handle == 0 {
                            return;
                        }
                        #local_registry_ident.with(|registry| {
                            registry.borrow_mut().remove(&handle);
                        });
                    }

                    #cfg
                    #[unsafe(no_mangle)]
                    pub extern "C" fn #local_clone_ident(handle: u32) -> u32 {
                        if handle == 0 {
                            return 0;
                        }
                        let callback = #local_lookup_ident(handle);
                        #local_handle_ident(callback).handle() as u32
                    }

                    #(#local_methods)*

                    #cfg
                    pub(crate) fn #local_handle_ident(
                        callback: ::std::sync::Arc<dyn #trait_path>,
                    ) -> ::boltffi::__private::CallbackHandle {
                        let handle = #local_registry_ident.with(|registry| {
                            #local_next_ident.with(|next_handle| {
                                let mut registry = registry.borrow_mut();
                                let start = next_handle.get();
                                let handle = (start..#wasm_foreign_callback_handle_start)
                                    .chain(1..start)
                                    .find(|candidate| !registry.contains_key(candidate))
                                    .unwrap_or_else(|| panic!("local callback handle space exhausted"));
                                let next_value = handle
                                    .checked_add(1)
                                    .filter(|candidate| *candidate != 0 && *candidate < #wasm_foreign_callback_handle_start)
                                    .unwrap_or(1);
                                next_handle.set(next_value);
                                registry.insert(handle, callback);
                                handle
                            })
                        });

                        ::boltffi::__private::CallbackHandle::from_wasm_handle(handle)
                    }
                }
            })
            .unwrap_or_default();

        Ok(quote! {
            #cfg
            #[derive(Debug)]
            pub struct #foreign_ident {
                handle: u32,
            }

            #cfg
            unsafe impl Send for #foreign_ident {}

            #cfg
            unsafe impl Sync for #foreign_ident {}

            #free_import
            #clone_import
            #(#imports)*
            #(#completions)*

            #cfg
            impl Drop for #foreign_ident {
                fn drop(&mut self) {
                    unsafe { #foreign_free_ident(self.handle) };
                }
            }

            #cfg
            impl Clone for #foreign_ident {
                fn clone(&self) -> Self {
                    let handle = unsafe { #clone_import_ident(self.handle) };
                    Self { handle }
                }
            }

            #cfg
            #async_trait
            impl #trait_path for #foreign_ident {
                #(#foreign_methods)*
            }

            #(#parameter_items)*

            #cfg
            #[unsafe(no_mangle)]
            pub extern "C" fn #create_ident(handle: u32) -> u32 {
                if handle < #wasm_foreign_callback_handle_start {
                    0
                } else {
                    handle
                }
            }

            #cfg
            impl ::boltffi::__private::ArcFromCallbackHandle for #foreign_ident {
                unsafe fn arc_from_callback_handle(
                    handle: ::boltffi::__private::CallbackHandle,
                ) -> ::std::sync::Arc<Self> {
                    debug_assert!(!handle.is_null());
                    let handle = handle.handle() as u32;
                    if handle < #wasm_foreign_callback_handle_start {
                        panic!("local callback handle cannot be converted to foreign callback");
                    }
                    ::std::sync::Arc::new(Self {
                        handle,
                    })
                }
            }

            #cfg
            impl ::boltffi::__private::BoxFromCallbackHandle for #foreign_ident {
                unsafe fn box_from_callback_handle(
                    handle: ::boltffi::__private::CallbackHandle,
                ) -> Box<Self> {
                    debug_assert!(!handle.is_null());
                    let handle = handle.handle() as u32;
                    if handle < #wasm_foreign_callback_handle_start {
                        panic!("local callback handle cannot be converted to foreign callback");
                    }
                    Box::new(Self {
                        handle,
                    })
                }
            }

            #trait_object_tokens

            #local_protocol_tokens
        })
    }
}

struct MethodPair<'lowered, S: RenderSurface, TargetName> {
    source: &'lowered MethodDef,
    binding: &'lowered ImportedMethodDecl<S, TargetName>,
    local: Option<&'lowered CallbackLocalMethodDecl<S>>,
}

struct CallbackMethods<'lowered, S: RenderSurface, TargetName> {
    methods: Vec<MethodPair<'lowered, S, TargetName>>,
}

impl<'lowered, S: RenderSurface, TargetName> CallbackMethods<'lowered, S, TargetName> {
    fn new(
        source: &'lowered [MethodDef],
        binding: &'lowered [ImportedMethodDecl<S, TargetName>],
        local: Option<&'lowered [CallbackLocalMethodDecl<S>]>,
    ) -> Result<Self, Error> {
        if source.len() != binding.len() || local.is_some_and(|local| source.len() != local.len()) {
            return Err(Error::SourceSyntaxMismatch(
                "source callback method count does not match binding method count",
            ));
        }
        let methods = source
            .iter()
            .zip(binding)
            .enumerate()
            .map(|(index, (source, binding))| {
                let local = local.and_then(|local| local.get(index));
                let source_name = CanonicalName::from(&source.name);
                if &source_name != binding.name()
                    || local.is_some_and(|local| &source_name != local.name())
                {
                    return Err(Error::SourceSyntaxMismatch(
                        "callback method identity does not match binding method identity",
                    ));
                }
                Ok(MethodPair {
                    source,
                    binding,
                    local,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { methods })
    }

    fn iter(&self) -> impl Iterator<Item = &MethodPair<'lowered, S, TargetName>> {
        self.methods.iter()
    }

    fn supports_trait_object(&self) -> bool {
        self.methods
            .iter()
            .map(|method| method.source)
            .all(Self::accepts_method)
    }

    fn accepts_method(method: &MethodDef) -> bool {
        method.execution == ExecutionKind::Sync
            && method.receiver == Receiver::Shared
            && method
                .parameters
                .iter()
                .all(|parameter| Self::accepts_type(&parameter.type_expr))
            && match &method.returns {
                ReturnDef::Void => true,
                ReturnDef::Value(type_expr) => Self::accepts_type(type_expr),
            }
    }

    fn accepts_type(type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::ImplTrait(_) | TypeExpr::SelfType | TypeExpr::Parameter(_) => false,
            TypeExpr::Boxed(inner)
            | TypeExpr::Arc(inner)
            | TypeExpr::Vec(inner)
            | TypeExpr::Slice(inner)
            | TypeExpr::Option(inner) => Self::accepts_type(inner),
            TypeExpr::Result { ok, err } => Self::accepts_type(ok) && Self::accepts_type(err),
            TypeExpr::Tuple(elements) => elements.iter().all(Self::accepts_type),
            TypeExpr::Map { key, value, .. } => {
                Self::accepts_type(key) && Self::accepts_type(value)
            }
            TypeExpr::Primitive(_)
            | TypeExpr::Unit
            | TypeExpr::String
            | TypeExpr::Str
            | TypeExpr::InternedString { .. }
            | TypeExpr::Builtin(_)
            | TypeExpr::Record { .. }
            | TypeExpr::Enum { .. }
            | TypeExpr::Class { .. }
            | TypeExpr::Custom { .. }
            | TypeExpr::Dyn(_)
            | TypeExpr::FnPtr(_) => true,
        }
    }
}

struct MethodAbi<'expansion, 'lowered, S: RenderSurface> {
    source: &'lowered MethodDef,
    callable: &'lowered ImportedCallable<S>,
    local_callable: Option<&'lowered ExportedCallable<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S> MethodAbi<'expansion, 'lowered, S>
where
    S: CallbackMethodSurface,
    wrapper::returns::direct_vec::Incoming:
        Render<S, wrapper::returns::direct_vec::IncomingInput, Output = TokenStream>,
    wrapper::returns::direct_vec::Renderer:
        Render<S, wrapper::returns::direct_vec::Input, Output = wrapper::returns::Tokens>,
    wrapper::returns::scalar_option::Incoming:
        Render<S, wrapper::returns::scalar_option::IncomingInput, Output = TokenStream>,
    wrapper::returns::scalar_option::Renderer:
        Render<S, wrapper::returns::scalar_option::Input, Output = wrapper::returns::Tokens>,
    wrapper::returns::closure::Write: Render<
            S,
            wrapper::returns::closure::WriteInput<'expansion, 'lowered, S>,
            Output = wrapper::returns::closure::WriteTokens,
        >,
{
    fn new(
        source: &'lowered MethodDef,
        callable: &'lowered ImportedCallable<S>,
        local_callable: Option<&'lowered ExportedCallable<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Ok(Self {
                source,
                callable,
                local_callable,
                expansion,
            }),
            ExecutionDecl::Asynchronous(_) => Ok(Self {
                source,
                callable,
                local_callable,
                expansion,
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown callback method execution",
            )),
        }
    }

    fn native_vtable_field(&self, slot: &VTableSlot) -> Result<TokenStream, Error> {
        let slot = RustIdent::new(slot.as_str())?;
        let parameters = self.parameters()?.ffi_types;
        let return_tokens = self.return_tokens()?;
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            let completion = return_tokens.native_async_completion_type()?;
            return Ok(quote! {
                pub #slot: extern "C" fn(
                    handle: u64,
                    #(#parameters,)*
                    completion: #completion,
                    completion_data: *mut ::core::ffi::c_void
                )
            });
        }
        let return_parameters = return_tokens.foreign_ffi_parameters();
        let return_type = return_tokens.foreign_return_type();
        Ok(quote! {
            pub #slot: extern "C" fn(handle: u64 #(, #return_parameters)* #(, #parameters)*) #return_type
        })
    }

    fn native_foreign_method(&self, slot: &VTableSlot) -> Result<TokenStream, Error> {
        let method_ident = RustIdent::new(self.source.name.spelling())?;
        let receiver = ReceiverTokens::new(self.source.receiver)?.tokens();
        let source_parameters = self.source_parameters()?;
        let return_signature = self.return_signature()?;
        let slot = RustIdent::new(slot.as_str())?;
        let parameters = self.parameters()?;
        let return_tokens = self.return_tokens()?;
        let setup = parameters.foreign_setup;
        let arguments = parameters.foreign_arguments;
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            let call = quote! {
                {
                    #(#setup)*
                    ((*self.vtable).#slot)(
                        self.handle,
                        #(#arguments,)*
                        __boltffi_completion,
                        __boltffi_completion_data
                    )
                }
            };
            let body = return_tokens.native_async_foreign_body(call)?;
            return Ok(quote! {
                async fn #method_ident(#receiver #(, #source_parameters)*) #return_signature {
                    #body
                }
            });
        }
        let return_arguments = return_tokens.foreign_arguments();
        let call = quote! {
            ((*self.vtable).#slot)(self.handle #(, #return_arguments)* #(, #arguments)*)
        };
        let body = return_tokens.foreign_body(call)?;
        Ok(quote! {
            fn #method_ident(#receiver #(, #source_parameters)*) #return_signature {
                #(#setup)*
                #body
            }
        })
    }

    fn native_local_method(
        &self,
        slot: &VTableSlot,
        function: &CallbackLocalFunction,
        names: &LocalCallbackNames,
    ) -> Result<LocalNativeMethod, Error> {
        let slot_ident = RustIdent::new(slot.as_str())?;
        let function_ident = RustIdent::from_local_function(function)?;
        let parameters = self.parameters()?;
        let return_tokens = self.return_tokens()?;
        let ffi_parameters = parameters.ffi_parameters;
        let return_parameters = return_tokens.local_ffi_parameters();
        let setup = parameters.local_setup;
        let arguments = parameters.local_arguments;
        let return_type = return_tokens.local_return_type();
        let method_ident = RustIdent::new(self.source.name.spelling())?;
        let local_state_ident = &names.state_ident;
        let call = quote! { callback.#method_ident(#(#arguments),*) };
        let local_return = return_tokens.local_body(function_ident.as_ident().clone(), call)?;
        let local_return_items = local_return.items;
        let body = local_return.body;
        Ok(LocalNativeMethod {
            vtable_field: quote! { #slot_ident: #function_ident },
            function: quote! {
                #(#local_return_items)*
                #[cfg(not(target_arch = "wasm32"))]
                extern "C" fn #function_ident(
                    handle: u64
                    #(, #return_parameters)*
                    #(, #ffi_parameters)*
                ) #return_type {
                    let callback = unsafe { &*(handle as *const #local_state_ident) };
                    #(#setup)*
                    #body
                }
            },
        })
    }

    fn wasm_import(&self, import: &ImportSymbol) -> Result<TokenStream, Error> {
        let import = WasmImport::new(import)?;
        let parameters = self.parameters()?;
        let import_parameters = parameters.wasm_import_parameters();
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            let ident = import.ident();
            return Ok(import.declaration(quote! {
                fn #ident(handle: u32, request_id: u32 #(, #import_parameters)*);
            }));
        }
        let return_tokens = self.return_tokens()?;
        let return_parameters = return_tokens.foreign_ffi_parameters();
        let return_type = return_tokens.foreign_return_type();
        let ident = import.ident();
        Ok(import.declaration(quote! {
            fn #ident(handle: u32 #(, #return_parameters)* #(, #import_parameters)*) #return_type;
        }))
    }

    fn wasm_foreign_method(&self, import: &ImportSymbol) -> Result<TokenStream, Error> {
        let method_ident = RustIdent::new(self.source.name.spelling())?;
        let receiver = ReceiverTokens::new(self.source.receiver)?.tokens();
        let source_parameters = self.source_parameters()?;
        let return_signature = self.return_signature()?;
        let import = RustIdent::new(import.name().as_str())?;
        let parameters = self.parameters()?;
        let return_tokens = self.return_tokens()?;
        let setup = parameters.foreign_setup;
        let arguments = parameters.foreign_arguments;
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            let call = quote! {
                {
                    #(#setup)*
                    #import(
                        self.handle,
                        __boltffi_request.as_u32()
                        #(, #arguments)*
                    )
                }
            };
            let body = return_tokens.wasm_async_foreign_body(call)?;
            return Ok(quote! {
                async fn #method_ident(#receiver #(, #source_parameters)*) #return_signature {
                    #body
                }
            });
        }
        let return_arguments = return_tokens.foreign_arguments();
        let call = quote! { #import(self.handle #(, #return_arguments)* #(, #arguments)*) };
        let body = return_tokens.foreign_body(call)?;
        Ok(quote! {
            fn #method_ident(#receiver #(, #source_parameters)*) #return_signature {
                #(#setup)*
                #body
            }
        })
    }

    fn wasm_local_proxy_method(
        &self,
        function: &CallbackLocalFunction,
    ) -> Result<TokenStream, Error> {
        let method_ident = RustIdent::new(self.source.name.spelling())?;
        let receiver = ReceiverTokens::new(self.source.receiver)?.tokens();
        let source_parameters = self.source_parameters()?;
        let return_signature = self.return_signature()?;
        let function_ident = RustIdent::from_local_function(function)?;
        let parameters = self.parameters()?;
        let return_tokens = self.return_tokens()?;
        let setup = parameters.foreign_setup;
        let arguments = parameters.foreign_arguments;
        let return_arguments = return_tokens.local_arguments();
        let call = quote! { #function_ident(self.handle #(, #return_arguments)* #(, #arguments)*) };
        let body = return_tokens.local_proxy_body(call)?;
        Ok(quote! {
            fn #method_ident(#receiver #(, #source_parameters)*) #return_signature {
                #(#setup)*
                #body
            }
        })
    }

    fn wasm_local_method(
        &self,
        function: &CallbackLocalFunction,
        names: &LocalCallbackNames,
    ) -> Result<TokenStream, Error> {
        let function_ident = RustIdent::from_local_function(function)?;
        let parameters = self.parameters()?;
        let return_tokens = self.return_tokens()?;
        let ffi_parameters = parameters.ffi_parameters;
        let return_parameters = return_tokens.local_ffi_parameters();
        let setup = parameters.local_setup;
        let arguments = parameters.local_arguments;
        let return_type = return_tokens.local_return_type();
        let method_ident = RustIdent::new(self.source.name.spelling())?;
        let local_lookup_ident = &names.lookup_ident;
        let cfg = Wasm32::cfg_attr();
        let call = quote! { callback.#method_ident(#(#arguments),*) };
        let local_return = return_tokens.local_body(function_ident.as_ident().clone(), call)?;
        let local_return_items = local_return.items;
        let body = local_return.body;
        Ok(quote! {
            #(#local_return_items)*
            #cfg
            #[unsafe(no_mangle)]
            pub extern "C" fn #function_ident(
                handle: u32
                #(, #return_parameters)*
                #(, #ffi_parameters)*
            ) #return_type {
                let callback = #local_lookup_ident(handle);
                #(#setup)*
                #body
            }
        })
    }

    fn parameters(&self) -> Result<MethodParameters, Error> {
        if self.callable.params().len() != self.source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "source callback method parameter count does not match binding parameter count",
            ));
        }
        let local_params = self.local_callable.map(ExportedCallable::params);
        if local_params.is_some_and(|params| params.len() != self.source.parameters.len()) {
            return Err(Error::SourceSyntaxMismatch(
                "source callback method parameter count does not match local binding parameter count",
            ));
        }
        self.callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .enumerate()
            .map(|(index, (binding, source))| {
                MethodParameter::new(
                    binding,
                    local_params.and_then(|params| params.get(index)),
                    source,
                    self.expansion,
                )
                .tokens()
            })
            .collect::<Result<Vec<_>, _>>()
            .map(MethodParameters::from)
    }

    fn parameter_items(&self) -> Result<Vec<TokenStream>, Error> {
        self.parameters().map(|parameters| parameters.items)
    }

    fn return_tokens(&self) -> Result<MethodReturn<'expansion, 'lowered, S>, Error> {
        MethodReturn::new(
            self.callable.returns().plan(),
            self.callable.error(),
            self.local_callable,
            rust_api::Return::new(&self.source.returns),
            self.expansion,
        )
    }

    fn source_parameters(&self) -> Result<Vec<TokenStream>, Error> {
        self.source
            .parameters
            .iter()
            .map(|parameter| {
                let ident = RustIdent::new(parameter.name.spelling())?;
                let ty = rust_api::TypeTokens::parameter(parameter.passing, &parameter.type_expr)?
                    .into_type();
                Ok(quote! { #ident: #ty })
            })
            .collect()
    }

    fn return_signature(&self) -> Result<TokenStream, Error> {
        match &self.source.returns {
            ReturnDef::Void => Ok(TokenStream::new()),
            ReturnDef::Value(type_expr) => {
                let ty = rust_api::TypeTokens::new(type_expr)?.into_type();
                Ok(quote! { -> #ty })
            }
        }
    }
}

impl<'expansion, 'lowered> MethodAbi<'expansion, 'lowered, Wasm32> {
    fn wasm_complete_export(&self) -> Option<Result<TokenStream, Error>> {
        match self.callable.execution() {
            ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::CallbackCompletion { complete }) => {
                let complete_ident = RustIdent::new(complete.name().as_str());
                Some(complete_ident.map(|complete_ident| {
                    quote! {
                        #[cfg(target_arch = "wasm32")]
                        #[unsafe(no_mangle)]
                        pub unsafe extern "C" fn #complete_ident(
                            request_id: u32,
                            completion_code: i32,
                            data_ptr: u32,
                            data_len: u32,
                            data_cap: u32,
                        ) -> i32 {
                            unsafe {
                                ::boltffi::__private::AsyncCallbackRegistry::current().complete_from_ffi(
                                    request_id,
                                    completion_code,
                                    data_ptr,
                                    data_len,
                                    data_cap,
                                )
                            }
                        }
                    }
                }))
            }
            ExecutionDecl::Asynchronous(_) => Some(Err(Error::UnsupportedExpansion(
                "wasm callback async completion protocol",
            ))),
            _ => None,
        }
    }
}

struct MethodParameter<'expansion, 'lowered, S: RenderSurface> {
    binding: &'lowered ParamDecl<S, OutOfRust>,
    local: Option<&'lowered ParamDecl<S, IntoRust>>,
    source: &'lowered ParameterDef,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: CallbackMethodSurface> MethodParameter<'expansion, 'lowered, S> {
    fn new(
        binding: &'lowered ParamDecl<S, OutOfRust>,
        local: Option<&'lowered ParamDecl<S, IntoRust>>,
        source: &'lowered ParameterDef,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            binding,
            local,
            source,
            expansion,
        }
    }

    fn tokens(self) -> Result<MethodParameterTokens, Error> {
        let foreign = self.foreign_tokens()?;
        let local = self.local_tokens()?;
        Ok(MethodParameterTokens {
            items: foreign.items,
            ffi_parameters: local.ffi_parameters,
            ffi_types: foreign.ffi_types,
            foreign_setup: foreign.setup,
            foreign_arguments: foreign.arguments,
            local_setup: local.setup,
            local_arguments: local.arguments,
        })
    }

    fn foreign_tokens(&self) -> Result<ForeignMethodParameterTokens, Error> {
        match self.binding.payload() {
            OutgoingParam::Value(ParamPlan::Direct { ty, receive: () }) => {
                self.foreign_direct_tokens(ty)
            }
            OutgoingParam::Value(ParamPlan::Encoded {
                codec,
                shape,
                receive: (),
                ..
            }) => self.foreign_encoded_tokens(codec, *shape),
            OutgoingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                self.foreign_scalar_option_tokens(*primitive)
            }
            OutgoingParam::Value(ParamPlan::DirectVec { element, .. }) => {
                self.foreign_direct_vec_tokens(element)
            }
            OutgoingParam::Value(_) => Err(Error::UnsupportedExpansion(
                "callback method parameter shape",
            )),
            OutgoingParam::Closure(closure) => {
                S::foreign_closure_parameter_tokens(closure, self.source, self.expansion)
            }
        }
    }

    fn local_tokens(&self) -> Result<LocalMethodParameterTokens, Error> {
        let Some(local) = self.local else {
            return Ok(LocalMethodParameterTokens::default());
        };
        let ident = RustIdent::new(self.source.name.spelling())?;
        S::local_parameter_tokens(
            local,
            self.source,
            quote! {
                panic!(
                    "callback method argument conversion failed for {}",
                    stringify!(#ident)
                )
            },
            self.expansion,
        )
    }

    fn foreign_direct_tokens(
        &self,
        ty: &DirectValueType,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        let ident = RustIdent::new(self.source.name.spelling())?;
        match (self.source.passing, ty) {
            (ParameterPassing::Value, DirectValueType::Primitive(primitive)) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(ForeignMethodParameterTokens::new(
                    ffi_type,
                    Vec::new(),
                    quote! { #ident },
                ))
            }
            (ParameterPassing::Ref, DirectValueType::Primitive(primitive)) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(ForeignMethodParameterTokens::new(
                    ffi_type,
                    Vec::new(),
                    quote! { *#ident },
                ))
            }
            (ParameterPassing::RefMut, DirectValueType::Primitive(_)) => Err(
                Error::UnsupportedExpansion("mutable borrowed callback method parameter"),
            ),
            (ParameterPassing::Value, DirectValueType::Record(_)) => {
                let rust_type = rust_api::TypeTokens::new(&self.source.type_expr)?.into_type();
                let packed = wrapper::names::Parameter::new(ident.as_ident()).packed();
                match S::DIRECT_RECORD_PARAMS {
                    DirectRecordCrossing::Value => Ok(ForeignMethodParameterTokens::new(
                        quote! { <#rust_type as ::boltffi::__private::Passable>::Out },
                        vec![quote! {
                            let #packed = <#rust_type as ::boltffi::__private::Passable>::pack(#ident);
                        }],
                        quote! { #packed },
                    )),
                    DirectRecordCrossing::Pointer => {
                        let pointer = wrapper::names::Parameter::new(ident.as_ident()).pointer();
                        Ok(ForeignMethodParameterTokens::new(
                            quote! { *const u8 },
                            vec![quote! {
                                let #packed = <#rust_type as ::boltffi::__private::Passable>::pack(#ident);
                                let #pointer = &#packed as *const _ as *const u8;
                            }],
                            quote! { #pointer },
                        ))
                    }
                }
            }
            (ParameterPassing::Value, _) => {
                let rust_type = rust_api::TypeTokens::new(&self.source.type_expr)?.into_type();
                let ffi_type = quote! { <#rust_type as ::boltffi::__private::Passable>::Out };
                let packed = wrapper::names::Parameter::new(ident.as_ident()).packed();
                Ok(ForeignMethodParameterTokens::new(
                    ffi_type,
                    vec![quote! {
                        let #packed = <#rust_type as ::boltffi::__private::Passable>::pack(#ident);
                    }],
                    quote! { #packed },
                ))
            }
            (ParameterPassing::Ref | ParameterPassing::RefMut, _) => Err(
                Error::UnsupportedExpansion("borrowed non-primitive callback method parameter"),
            ),
        }
    }

    fn foreign_encoded_tokens(
        &self,
        codec: &ReadPlan,
        shape: S::BufferShape,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        match self.source.passing {
            ParameterPassing::Value | ParameterPassing::Ref => {}
            ParameterPassing::RefMut => {
                return Err(Error::UnsupportedExpansion(
                    "mutable borrowed encoded callback method parameter",
                ));
            }
        }
        let ident = RustIdent::new(self.source.name.spelling())?;
        let parameter_names = wrapper::names::Parameter::new(ident.as_ident());
        let buffer = parameter_names.buffer();
        let pointer = parameter_names.pointer();
        let length = parameter_names.length();
        let outgoing = wrapper::encoded::outgoing::Value::new(codec.root(), self.expansion);
        let foreign_value = match self.source.passing {
            ParameterPassing::Value => outgoing.buffer(quote! { #ident })?,
            ParameterPassing::Ref => outgoing.borrowed_buffer(quote! { #ident })?,
            ParameterPassing::RefMut => unreachable!(),
        };
        match S::callback_encoded_parameter(shape)? {
            CallbackEncodedParameter::Slice => Ok(ForeignMethodParameterTokens::new(
                quote! { *const u8 },
                vec![quote! {
                    let #buffer = #foreign_value;
                    let #pointer = #buffer.as_ptr();
                    let #length = #buffer.len();
                }],
                quote! { #pointer },
            )
            .with_extra_ffi_parameter(quote! { usize }, quote! { #length })),
        }
    }

    fn foreign_scalar_option_tokens(
        &self,
        primitive: Primitive,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        let ident = RustIdent::new(self.source.name.spelling())?;
        rust_api::Parameter::new(self.source).scalar_option(primitive)?;
        S::foreign_scalar_option_parameter_tokens(primitive, ident.as_ident())
    }

    fn foreign_direct_vec_tokens(
        &self,
        element: &DirectVectorElementType,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        let ident = RustIdent::new(self.source.name.spelling())?;
        let parameter_names = wrapper::names::Parameter::new(ident.as_ident());
        let pointer = parameter_names.pointer();
        let length = parameter_names.length();
        match element {
            DirectVectorElementType::Primitive(primitive) => {
                let element_type = wrapper::type_ref::Renderer.primitive(primitive.primitive())?;
                Ok(ForeignMethodParameterTokens::new(
                    quote! { *const #element_type },
                    vec![quote! {
                        let #pointer = #ident.as_ptr();
                        let #length = #ident.len();
                    }],
                    quote! { #pointer },
                )
                .with_extra_ffi_parameter(quote! { usize }, quote! { #length }))
            }
            DirectVectorElementType::Record(_) => {
                let rust_element =
                    rust_api::Parameter::new(self.source).direct_vec_element_type()?;
                let buffer = parameter_names.buffer();
                Ok(ForeignMethodParameterTokens::new(
                    quote! { *const u8 },
                    vec![quote! {
                        let #buffer =
                            <#rust_element as ::boltffi::__private::VecTransport>::pack_vec(#ident);
                        let #pointer = #buffer.as_ptr();
                        let #length = #buffer.len();
                    }],
                    quote! { #pointer },
                )
                .with_extra_ffi_parameter(quote! { usize }, quote! { #length }))
            }
            _ => Err(Error::UnsupportedExpansion(
                "callback method direct-vector element",
            )),
        }
    }
}

struct ForeignMethodParameterTokens {
    items: Vec<TokenStream>,
    ffi_types: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl ForeignMethodParameterTokens {
    fn new(ffi_type: TokenStream, setup: Vec<TokenStream>, argument: TokenStream) -> Self {
        Self {
            items: Vec::new(),
            ffi_types: vec![ffi_type],
            setup,
            arguments: vec![argument],
        }
    }

    fn with_items(mut self, items: Vec<TokenStream>) -> Self {
        self.items = items;
        self
    }

    fn with_extra_ffi_parameter(mut self, ffi_type: TokenStream, argument: TokenStream) -> Self {
        self.ffi_types.push(ffi_type);
        self.arguments.push(argument);
        self
    }
}

struct NativeOutgoingClosure<'lowered> {
    closure: &'lowered ClosureParameter<Native, OutOfRust>,
    source: &'lowered ParameterDef,
    expansion: &'lowered Expansion<'lowered, Native>,
}

impl<'lowered> NativeOutgoingClosure<'lowered> {
    fn new(
        closure: &'lowered ClosureParameter<Native, OutOfRust>,
        source: &'lowered ParameterDef,
        expansion: &'lowered Expansion<'lowered, Native>,
    ) -> Self {
        Self {
            closure,
            source,
            expansion,
        }
    }

    fn tokens(self) -> Result<ForeignMethodParameterTokens, Error> {
        match self.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => {}
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown native outgoing closure registration",
                ));
            }
        }
        let source = rust_api::Closure::new(&self.source.type_expr, self.closure.presence())?;
        let closure = RustOwnedClosure::new(&source, self.closure.form())?;
        let ident = RustIdent::new(self.source.name.spelling())?.into_ident();
        let registration_names = wrapper::names::NativeClosureRegistration::new(&ident);
        let call = registration_names.call();
        let context = registration_names.context();
        let release = registration_names.release();
        let invoke = wrapper::closure::Invoke::<Native>::new(
            self.closure.invoke(),
            source.signature(),
            &closure.signature,
            self.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let parameters = invoke.parameters(&failure)?;
        let ffi_parameter_types = parameters
            .ffi_parameter_types()
            .iter()
            .cloned()
            .chain(return_tokens.ffi_parameter_types())
            .collect::<Vec<_>>();
        let ffi_parameters = parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_tokens.ffi_parameters())
            .collect::<Vec<_>>();
        let conversions = parameters.conversions();
        let arguments = parameters.arguments();
        let return_type = return_tokens.return_type();
        let context_type = closure.context_type();
        let context_value = closure.context_value(&ident)?;
        let context_binding = closure.context_binding(quote! {
            __boltffi_context as *mut #context_type
        });
        let call_body = return_tokens.body(quote! {
            __boltffi_closure(#(#arguments),*)
        });
        let function_pointer_type = quote! {
            extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type
        };
        let release_type = quote! {
            unsafe extern "C" fn(*mut ::core::ffi::c_void)
        };
        Ok(ForeignMethodParameterTokens::new(
            function_pointer_type.clone(),
            vec![quote! {
                extern "C" fn #call(
                    __boltffi_context: *mut ::core::ffi::c_void,
                    #(#ffi_parameters),*
                ) #return_type {
                    let mut __boltffi_closure = unsafe { #context_binding };
                    #(#conversions)*
                    #call_body
                }

                unsafe extern "C" fn #release(__boltffi_context: *mut ::core::ffi::c_void) {
                    if !__boltffi_context.is_null() {
                        unsafe {
                            drop(Box::from_raw(__boltffi_context as *mut #context_type));
                        }
                    }
                }

                let #context = Box::into_raw(Box::new(#context_value)) as *mut ::core::ffi::c_void;
            }],
            quote! { #call },
        )
        .with_extra_ffi_parameter(quote! { *mut ::core::ffi::c_void }, quote! { #context })
        .with_extra_ffi_parameter(release_type, quote! { #release }))
    }
}

struct WasmOutgoingClosure<'lowered> {
    closure: &'lowered ClosureParameter<Wasm32, OutOfRust>,
    source: &'lowered ParameterDef,
    expansion: &'lowered Expansion<'lowered, Wasm32>,
}

impl<'lowered> WasmOutgoingClosure<'lowered> {
    fn new(
        closure: &'lowered ClosureParameter<Wasm32, OutOfRust>,
        source: &'lowered ParameterDef,
        expansion: &'lowered Expansion<'lowered, Wasm32>,
    ) -> Self {
        Self {
            closure,
            source,
            expansion,
        }
    }

    fn tokens(self) -> Result<ForeignMethodParameterTokens, Error> {
        let source = rust_api::Closure::new(&self.source.type_expr, self.closure.presence())?;
        let closure = RustOwnedClosure::new(&source, self.closure.form())?;
        let ident = RustIdent::new(self.source.name.spelling())?.into_ident();
        let handle = wrapper::names::Parameter::new(&ident).handle();
        let registration = self.closure.registration().shape();
        let call = RustIdent::new(registration.call().name().as_str())?.into_ident();
        let release = RustIdent::new(registration.free().name().as_str())?.into_ident();
        let invoke = wrapper::closure::Invoke::<Wasm32>::new(
            self.closure.invoke(),
            source.signature(),
            &closure.signature,
            self.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let parameters = invoke.parameters(&failure)?;
        let ffi_parameters = parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_tokens.ffi_parameters())
            .collect::<Vec<_>>();
        let conversions = parameters.conversions();
        let arguments = parameters.arguments();
        let return_type = return_tokens.return_type();
        let context_type = closure.context_type();
        let context_value = closure.context_value(&ident)?;
        let context_binding = closure.context_binding(quote! {
            __boltffi_context as usize as *mut #context_type
        });
        let call_body = return_tokens.body(quote! {
            __boltffi_closure(#(#arguments),*)
        });
        Ok(ForeignMethodParameterTokens::new(
            quote! { u32 },
            vec![quote! {
                let #handle = Box::into_raw(Box::new(#context_value)) as usize as u32;
            }],
            quote! { #handle },
        )
        .with_items(vec![quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #call(
                __boltffi_context: u32,
                #(#ffi_parameters),*
            ) #return_type {
                let mut __boltffi_closure = unsafe { #context_binding };
                #(#conversions)*
                #call_body
            }

            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #release(__boltffi_context: u32) {
                if __boltffi_context != 0 {
                    unsafe {
                        drop(Box::from_raw(__boltffi_context as usize as *mut #context_type));
                    }
                }
            }
        }]))
    }
}

struct RustOwnedClosure {
    source: rust_api::ClosureSourceForm,
    form: ClosureForm,
    signature: wrapper::closure::Signature,
}

impl RustOwnedClosure {
    fn new(source: &rust_api::Closure, form: ClosureForm) -> Result<Self, Error> {
        if source.function() != form {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter form does not match binding closure",
            ));
        }
        Ok(Self {
            source: source.form(),
            form,
            signature: wrapper::closure::Signature::from_source(source.signature())?,
        })
    }

    fn context_type(&self) -> TokenStream {
        let object = self.trait_object();
        match self.form {
            ClosureForm::Fn | ClosureForm::FnMut | ClosureForm::FunctionPointer => object,
            ClosureForm::FnOnce => quote! { Option<#object> },
            _ => object,
        }
    }

    fn context_value(&self, value: &Ident) -> Result<TokenStream, Error> {
        let object = self.trait_object();
        match (self.source, self.form) {
            (
                rust_api::ClosureSourceForm::ImplTrait
                | rust_api::ClosureSourceForm::FunctionPointer,
                ClosureForm::Fn | ClosureForm::FnMut | ClosureForm::FunctionPointer,
            ) => Ok(quote! { Box::new(#value) as #object }),
            (rust_api::ClosureSourceForm::ImplTrait, ClosureForm::FnOnce) => {
                Ok(quote! { Some(Box::new(#value) as #object) })
            }
            (rust_api::ClosureSourceForm::BoxedDyn, ClosureForm::Fn | ClosureForm::FnMut) => {
                Ok(quote! { #value })
            }
            (rust_api::ClosureSourceForm::BoxedDyn, ClosureForm::FnOnce) => {
                Ok(quote! { Some(#value) })
            }
            _ => Err(Error::UnsupportedExpansion(
                "outgoing closure parameter form",
            )),
        }
    }

    fn context_binding(&self, context: TokenStream) -> TokenStream {
        match self.form {
            ClosureForm::Fn | ClosureForm::FunctionPointer => quote! { &*(#context) },
            ClosureForm::FnMut => quote! { &mut *(#context) },
            ClosureForm::FnOnce => quote! {
                (&mut *(#context)).take().expect("closure already invoked")
            },
            _ => quote! { &*(#context) },
        }
    }

    fn trait_object(&self) -> TokenStream {
        let trait_ident = self.form.trait_ident();
        let parameters = self.signature.parameters();
        let return_type = self.signature.return_tokens();
        quote! { Box<dyn #trait_ident(#(#parameters),*) #return_type + 'static> }
    }
}

trait ClosureFormToken {
    fn trait_ident(self) -> Ident;
}

impl ClosureFormToken for ClosureForm {
    fn trait_ident(self) -> Ident {
        match self {
            ClosureForm::Fn | ClosureForm::FunctionPointer => format_ident!("Fn"),
            ClosureForm::FnMut => format_ident!("FnMut"),
            ClosureForm::FnOnce => format_ident!("FnOnce"),
            _ => format_ident!("Fn"),
        }
    }
}

#[derive(Default)]
struct LocalMethodParameterTokens {
    ffi_parameters: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

struct MethodParameterTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_types: Vec<TokenStream>,
    foreign_setup: Vec<TokenStream>,
    foreign_arguments: Vec<TokenStream>,
    local_setup: Vec<TokenStream>,
    local_arguments: Vec<TokenStream>,
}

impl MethodParameterTokens {
    fn new(
        ffi_parameter: TokenStream,
        ffi_type: TokenStream,
        foreign_setup: Vec<TokenStream>,
        foreign_argument: TokenStream,
        local_setup: Vec<TokenStream>,
        local_argument: TokenStream,
    ) -> Self {
        Self {
            items: Vec::new(),
            ffi_parameters: vec![ffi_parameter],
            ffi_types: vec![ffi_type],
            foreign_setup,
            foreign_arguments: vec![foreign_argument],
            local_setup,
            local_arguments: vec![local_argument],
        }
    }

    fn with_extra_ffi_parameter(
        mut self,
        ffi_parameter: TokenStream,
        ffi_type: TokenStream,
        argument: TokenStream,
    ) -> Self {
        self.ffi_parameters.push(ffi_parameter);
        self.ffi_types.push(ffi_type);
        self.foreign_arguments.push(argument);
        self
    }
}

struct MethodParameters {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_types: Vec<TokenStream>,
    foreign_setup: Vec<TokenStream>,
    foreign_arguments: Vec<TokenStream>,
    local_setup: Vec<TokenStream>,
    local_arguments: Vec<TokenStream>,
}

impl From<Vec<MethodParameterTokens>> for MethodParameters {
    fn from(parameters: Vec<MethodParameterTokens>) -> Self {
        Self {
            items: parameters
                .iter()
                .flat_map(|parameter| parameter.items.iter().cloned())
                .collect(),
            ffi_parameters: parameters
                .iter()
                .flat_map(|parameter| parameter.ffi_parameters.iter().cloned())
                .collect(),
            ffi_types: parameters
                .iter()
                .flat_map(|parameter| parameter.ffi_types.iter().cloned())
                .collect(),
            foreign_setup: parameters
                .iter()
                .flat_map(|parameter| parameter.foreign_setup.iter().cloned())
                .collect(),
            foreign_arguments: parameters
                .iter()
                .flat_map(|parameter| parameter.foreign_arguments.iter().cloned())
                .collect(),
            local_setup: parameters
                .iter()
                .flat_map(|parameter| parameter.local_setup.iter().cloned())
                .collect(),
            local_arguments: parameters
                .iter()
                .flat_map(|parameter| parameter.local_arguments.iter().cloned())
                .collect(),
        }
    }
}

impl MethodParameters {
    fn wasm_import_parameters(&self) -> Vec<TokenStream> {
        self.ffi_types
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let ident = wrapper::names::ClosureArgument::new(index).value();
                quote! { #ident: #parameter }
            })
            .collect()
    }
}

struct MethodReturn<'expansion, 'lowered, S: CallbackMethodSurface> {
    plan: &'lowered ReturnPlan<S, IntoRust>,
    error: &'lowered ErrorDecl<S, IntoRust>,
    local_plan: Option<&'lowered ReturnPlan<S, OutOfRust>>,
    local_error: Option<&'lowered ErrorDecl<S, OutOfRust>>,
    source: rust_api::Return<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
    foreign_ffi_parameters: Vec<TokenStream>,
    foreign_arguments: Vec<TokenStream>,
    foreign_return_type: TokenStream,
    local_ffi_parameters: Vec<TokenStream>,
    local_arguments: Vec<TokenStream>,
    local_return_type: TokenStream,
}

struct LocalReturn<'lowered, S: CallbackMethodSurface> {
    plan: &'lowered ReturnPlan<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
}

enum InfallibleMethodReturn<'lowered, S: CallbackMethodSurface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    Void,
    Direct {
        ty: &'lowered DirectValueType,
        slot: ReturnValueSlot,
    },
    Encoded {
        codec: &'lowered D::Codec,
        shape: S::BufferShape,
        ty: &'lowered TypeRef,
    },
    Handle {
        target: &'lowered HandleTarget,
        carrier: S::HandleCarrier,
        presence: HandlePresence,
    },
    ScalarOption {
        primitive: Primitive,
    },
    DirectVec,
    Closure {
        closure: &'lowered ClosureReturn<S, D>,
    },
}

enum FallibleMethodSuccess<'lowered, S: CallbackMethodSurface, D: Direction>
where
    D::Opposite: ParamDirection<S>,
{
    Void,
    Direct {
        ty: &'lowered DirectValueType,
    },
    Encoded {
        codec: &'lowered D::Codec,
        shape: S::BufferShape,
    },
    Handle {
        target: &'lowered HandleTarget,
        carrier: S::HandleCarrier,
        presence: HandlePresence,
    },
    Closure {
        closure: &'lowered ClosureReturn<S, D>,
    },
}

impl<'lowered, S, D> InfallibleMethodReturn<'lowered, S, D>
where
    S: CallbackMethodSurface,
    D: Direction,
    D::Opposite: ParamDirection<S>,
{
    fn new(
        plan: &'lowered ReturnPlan<S, D>,
        error: &'lowered ErrorDecl<S, D>,
    ) -> Result<Self, Error> {
        match error {
            ErrorDecl::None(_) => Self::from_plan(plan),
            ErrorDecl::EncodedViaReturnSlot { .. } => Err(Error::UnsupportedExpansion(
                "fallible callback method return",
            )),
            _ => Err(Error::UnsupportedExpansion("callback method error shape")),
        }
    }

    fn from_plan(plan: &'lowered ReturnPlan<S, D>) -> Result<Self, Error> {
        match plan {
            ReturnPlan::Void => Ok(Self::Void),
            ReturnPlan::DirectViaReturnSlot { ty } => Ok(Self::Direct {
                ty,
                slot: ReturnValueSlot::ReturnSlot,
            }),
            ReturnPlan::DirectViaOutPointer { ty } => Ok(Self::Direct {
                ty,
                slot: ReturnValueSlot::OutPointer,
            }),
            ReturnPlan::EncodedViaReturnSlot { codec, shape, ty } => Ok(Self::Encoded {
                codec,
                shape: *shape,
                ty,
            }),
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => Ok(Self::Handle {
                target,
                carrier: *carrier,
                presence: *presence,
            }),
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => Ok(Self::ScalarOption {
                primitive: *primitive,
            }),
            ReturnPlan::DirectVecViaReturnSlot { .. } => Ok(Self::DirectVec),
            ReturnPlan::ClosureViaOutPointer(closure) => Ok(Self::Closure { closure }),
            _ => Err(Error::UnsupportedExpansion("callback method return shape")),
        }
    }
}

impl<'lowered, S, D> FallibleMethodSuccess<'lowered, S, D>
where
    S: CallbackMethodSurface,
    D: Direction,
    D::Opposite: ParamDirection<S>,
{
    fn new(plan: &'lowered ReturnPlan<S, D>) -> Result<Self, Error> {
        match plan {
            ReturnPlan::Void => Ok(Self::Void),
            ReturnPlan::DirectViaOutPointer { ty } => Ok(Self::Direct { ty }),
            ReturnPlan::EncodedViaOutPointer { codec, shape, .. } => Ok(Self::Encoded {
                codec,
                shape: *shape,
            }),
            ReturnPlan::HandleViaOutPointer {
                target,
                carrier,
                presence,
            } => Ok(Self::Handle {
                target,
                carrier: *carrier,
                presence: *presence,
            }),
            ReturnPlan::ClosureViaOutPointer(closure) => Ok(Self::Closure { closure }),
            _ => Err(Error::UnsupportedExpansion(
                "fallible callback success shape",
            )),
        }
    }
}

impl<'expansion, 'lowered, S> MethodReturn<'expansion, 'lowered, S>
where
    S: CallbackMethodSurface,
    wrapper::returns::direct_vec::Incoming:
        Render<S, wrapper::returns::direct_vec::IncomingInput, Output = TokenStream>,
    wrapper::returns::direct_vec::Renderer:
        Render<S, wrapper::returns::direct_vec::Input, Output = wrapper::returns::Tokens>,
    wrapper::returns::scalar_option::Incoming:
        Render<S, wrapper::returns::scalar_option::IncomingInput, Output = TokenStream>,
    wrapper::returns::scalar_option::Renderer:
        Render<S, wrapper::returns::scalar_option::Input, Output = wrapper::returns::Tokens>,
    wrapper::returns::closure::Write: Render<
            S,
            wrapper::returns::closure::WriteInput<'expansion, 'lowered, S>,
            Output = wrapper::returns::closure::WriteTokens,
        >,
{
    fn new(
        plan: &'lowered ReturnPlan<S, IntoRust>,
        error: &'lowered ErrorDecl<S, IntoRust>,
        local_callable: Option<&'lowered ExportedCallable<S>>,
        source: rust_api::Return<'lowered>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        let foreign_abi = Self::abi(plan, error, source)?;
        let local_plan = local_callable.map(|callable| callable.returns().plan());
        let local_error = local_callable.map(ExportedCallable::error);
        let local_abi = match (local_plan, local_error) {
            (Some(plan), Some(error)) => Self::abi(plan, error, source)?,
            (None, None) => CallbackReturnAbi::empty(),
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "callback local return is incomplete",
                ));
            }
        };
        Ok(Self {
            plan,
            error,
            local_plan,
            local_error,
            source,
            expansion,
            foreign_ffi_parameters: foreign_abi.foreign_ffi_parameters,
            foreign_arguments: foreign_abi.foreign_arguments,
            foreign_return_type: foreign_abi.foreign_return_type,
            local_ffi_parameters: local_abi.local_ffi_parameters,
            local_arguments: local_abi.local_arguments,
            local_return_type: local_abi.local_return_type,
        })
    }

    fn abi<D>(
        plan: &'lowered ReturnPlan<S, D>,
        error: &'lowered ErrorDecl<S, D>,
        source: rust_api::Return<'lowered>,
    ) -> Result<CallbackReturnAbi, Error>
    where
        D: Direction,
        D::Opposite: ParamDirection<S>,
    {
        match error {
            ErrorDecl::None(_) => match InfallibleMethodReturn::from_plan(plan)? {
                InfallibleMethodReturn::Void => Ok(CallbackReturnAbi::empty()),
                InfallibleMethodReturn::Direct { ty, slot } => match slot {
                    ReturnValueSlot::ReturnSlot => Ok(CallbackReturnAbi::direct(
                        Self::direct_return_type(ty, source)?,
                    )),
                    ReturnValueSlot::OutPointer => {
                        Ok(CallbackReturnAbi::direct_out(Self::source_type(source)?))
                    }
                    _ => Err(Error::UnsupportedExpansion(
                        "unknown callback direct return slot",
                    )),
                },
                InfallibleMethodReturn::Encoded { shape, ty, .. } => {
                    Ok(S::callback_encoded_return(shape, ty)?.abi())
                }
                InfallibleMethodReturn::Handle {
                    carrier,
                    target,
                    presence,
                } => Self::handle_abi(target, carrier, presence),
                InfallibleMethodReturn::ScalarOption { primitive } => {
                    source.scalar_option(primitive)?;
                    let result = wrapper::names::Locals::new(Span::call_site()).result();
                    let optional =
                        <wrapper::returns::scalar_option::Renderer as Render<S, _>>::render(
                            wrapper::returns::scalar_option::Renderer,
                            wrapper::returns::scalar_option::Input::new(primitive, result),
                        )?;
                    Ok(CallbackReturnAbi::direct(optional.return_type().clone()))
                }
                InfallibleMethodReturn::DirectVec => {
                    let element = source.direct_vec_element_type()?;
                    let result = wrapper::names::Locals::new(Span::call_site()).result();
                    let sequence =
                        <wrapper::returns::direct_vec::Renderer as Render<S, _>>::render(
                            wrapper::returns::direct_vec::Renderer,
                            wrapper::returns::direct_vec::Input::new(result, element),
                        )?;
                    Ok(CallbackReturnAbi::direct(sequence.return_type().clone()))
                }
                InfallibleMethodReturn::Closure { .. } => Ok(S::callback_closure_return_abi()),
            },
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => {
                let error_slot = S::callback_encoded_error(*shape)?;
                Self::fallible_abi(plan, source.fallible()?, error_slot.return_type())
            }
            _ => Err(Error::UnsupportedExpansion("callback method error shape")),
        }
    }

    fn foreign_ffi_parameters(&self) -> &[TokenStream] {
        &self.foreign_ffi_parameters
    }

    fn foreign_arguments(&self) -> &[TokenStream] {
        &self.foreign_arguments
    }

    fn foreign_return_type(&self) -> &TokenStream {
        &self.foreign_return_type
    }

    fn local_ffi_parameters(&self) -> &[TokenStream] {
        &self.local_ffi_parameters
    }

    fn local_arguments(&self) -> &[TokenStream] {
        &self.local_arguments
    }

    fn local_return_type(&self) -> &TokenStream {
        &self.local_return_type
    }

    fn native_async_completion_type(&self) -> Result<TokenStream, Error> {
        if let ErrorDecl::EncodedViaReturnSlot { .. } = self.error {
            FallibleMethodSuccess::new(self.plan)?;
            return Ok(
                quote! { ::boltffi::__private::AsyncCallback<::boltffi::__private::FfiBuf> },
            );
        }

        match InfallibleMethodReturn::new(self.plan, self.error)? {
            InfallibleMethodReturn::Void => Ok(quote! { ::boltffi::__private::AsyncCallbackVoid }),
            InfallibleMethodReturn::Encoded { .. }
            | InfallibleMethodReturn::ScalarOption { .. }
            | InfallibleMethodReturn::DirectVec => {
                Ok(quote! { ::boltffi::__private::AsyncCallback<::boltffi::__private::FfiBuf> })
            }
            InfallibleMethodReturn::Direct {
                ty: DirectValueType::Primitive(primitive),
                ..
            } => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(quote! { ::boltffi::__private::AsyncCallback<#ffi_type> })
            }
            InfallibleMethodReturn::Direct { .. } => {
                let rust_type = self.direct_source_type()?;
                Ok(quote! {
                    ::boltffi::__private::AsyncCallback<<#rust_type as ::boltffi::__private::Passable>::In>
                })
            }
            InfallibleMethodReturn::Handle { carrier, .. } => {
                let carrier = S::handle_carrier(carrier)?;
                let ty = carrier.ty();
                Ok(quote! { ::boltffi::__private::AsyncCallback<#ty> })
            }
            InfallibleMethodReturn::Closure { .. } => {
                Err(Error::UnsupportedExpansion("async callback closure return"))
            }
        }
    }

    fn native_async_foreign_body(&self, call: TokenStream) -> Result<TokenStream, Error> {
        if let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = self.error {
            return self.native_async_fallible_body(call, codec.root(), *shape);
        }

        match InfallibleMethodReturn::new(self.plan, self.error)? {
            InfallibleMethodReturn::Void => Ok(self.native_async_void_body(call)),
            InfallibleMethodReturn::Encoded { codec, .. } => {
                let value = self.native_async_encoded_value(
                    codec.root(),
                    quote! { __boltffi_result.as_slice() },
                )?;
                Ok(self.native_async_bytes_body(call, value))
            }
            InfallibleMethodReturn::Direct {
                ty: DirectValueType::Primitive(primitive),
                ..
            } => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(self.native_async_value_body(ffi_type, call, quote! { __boltffi_result }))
            }
            InfallibleMethodReturn::Direct { .. } => {
                let rust_type = self.direct_source_type()?;
                Ok(self.native_async_value_body(
                    quote! { <#rust_type as ::boltffi::__private::Passable>::In },
                    call,
                    quote! {
                        unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(__boltffi_result)
                        }
                    },
                ))
            }
            InfallibleMethodReturn::Handle {
                target,
                carrier,
                presence,
            } => {
                let carrier_tokens = S::handle_carrier(carrier)?;
                let carrier_type = carrier_tokens.ty();
                let value = self.foreign_handle_value(
                    target,
                    carrier,
                    presence,
                    quote! { __boltffi_result },
                )?;
                Ok(self.native_async_value_body(carrier_type.clone(), call, value))
            }
            InfallibleMethodReturn::ScalarOption { primitive } => {
                self.source.scalar_option(primitive)?;
                let rust_type = self.direct_source_type()?;
                let value = <wrapper::returns::scalar_option::Incoming as Render<S, _>>::render(
                    wrapper::returns::scalar_option::Incoming,
                    wrapper::returns::scalar_option::IncomingInput::new(
                        primitive,
                        rust_type,
                        quote! {
                            ::boltffi::__private::FfiBuf::from_vec(__boltffi_result)
                        },
                    ),
                )?;
                Ok(self.native_async_bytes_body(call, value))
            }
            InfallibleMethodReturn::DirectVec => {
                let element = self.source.direct_vec_element_type()?;
                let value = <wrapper::returns::direct_vec::Incoming as Render<S, _>>::render(
                    wrapper::returns::direct_vec::Incoming,
                    wrapper::returns::direct_vec::IncomingInput::new(
                        element,
                        quote! {
                            ::boltffi::__private::FfiBuf::from_vec(__boltffi_result)
                        },
                    ),
                )?;
                Ok(self.native_async_bytes_body(call, value))
            }
            InfallibleMethodReturn::Closure { .. } => {
                Err(Error::UnsupportedExpansion("async callback closure return"))
            }
        }
    }

    fn wasm_async_foreign_body(&self, call: TokenStream) -> Result<TokenStream, Error> {
        if let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = self.error {
            return self.wasm_async_fallible_body(call, codec.root(), *shape);
        }

        match InfallibleMethodReturn::new(self.plan, self.error)? {
            InfallibleMethodReturn::Void => Ok(self.wasm_async_void_body(call)),
            InfallibleMethodReturn::Encoded { codec, .. } => {
                let value = self.wasm_async_encoded_value(codec.root())?;
                Ok(self.wasm_async_value_body(call, value))
            }
            InfallibleMethodReturn::Direct {
                ty: DirectValueType::Primitive(primitive),
                ..
            } => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                let value = Self::wasm_completion_value(ffi_type);
                Ok(self.wasm_async_value_body(call, value))
            }
            InfallibleMethodReturn::Direct { .. } => {
                let rust_type = self.direct_source_type()?;
                let value = Self::wasm_completion_value(quote! {
                    <#rust_type as ::boltffi::__private::Passable>::In
                });
                Ok(self.wasm_async_value_body(
                    call,
                    quote! {
                        unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(#value)
                        }
                    },
                ))
            }
            InfallibleMethodReturn::Handle {
                target,
                carrier,
                presence,
            } => {
                let carrier_tokens = S::handle_carrier(carrier)?;
                let carrier_type = carrier_tokens.ty();
                let value = self.foreign_handle_value(
                    target,
                    carrier,
                    presence,
                    Self::wasm_completion_value(carrier_type.clone()),
                )?;
                Ok(self.wasm_async_value_body(call, value))
            }
            InfallibleMethodReturn::ScalarOption { primitive } => {
                let value = self.wasm_async_scalar_option_value(primitive)?;
                Ok(self.wasm_async_value_body(call, value))
            }
            InfallibleMethodReturn::DirectVec => {
                let value = self.wasm_async_direct_vec_value()?;
                Ok(self.wasm_async_value_body(call, value))
            }
            InfallibleMethodReturn::Closure { .. } => {
                Err(Error::UnsupportedExpansion("async callback closure return"))
            }
        }
    }

    fn wasm_completion_value(ty: TokenStream) -> TokenStream {
        quote! {
            match ::boltffi::__private::wire::decode::<#ty>(
                __boltffi_completion.data.as_slice()
            ) {
                Ok(__boltffi_value) => __boltffi_value,
                Err(error) => {
                    panic!(
                        "async callback return conversion failed: {:?}",
                        error
                    )
                }
            }
        }
    }

    fn native_async_encoded_value(
        &self,
        codec: &CodecNode,
        bytes: TokenStream,
    ) -> Result<TokenStream, Error> {
        let source = self.source.value_type()?;
        let rust_type = rust_api::TypeTokens::new(source.as_ref())?.into_type();
        wrapper::encoded::incoming::Value::new(codec, self.expansion).expression(
            wrapper::encoded::incoming::Bytes::new(
                &rust_type,
                source.as_ref(),
                bytes,
                quote! {
                    panic!("async callback return conversion failed: {:?}", error)
                },
            ),
        )
    }

    fn wasm_async_encoded_value(&self, codec: &CodecNode) -> Result<TokenStream, Error> {
        let source = self.source.value_type()?;
        let rust_type = rust_api::TypeTokens::new(source.as_ref())?.into_type();
        wrapper::encoded::incoming::Value::new(codec, self.expansion).expression(
            wrapper::encoded::incoming::Bytes::new(
                &rust_type,
                source.as_ref(),
                quote! { __boltffi_completion.data.as_slice() },
                quote! {
                    panic!("async callback return conversion failed: {:?}", error)
                },
            ),
        )
    }

    fn wasm_async_scalar_option_value(&self, primitive: Primitive) -> Result<TokenStream, Error> {
        self.source.scalar_option(primitive)?;
        let rust_type = self.direct_source_type()?;
        Ok(quote! {
            match ::boltffi::__private::wire::decode::<#rust_type>(
                __boltffi_completion.data.as_slice()
            ) {
                Ok(__boltffi_value) => __boltffi_value,
                Err(error) => {
                    panic!(
                        "async callback optional scalar return conversion failed: {:?}",
                        error
                    )
                }
            }
        })
    }

    fn wasm_async_direct_vec_value(&self) -> Result<TokenStream, Error> {
        let element = self.source.direct_vec_element_type()?;
        Ok(quote! {
            if __boltffi_completion.data.is_empty() {
                Vec::new()
            } else {
                let __boltffi_byte_len = __boltffi_completion.data.len();
                let __boltffi_element_size =
                    ::core::mem::size_of::<<#element as ::boltffi::__private::Passable>::In>();
                if __boltffi_byte_len % __boltffi_element_size == 0 {
                    unsafe {
                        <#element as ::boltffi::__private::VecTransport>::unpack_vec(
                            __boltffi_completion.data.as_ptr(),
                            __boltffi_byte_len,
                        )
                    }
                } else {
                    panic!(
                        "invalid async callback Vec<{}> return byte length {} for element size {}",
                        ::core::any::type_name::<#element>(),
                        __boltffi_byte_len,
                        __boltffi_element_size,
                    )
                }
            }
        })
    }

    fn async_fallible_success_value(&self, bytes: TokenStream) -> Result<TokenStream, Error> {
        match FallibleMethodSuccess::new(self.plan)? {
            FallibleMethodSuccess::Void => Ok(quote! { () }),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(Self::async_wire_value(
                    ffi_type,
                    bytes,
                    "async callback success conversion failed",
                ))
            }
            FallibleMethodSuccess::Direct { .. } => {
                let ok_type = self.source.fallible()?.ok_written_type()?;
                let passable = Self::async_wire_value(
                    quote! { <#ok_type as ::boltffi::__private::Passable>::In },
                    bytes,
                    "async callback success conversion failed",
                );
                Ok(quote! {
                    unsafe {
                        <#ok_type as ::boltffi::__private::Passable>::unpack(#passable)
                    }
                })
            }
            FallibleMethodSuccess::Encoded { codec, .. } => {
                let fallible = self.source.fallible()?;
                let ok_type = fallible.ok_written_type()?;
                wrapper::encoded::incoming::Value::new(codec.root(), self.expansion).expression(
                    wrapper::encoded::incoming::Bytes::new(
                        &ok_type,
                        fallible.ok(),
                        bytes,
                        quote! {
                            panic!("async callback success conversion failed: {:?}", error)
                        },
                    ),
                )
            }
            FallibleMethodSuccess::Handle {
                target,
                carrier,
                presence,
            } => {
                let carrier_tokens = S::handle_carrier(carrier)?;
                let value = Self::async_wire_value(
                    carrier_tokens.ty().clone(),
                    bytes,
                    "async callback success conversion failed",
                );
                self.foreign_handle_value(target, carrier, presence, value)
            }
            FallibleMethodSuccess::Closure { .. } => Err(Error::UnsupportedExpansion(
                "async callback closure success",
            )),
        }
    }

    fn async_fallible_error_value(
        &self,
        codec: &CodecNode,
        bytes: TokenStream,
    ) -> Result<TokenStream, Error> {
        let fallible = self.source.fallible()?;
        let error_type = fallible.error_written_type()?;
        wrapper::encoded::incoming::Value::new(codec, self.expansion).expression(
            wrapper::encoded::incoming::Bytes::new(
                &error_type,
                fallible.error(),
                bytes,
                quote! {
                    panic!("async callback error conversion failed: {:?}", error)
                },
            ),
        )
    }

    fn async_wire_value(ty: TokenStream, bytes: TokenStream, failure: &'static str) -> TokenStream {
        let failure = LitStr::new(failure, Span::call_site());
        quote! {
            match ::boltffi::__private::wire::decode::<#ty>(#bytes) {
                Ok(__boltffi_value) => __boltffi_value,
                Err(error) => {
                    panic!("{}: {:?}", #failure, error)
                }
            }
        }
    }

    fn native_async_fallible_body(
        &self,
        call: TokenStream,
        error_codec: &CodecNode,
        error_shape: S::BufferShape,
    ) -> Result<TokenStream, Error> {
        S::callback_encoded_error(error_shape)?;
        let success = self.async_fallible_success_value(quote! {
            __boltffi_result.as_slice()
        })?;
        let error =
            self.async_fallible_error_value(error_codec, quote! { __boltffi_result.as_slice() })?;
        Ok(self.native_async_bytes_result_body(call, success, error))
    }

    fn wasm_async_fallible_body(
        &self,
        call: TokenStream,
        error_codec: &CodecNode,
        error_shape: S::BufferShape,
    ) -> Result<TokenStream, Error> {
        S::callback_encoded_error(error_shape)?;
        let success =
            self.async_fallible_success_value(quote! { __boltffi_completion.data.as_slice() })?;
        let error = self.async_fallible_error_value(
            error_codec,
            quote! { __boltffi_completion.data.as_slice() },
        )?;
        Ok(self.wasm_async_result_body(call, success, error))
    }

    fn native_async_void_body(&self, call: TokenStream) -> TokenStream {
        quote! {
            use std::sync::{Arc, Mutex};
            use std::task::Waker;

            struct __BoltffiAsyncState {
                completed: bool,
                status: ::boltffi::__private::FfiStatus,
                waker: Option<Waker>,
            }

            struct __BoltffiAsyncContext {
                state: Mutex<__BoltffiAsyncState>,
            }

            extern "C" fn __boltffi_completion(
                completion_data: *mut ::core::ffi::c_void,
                status: ::boltffi::__private::FfiStatus,
            ) {
                let context =
                    unsafe { Arc::from_raw(completion_data as *const __BoltffiAsyncContext) };
                let waker = context
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| {
                        state.completed = true;
                        state.status = status;
                        state.waker.take()
                    });
                if let Some(waker) = waker {
                    waker.wake();
                }
            }

            let __boltffi_context = Arc::new(__BoltffiAsyncContext {
                state: Mutex::new(__BoltffiAsyncState {
                    completed: false,
                    status: ::boltffi::__private::FfiStatus::OK,
                    waker: None,
                }),
            });
            let __boltffi_completion_data =
                Arc::into_raw(Arc::clone(&__boltffi_context)) as *mut ::core::ffi::c_void;
            unsafe {
                #call;
            }

            std::future::poll_fn(move |task| {
                let mut state = __boltffi_context
                    .state
                    .lock()
                    .expect("async callback mutex poisoned");
                if state.completed {
                    if state.status.is_err() {
                        panic!("async callback failed");
                    }
                    std::task::Poll::Ready(())
                } else {
                    state.waker = Some(task.waker().clone());
                    std::task::Poll::Pending
                }
            }).await
        }
    }

    fn native_async_value_body(
        &self,
        result_type: TokenStream,
        call: TokenStream,
        value: TokenStream,
    ) -> TokenStream {
        quote! {
            use std::sync::{Arc, Mutex};
            use std::task::Waker;

            struct __BoltffiAsyncState {
                result: Option<#result_type>,
                status: ::boltffi::__private::FfiStatus,
                waker: Option<Waker>,
            }

            struct __BoltffiAsyncContext {
                state: Mutex<__BoltffiAsyncState>,
            }

            extern "C" fn __boltffi_completion(
                completion_data: *mut ::core::ffi::c_void,
                status: ::boltffi::__private::FfiStatus,
                result: #result_type,
            ) {
                let context =
                    unsafe { Arc::from_raw(completion_data as *const __BoltffiAsyncContext) };
                let waker = context
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| {
                        state.result = Some(result);
                        state.status = status;
                        state.waker.take()
                    });
                if let Some(waker) = waker {
                    waker.wake();
                }
            }

            let __boltffi_context = Arc::new(__BoltffiAsyncContext {
                state: Mutex::new(__BoltffiAsyncState {
                    result: None,
                    status: ::boltffi::__private::FfiStatus::OK,
                    waker: None,
                }),
            });
            let __boltffi_completion_data =
                Arc::into_raw(Arc::clone(&__boltffi_context)) as *mut ::core::ffi::c_void;
            unsafe {
                #call;
            }

            std::future::poll_fn(move |task| {
                let mut state = __boltffi_context
                    .state
                    .lock()
                    .expect("async callback mutex poisoned");
                if let Some(__boltffi_result) = state.result.take() {
                    if state.status.is_err() {
                        panic!("async callback failed");
                    }
                    std::task::Poll::Ready(#value)
                } else {
                    state.waker = Some(task.waker().clone());
                    std::task::Poll::Pending
                }
            }).await
        }
    }

    fn native_async_bytes_body(&self, call: TokenStream, value: TokenStream) -> TokenStream {
        quote! {
            use std::sync::{Arc, Mutex};
            use std::task::Waker;

            struct __BoltffiAsyncState {
                result: Option<Vec<u8>>,
                status: ::boltffi::__private::FfiStatus,
                waker: Option<Waker>,
            }

            struct __BoltffiAsyncContext {
                state: Mutex<__BoltffiAsyncState>,
            }

            extern "C" fn __boltffi_completion(
                completion_data: *mut ::core::ffi::c_void,
                status: ::boltffi::__private::FfiStatus,
                result: ::boltffi::__private::FfiBuf,
            ) {
                let context =
                    unsafe { Arc::from_raw(completion_data as *const __BoltffiAsyncContext) };
                let bytes = unsafe { result.as_byte_slice() }.to_vec();
                let waker = context
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| {
                        state.result = Some(bytes);
                        state.status = status;
                        state.waker.take()
                    });
                if let Some(waker) = waker {
                    waker.wake();
                }
            }

            let __boltffi_context = Arc::new(__BoltffiAsyncContext {
                state: Mutex::new(__BoltffiAsyncState {
                    result: None,
                    status: ::boltffi::__private::FfiStatus::OK,
                    waker: None,
                }),
            });
            let __boltffi_completion_data =
                Arc::into_raw(Arc::clone(&__boltffi_context)) as *mut ::core::ffi::c_void;
            unsafe {
                #call;
            }

            std::future::poll_fn(move |task| {
                let mut state = __boltffi_context
                    .state
                    .lock()
                    .expect("async callback mutex poisoned");
                if let Some(__boltffi_result) = state.result.take() {
                    if state.status.is_err() {
                        panic!("async callback failed");
                    }
                    std::task::Poll::Ready(#value)
                } else {
                    state.waker = Some(task.waker().clone());
                    std::task::Poll::Pending
                }
            }).await
        }
    }

    fn native_async_result_body(
        &self,
        call: TokenStream,
        success: TokenStream,
        error: TokenStream,
    ) -> TokenStream {
        quote! {
            use std::sync::{Arc, Mutex};
            use std::task::Waker;

            struct __BoltffiAsyncState {
                result: Option<::boltffi::__private::FfiBuf>,
                status: ::boltffi::__private::FfiStatus,
                waker: Option<Waker>,
            }

            struct __BoltffiAsyncContext {
                state: Mutex<__BoltffiAsyncState>,
            }

            extern "C" fn __boltffi_completion(
                completion_data: *mut ::core::ffi::c_void,
                status: ::boltffi::__private::FfiStatus,
                result: ::boltffi::__private::FfiBuf,
            ) {
                let context =
                    unsafe { Arc::from_raw(completion_data as *const __BoltffiAsyncContext) };
                let waker = context
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| {
                        state.result = Some(result);
                        state.status = status;
                        state.waker.take()
                    });
                if let Some(waker) = waker {
                    waker.wake();
                }
            }

            let __boltffi_context = Arc::new(__BoltffiAsyncContext {
                state: Mutex::new(__BoltffiAsyncState {
                    result: None,
                    status: ::boltffi::__private::FfiStatus::OK,
                    waker: None,
                }),
            });
            let __boltffi_completion_data =
                Arc::into_raw(Arc::clone(&__boltffi_context)) as *mut ::core::ffi::c_void;
            unsafe {
                #call;
            }

            std::future::poll_fn(move |task| {
                let mut state = __boltffi_context
                    .state
                    .lock()
                    .expect("async callback mutex poisoned");
                if let Some(__boltffi_result) = state.result.take() {
                    let __boltffi_return = if state.status.is_err() {
                        Err(#error)
                    } else {
                        Ok(#success)
                    };
                    std::task::Poll::Ready(__boltffi_return)
                } else {
                    state.waker = Some(task.waker().clone());
                    std::task::Poll::Pending
                }
            }).await
        }
    }

    fn native_async_bytes_result_body(
        &self,
        call: TokenStream,
        success: TokenStream,
        error: TokenStream,
    ) -> TokenStream {
        quote! {
            use std::sync::{Arc, Mutex};
            use std::task::Waker;

            struct __BoltffiAsyncState {
                result: Option<Vec<u8>>,
                status: ::boltffi::__private::FfiStatus,
                waker: Option<Waker>,
            }

            struct __BoltffiAsyncContext {
                state: Mutex<__BoltffiAsyncState>,
            }

            extern "C" fn __boltffi_completion(
                completion_data: *mut ::core::ffi::c_void,
                status: ::boltffi::__private::FfiStatus,
                result: ::boltffi::__private::FfiBuf,
            ) {
                let context =
                    unsafe { Arc::from_raw(completion_data as *const __BoltffiAsyncContext) };
                let bytes = unsafe { result.as_byte_slice() }.to_vec();
                let waker = context
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| {
                        state.result = Some(bytes);
                        state.status = status;
                        state.waker.take()
                    });
                if let Some(waker) = waker {
                    waker.wake();
                }
            }

            let __boltffi_context = Arc::new(__BoltffiAsyncContext {
                state: Mutex::new(__BoltffiAsyncState {
                    result: None,
                    status: ::boltffi::__private::FfiStatus::OK,
                    waker: None,
                }),
            });
            let __boltffi_completion_data =
                Arc::into_raw(Arc::clone(&__boltffi_context)) as *mut ::core::ffi::c_void;
            unsafe {
                #call;
            }

            std::future::poll_fn(move |task| {
                let mut state = __boltffi_context
                    .state
                    .lock()
                    .expect("async callback mutex poisoned");
                if let Some(__boltffi_result) = state.result.take() {
                    let __boltffi_return = if state.status.is_err() {
                        Err(#error)
                    } else {
                        Ok(#success)
                    };
                    std::task::Poll::Ready(__boltffi_return)
                } else {
                    state.waker = Some(task.waker().clone());
                    std::task::Poll::Pending
                }
            }).await
        }
    }

    fn wasm_async_void_body(&self, call: TokenStream) -> TokenStream {
        quote! {
            let __boltffi_registry = ::boltffi::__private::AsyncCallbackRegistry::current();
            let __boltffi_request = __boltffi_registry.allocate();
            let mut __boltffi_guard = Some(
                ::boltffi::__private::AsyncCallbackRequestGuard::new(__boltffi_request)
            );
            unsafe {
                #call;
            }
            std::future::poll_fn(move |task| {
                __boltffi_registry.set_waker(__boltffi_request, task.waker().clone());
                match __boltffi_registry.take_completion(__boltffi_request) {
                    Some(__boltffi_completion) => {
                        if !__boltffi_completion.code.is_success() {
                            panic!("async callback failed");
                        }
                        drop(__boltffi_guard.take());
                        std::task::Poll::Ready(())
                    }
                    None => std::task::Poll::Pending,
                }
            }).await
        }
    }

    fn wasm_async_result_body(
        &self,
        call: TokenStream,
        success: TokenStream,
        error: TokenStream,
    ) -> TokenStream {
        quote! {
            let __boltffi_registry = ::boltffi::__private::AsyncCallbackRegistry::current();
            let __boltffi_request = __boltffi_registry.allocate();
            let mut __boltffi_guard = Some(
                ::boltffi::__private::AsyncCallbackRequestGuard::new(__boltffi_request)
            );
            unsafe {
                #call;
            }
            std::future::poll_fn(move |task| {
                __boltffi_registry.set_waker(__boltffi_request, task.waker().clone());
                match __boltffi_registry.take_completion(__boltffi_request) {
                    Some(__boltffi_completion) => {
                        let __boltffi_return = if __boltffi_completion.code.is_success() {
                            Ok(#success)
                        } else {
                            Err(#error)
                        };
                        drop(__boltffi_guard.take());
                        std::task::Poll::Ready(__boltffi_return)
                    }
                    None => std::task::Poll::Pending,
                }
            }).await
        }
    }

    fn wasm_async_value_body(&self, call: TokenStream, value: TokenStream) -> TokenStream {
        quote! {
            let __boltffi_registry = ::boltffi::__private::AsyncCallbackRegistry::current();
            let __boltffi_request = __boltffi_registry.allocate();
            let mut __boltffi_guard = Some(
                ::boltffi::__private::AsyncCallbackRequestGuard::new(__boltffi_request)
            );
            unsafe {
                #call;
            }
            std::future::poll_fn(move |task| {
                __boltffi_registry.set_waker(__boltffi_request, task.waker().clone());
                match __boltffi_registry.take_completion(__boltffi_request) {
                    Some(__boltffi_completion) => {
                        if !__boltffi_completion.code.is_success() {
                            panic!("async callback failed");
                        }
                        drop(__boltffi_guard.take());
                        std::task::Poll::Ready(#value)
                    }
                    None => std::task::Poll::Pending,
                }
            }).await
        }
    }

    fn fallible_abi<D>(
        plan: &ReturnPlan<S, D>,
        source: rust_api::Fallible<'lowered>,
        error_return_type: TokenStream,
    ) -> Result<CallbackReturnAbi, Error>
    where
        D: Direction,
        D::Opposite: ParamDirection<S>,
    {
        let success_out = quote! { __boltffi_success_out };
        let success_pointer = match FallibleMethodSuccess::new(plan)? {
            FallibleMethodSuccess::Void => None,
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Some(quote! { *mut #ty })
            }
            FallibleMethodSuccess::Direct { .. } => {
                let ok = source.ok_written_type()?;
                Some(quote! { *mut <#ok as ::boltffi::__private::Passable>::Out })
            }
            FallibleMethodSuccess::Encoded { shape, .. } => {
                Some(S::callback_encoded_out_pointer(shape)?)
            }
            FallibleMethodSuccess::Handle { carrier, .. } => {
                let carrier = S::handle_carrier(carrier)?;
                let ty = carrier.ty();
                Some(quote! { *mut #ty })
            }
            FallibleMethodSuccess::Closure { .. } => Some(S::callback_closure_out_pointer()),
        };
        let foreign_ffi_parameters = success_pointer
            .as_ref()
            .map(|pointer| quote! { #success_out: #pointer })
            .into_iter()
            .collect::<Vec<_>>();
        let foreign_arguments = success_pointer
            .as_ref()
            .map(|_| quote! { __boltffi_success_out.as_mut_ptr() })
            .into_iter()
            .collect::<Vec<_>>();
        let local_arguments = success_pointer
            .as_ref()
            .map(|_| quote! { __boltffi_success_out.as_mut_ptr() })
            .into_iter()
            .collect::<Vec<_>>();
        let local_ffi_parameters = success_pointer
            .map(|pointer| quote! { #success_out: #pointer })
            .into_iter()
            .collect();
        Ok(CallbackReturnAbi {
            foreign_ffi_parameters,
            foreign_arguments,
            foreign_return_type: error_return_type.clone(),
            local_ffi_parameters,
            local_arguments,
            local_return_type: error_return_type,
        })
    }

    fn foreign_body(&self, call: TokenStream) -> Result<TokenStream, Error> {
        if let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = self.error {
            return self.foreign_fallible_body(call, codec.root(), *shape);
        }
        match InfallibleMethodReturn::new(self.plan, self.error)? {
            InfallibleMethodReturn::Void => Ok(quote! {
                unsafe {
                    #call;
                }
            }),
            InfallibleMethodReturn::Direct { ty, slot } => match (ty, slot) {
                (DirectValueType::Primitive(_), ReturnValueSlot::ReturnSlot) => {
                    Ok(quote! { unsafe { #call } })
                }
                (_, ReturnValueSlot::ReturnSlot) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(quote! {
                        unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(#call)
                        }
                    })
                }
                (_, ReturnValueSlot::OutPointer) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(quote! {
                        {
                            let mut __boltffi_return_out = ::core::mem::MaybeUninit::<
                                <#rust_type as ::boltffi::__private::Passable>::Out
                            >::uninit();
                            unsafe {
                                #call;
                                <#rust_type as ::boltffi::__private::Passable>::unpack(
                                    __boltffi_return_out.assume_init()
                                )
                            }
                        }
                    })
                }
                _ => Err(Error::UnsupportedExpansion(
                    "unknown callback direct return slot",
                )),
            },
            InfallibleMethodReturn::Encoded { codec, shape, ty } => {
                let source = self.source.value_type()?;
                let rust_type = rust_api::TypeTokens::new(source.as_ref())?.into_type();
                S::callback_encoded_return(shape, ty)?.foreign_body(
                    call,
                    codec.root(),
                    rust_type,
                    source.as_ref(),
                    self.expansion,
                )
            }
            InfallibleMethodReturn::Handle {
                target,
                carrier,
                presence,
            } => self.foreign_handle_value(target, carrier, presence, quote! { unsafe { #call } }),
            InfallibleMethodReturn::ScalarOption { primitive } => {
                self.source.scalar_option(primitive)?;
                let rust_type = self.direct_source_type()?;
                <wrapper::returns::scalar_option::Incoming as Render<S, _>>::render(
                    wrapper::returns::scalar_option::Incoming,
                    wrapper::returns::scalar_option::IncomingInput::new(
                        primitive,
                        rust_type,
                        quote! { unsafe { #call } },
                    ),
                )
            }
            InfallibleMethodReturn::DirectVec => {
                let element = self.source.direct_vec_element_type()?;
                <wrapper::returns::direct_vec::Incoming as Render<S, _>>::render(
                    wrapper::returns::direct_vec::Incoming,
                    wrapper::returns::direct_vec::IncomingInput::new(
                        element,
                        quote! { unsafe { #call } },
                    ),
                )
            }
            InfallibleMethodReturn::Closure { closure } => S::foreign_closure_return_body(
                closure,
                self.source.closure(closure.presence())?,
                call,
                self.expansion,
            ),
        }
    }

    fn local_proxy_body(&self, call: TokenStream) -> Result<TokenStream, Error> {
        let local_return = self.local_return()?;
        if let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = local_return.error {
            return self.local_proxy_fallible_body(call, codec.root(), *shape);
        }
        match InfallibleMethodReturn::new(local_return.plan, local_return.error)? {
            InfallibleMethodReturn::Void => Ok(quote! {
                #call;
            }),
            InfallibleMethodReturn::Direct { ty, slot } => match (ty, slot) {
                (DirectValueType::Primitive(_), ReturnValueSlot::ReturnSlot) => {
                    Ok(quote! { #call })
                }
                (_, ReturnValueSlot::ReturnSlot) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(quote! {
                        unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(#call)
                        }
                    })
                }
                (_, ReturnValueSlot::OutPointer) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(quote! {
                        {
                            let mut __boltffi_return_out = ::core::mem::MaybeUninit::<
                                <#rust_type as ::boltffi::__private::Passable>::Out
                            >::uninit();
                            #call;
                            unsafe {
                                <#rust_type as ::boltffi::__private::Passable>::unpack(
                                    __boltffi_return_out.assume_init()
                                )
                            }
                        }
                    })
                }
                _ => Err(Error::UnsupportedExpansion(
                    "unknown callback direct return slot",
                )),
            },
            InfallibleMethodReturn::Encoded { codec, shape, ty } => {
                let source = self.source.value_type()?;
                let rust_type = rust_api::TypeTokens::new(source.as_ref())?.into_type();
                S::callback_encoded_return(shape, ty)?.local_proxy_body(
                    call,
                    codec.root(),
                    rust_type,
                    source.as_ref(),
                    self.expansion,
                )
            }
            InfallibleMethodReturn::Handle {
                target,
                carrier,
                presence,
            } => self.foreign_handle_value(target, carrier, presence, quote! { #call }),
            InfallibleMethodReturn::ScalarOption { primitive } => {
                self.source.scalar_option(primitive)?;
                let rust_type = self.direct_source_type()?;
                <wrapper::returns::scalar_option::Incoming as Render<S, _>>::render(
                    wrapper::returns::scalar_option::Incoming,
                    wrapper::returns::scalar_option::IncomingInput::new(
                        primitive,
                        rust_type,
                        quote! { #call },
                    ),
                )
            }
            InfallibleMethodReturn::DirectVec => {
                let element = self.source.direct_vec_element_type()?;
                <wrapper::returns::direct_vec::Incoming as Render<S, _>>::render(
                    wrapper::returns::direct_vec::Incoming,
                    wrapper::returns::direct_vec::IncomingInput::new(element, quote! { #call }),
                )
            }
            InfallibleMethodReturn::Closure { .. } => {
                let closure = self.foreign_closure_return()?;
                S::foreign_closure_return_body(
                    closure,
                    self.source.closure(closure.presence())?,
                    call,
                    self.expansion,
                )
            }
        }
    }

    fn foreign_fallible_body(
        &self,
        call: TokenStream,
        error_codec: &CodecNode,
        error_shape: S::BufferShape,
    ) -> Result<TokenStream, Error> {
        let error_slot = S::callback_encoded_error(error_shape)?;
        let fallible = self.source.fallible()?;
        let error_type = fallible.error_written_type()?;
        let error_value = error_slot.decode(
            quote! { __boltffi_error },
            error_codec,
            error_type,
            fallible.error(),
            self.expansion,
        )?;
        let success = self.foreign_success_value()?;
        let success_storage = self.foreign_success_storage()?;
        let error_empty = error_slot.is_empty(quote! { __boltffi_error });
        Ok(quote! {
            {
                #success_storage
                let __boltffi_error = unsafe { #call };
                if #error_empty {
                    Ok(#success)
                } else {
                    Err(#error_value)
                }
            }
        })
    }

    fn local_proxy_fallible_body(
        &self,
        call: TokenStream,
        error_codec: &CodecNode,
        error_shape: S::BufferShape,
    ) -> Result<TokenStream, Error> {
        let error_slot = S::callback_encoded_error(error_shape)?;
        let fallible = self.source.fallible()?;
        let error_type = fallible.error_written_type()?;
        let error_value = error_slot.decode(
            quote! { __boltffi_error },
            error_codec,
            error_type,
            fallible.error(),
            self.expansion,
        )?;
        let success = self.local_proxy_success_value()?;
        let success_storage = self.local_proxy_success_storage()?;
        let error_empty = error_slot.is_empty(quote! { __boltffi_error });
        Ok(quote! {
            {
                #success_storage
                let __boltffi_error = #call;
                if #error_empty {
                    Ok(#success)
                } else {
                    Err(#error_value)
                }
            }
        })
    }

    fn local_body(&self, owner: Ident, call: TokenStream) -> Result<LocalMethodBody, Error> {
        let local_return = self.local_return()?;
        if let ErrorDecl::EncodedViaReturnSlot { codec, shape, ty } = local_return.error {
            return self.local_fallible_body(owner, call, codec.root(), *shape, ty);
        }
        match InfallibleMethodReturn::new(local_return.plan, local_return.error)? {
            InfallibleMethodReturn::Void => Ok(LocalMethodBody::new(quote! { #call })),
            InfallibleMethodReturn::Direct { ty, slot } => match (ty, slot) {
                (DirectValueType::Primitive(_), ReturnValueSlot::ReturnSlot) => {
                    Ok(LocalMethodBody::new(quote! { #call }))
                }
                (_, ReturnValueSlot::ReturnSlot) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(LocalMethodBody::new(quote! {
                        <#rust_type as ::boltffi::__private::Passable>::pack(#call)
                    }))
                }
                (_, ReturnValueSlot::OutPointer) => {
                    let rust_type = self.direct_source_type()?;
                    Ok(LocalMethodBody::new(quote! {
                        {
                            let __boltffi_result: #rust_type = #call;
                            if !__boltffi_return_out.is_null() {
                                unsafe {
                                    ::core::ptr::write(
                                        __boltffi_return_out,
                                        <#rust_type as ::boltffi::__private::Passable>::pack(
                                            __boltffi_result
                                        ),
                                    );
                                }
                            }
                        }
                    }))
                }
                _ => Err(Error::UnsupportedExpansion(
                    "unknown callback direct return slot",
                )),
            },
            InfallibleMethodReturn::Encoded { codec, shape, ty } => {
                S::callback_encoded_return(shape, ty)?
                    .local_body(call, codec.root(), self.expansion)
                    .map(LocalMethodBody::new)
            }
            InfallibleMethodReturn::Handle {
                target,
                carrier,
                presence,
            } => {
                let result = wrapper::names::Locals::new(Span::call_site()).result();
                let value = self.local_handle_value(target, carrier, presence, result.clone())?;
                Ok(LocalMethodBody::new(quote! {
                    {
                        let #result = #call;
                        #value
                    }
                }))
            }
            InfallibleMethodReturn::ScalarOption { primitive } => {
                self.source.scalar_option(primitive)?;
                let result = wrapper::names::Locals::new(Span::call_site()).result();
                let optional = <wrapper::returns::scalar_option::Renderer as Render<S, _>>::render(
                    wrapper::returns::scalar_option::Renderer,
                    wrapper::returns::scalar_option::Input::new(primitive, result.clone()),
                )?;
                let body = optional.body();
                Ok(LocalMethodBody::new(quote! {
                    {
                        let #result = #call;
                        #body
                    }
                }))
            }
            InfallibleMethodReturn::DirectVec => {
                let element = self.source.direct_vec_element_type()?;
                let result = wrapper::names::Locals::new(Span::call_site()).result();
                let sequence = <wrapper::returns::direct_vec::Renderer as Render<S, _>>::render(
                    wrapper::returns::direct_vec::Renderer,
                    wrapper::returns::direct_vec::Input::new(result.clone(), element),
                )?;
                let body = sequence.body();
                Ok(LocalMethodBody::new(quote! {
                    {
                        let #result = #call;
                        #body
                    }
                }))
            }
            InfallibleMethodReturn::Closure { closure } => {
                let result = wrapper::names::Locals::new(owner.span()).closure();
                let writer = <wrapper::returns::closure::Write as Render<S, _>>::render(
                    wrapper::returns::closure::Write,
                    wrapper::returns::closure::WriteInput::returned(
                        closure,
                        self.source.closure(closure.presence())?,
                        result.clone(),
                        owner,
                        self.expansion,
                    ),
                )?;
                let (items, _ffi_parameters, body) = writer.into_parts();
                Ok(LocalMethodBody {
                    items,
                    body: quote! {
                        let #result = #call;
                        #body
                        ::boltffi::__private::FfiStatus::OK
                    },
                })
            }
        }
    }

    fn local_fallible_body(
        &self,
        owner: Ident,
        call: TokenStream,
        error_codec: &CodecNode,
        error_shape: S::BufferShape,
        _error_ty: &TypeRef,
    ) -> Result<LocalMethodBody, Error> {
        let error_slot = S::callback_encoded_error(error_shape)?;
        let success = self.local_success_write(owner)?;
        let error_value = wrapper::encoded::outgoing::Value::new(error_codec, self.expansion)
            .buffer(quote! { __boltffi_error })?;
        let error_value = error_slot.return_value(error_value);
        let empty_error = error_slot.empty_value();
        let success_body = success.body;
        Ok(LocalMethodBody {
            items: success.items,
            body: quote! {
            match #call {
                Ok(__boltffi_success) => {
                    #success_body
                    #empty_error
                }
                Err(__boltffi_error) => {
                        #error_value
                    }
                }
            },
        })
    }

    fn foreign_success_storage(&self) -> Result<TokenStream, Error> {
        match FallibleMethodSuccess::new(self.plan)? {
            FallibleMethodSuccess::Void => Ok(TokenStream::new()),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Direct { .. } => {
                let ok = self.source.fallible()?.ok_written_type()?;
                Ok(quote! {
                    let mut __boltffi_success_out =
                        ::core::mem::MaybeUninit::<<#ok as ::boltffi::__private::Passable>::Out>::uninit();
                })
            }
            FallibleMethodSuccess::Encoded { shape, .. } => {
                let ty = S::callback_encoded_out_value(shape)?;
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Handle { carrier, .. } => {
                let carrier = S::handle_carrier(carrier)?;
                let ty = carrier.ty();
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Closure { closure } => S::foreign_closure_success_storage(
                closure,
                self.source.fallible()?.ok_closure(closure.presence())?,
                self.expansion,
            ),
        }
    }

    fn foreign_success_value(&self) -> Result<TokenStream, Error> {
        match FallibleMethodSuccess::new(self.plan)? {
            FallibleMethodSuccess::Void => Ok(quote! { () }),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(_),
            } => Ok(quote! { unsafe { __boltffi_success_out.assume_init() } }),
            FallibleMethodSuccess::Direct { .. } => {
                let ok = self.source.fallible()?.ok_written_type()?;
                Ok(quote! {
                    unsafe {
                        <#ok as ::boltffi::__private::Passable>::unpack(
                            __boltffi_success_out.assume_init()
                        )
                    }
                })
            }
            FallibleMethodSuccess::Encoded { codec, shape } => {
                let fallible = self.source.fallible()?;
                let ok = fallible.ok_written_type()?;
                S::decode_callback_encoded_out_pointer(
                    shape,
                    quote! { unsafe { __boltffi_success_out.assume_init() } },
                    codec.root(),
                    ok,
                    fallible.ok(),
                    self.expansion,
                )
            }
            FallibleMethodSuccess::Handle {
                target,
                carrier,
                presence,
            } => self.foreign_handle_value(
                target,
                carrier,
                presence,
                quote! { unsafe { __boltffi_success_out.assume_init() } },
            ),
            FallibleMethodSuccess::Closure { closure } => S::foreign_closure_success_value(
                closure,
                self.source.fallible()?.ok_closure(closure.presence())?,
                self.expansion,
            ),
        }
    }

    fn local_proxy_success_storage(&self) -> Result<TokenStream, Error> {
        let local_return = self.local_return()?;
        match FallibleMethodSuccess::new(local_return.plan)? {
            FallibleMethodSuccess::Void => Ok(TokenStream::new()),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(primitive),
            } => {
                let ty = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Direct { .. } => {
                let ok = self.source.fallible()?.ok_written_type()?;
                Ok(quote! {
                    let mut __boltffi_success_out =
                        ::core::mem::MaybeUninit::<<#ok as ::boltffi::__private::Passable>::Out>::uninit();
                })
            }
            FallibleMethodSuccess::Encoded { shape, .. } => {
                let ty = S::callback_encoded_out_value(shape)?;
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Handle { carrier, .. } => {
                let carrier = S::handle_carrier(carrier)?;
                let ty = carrier.ty();
                Ok(quote! {
                    let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#ty>::uninit();
                })
            }
            FallibleMethodSuccess::Closure { .. } => {
                let closure = self.foreign_closure_success()?;
                S::foreign_closure_success_storage(
                    closure,
                    self.source.fallible()?.ok_closure(closure.presence())?,
                    self.expansion,
                )
            }
        }
    }

    fn local_proxy_success_value(&self) -> Result<TokenStream, Error> {
        let local_return = self.local_return()?;
        match FallibleMethodSuccess::new(local_return.plan)? {
            FallibleMethodSuccess::Void => Ok(quote! { () }),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(_),
            } => Ok(quote! { unsafe { __boltffi_success_out.assume_init() } }),
            FallibleMethodSuccess::Direct { .. } => {
                let ok = self.source.fallible()?.ok_written_type()?;
                Ok(quote! {
                    unsafe {
                        <#ok as ::boltffi::__private::Passable>::unpack(
                            __boltffi_success_out.assume_init()
                        )
                    }
                })
            }
            FallibleMethodSuccess::Encoded { codec, shape } => {
                let fallible = self.source.fallible()?;
                let ok = fallible.ok_written_type()?;
                S::decode_callback_encoded_out_pointer(
                    shape,
                    quote! { unsafe { __boltffi_success_out.assume_init() } },
                    codec.root(),
                    ok,
                    fallible.ok(),
                    self.expansion,
                )
            }
            FallibleMethodSuccess::Handle {
                target,
                carrier,
                presence,
            } => self.foreign_handle_value(
                target,
                carrier,
                presence,
                quote! { unsafe { __boltffi_success_out.assume_init() } },
            ),
            FallibleMethodSuccess::Closure { .. } => {
                let closure = self.foreign_closure_success()?;
                S::foreign_closure_success_value(
                    closure,
                    self.source.fallible()?.ok_closure(closure.presence())?,
                    self.expansion,
                )
            }
        }
    }

    fn local_success_write(&self, owner: Ident) -> Result<LocalMethodBody, Error> {
        let local_return = self.local_return()?;
        match FallibleMethodSuccess::new(local_return.plan)? {
            FallibleMethodSuccess::Void => Ok(LocalMethodBody::empty()),
            FallibleMethodSuccess::Direct {
                ty: DirectValueType::Primitive(_),
            } => Ok(LocalMethodBody::new(quote! {
                if !__boltffi_success_out.is_null() {
                    unsafe {
                        *__boltffi_success_out = __boltffi_success;
                    }
                }
            })),
            FallibleMethodSuccess::Direct { .. } => {
                let ok = self.source.fallible()?.ok_written_type()?;
                Ok(LocalMethodBody::new(quote! {
                    if !__boltffi_success_out.is_null() {
                        unsafe {
                            *__boltffi_success_out =
                                <#ok as ::boltffi::__private::Passable>::pack(__boltffi_success);
                        }
                    }
                }))
            }
            FallibleMethodSuccess::Encoded { codec, shape } => {
                let buffer = wrapper::encoded::outgoing::Value::new(codec.root(), self.expansion)
                    .buffer(quote! { __boltffi_success })?;
                let value = S::encode_callback_encoded_out_pointer(shape, buffer)?;
                Ok(LocalMethodBody::new(quote! {
                    if !__boltffi_success_out.is_null() {
                        unsafe {
                            *__boltffi_success_out = #value;
                        }
                    }
                }))
            }
            FallibleMethodSuccess::Handle {
                target,
                carrier,
                presence,
            } => {
                let success = wrapper::names::Locals::new(Span::call_site()).success();
                let value = self.local_handle_value(target, carrier, presence, success)?;
                Ok(LocalMethodBody::new(quote! {
                    if !__boltffi_success_out.is_null() {
                        unsafe {
                            *__boltffi_success_out = #value;
                        }
                    }
                }))
            }
            FallibleMethodSuccess::Closure { closure } => {
                let writer = <wrapper::returns::closure::Write as Render<S, _>>::render(
                    wrapper::returns::closure::Write,
                    wrapper::returns::closure::WriteInput::success(
                        closure,
                        self.source.fallible()?.ok_closure(closure.presence())?,
                        wrapper::names::Locals::new(Span::call_site()).success(),
                        owner,
                        self.expansion,
                    ),
                )?;
                let (items, _ffi_parameters, body) = writer.into_parts();
                Ok(LocalMethodBody {
                    items,
                    body: quote! {
                        let __boltffi_return_out = __boltffi_success_out;
                        #body
                    },
                })
            }
        }
    }

    fn handle_abi(
        _target: &HandleTarget,
        carrier: S::HandleCarrier,
        _presence: HandlePresence,
    ) -> Result<CallbackReturnAbi, Error> {
        let carrier = S::handle_carrier(carrier)?;
        let ty = carrier.ty();
        Ok(CallbackReturnAbi::direct(quote! { -> #ty }))
    }

    fn foreign_handle_value(
        &self,
        target: &HandleTarget,
        _carrier: S::HandleCarrier,
        presence: HandlePresence,
        value: TokenStream,
    ) -> Result<TokenStream, Error> {
        match self.handle_return(target, presence)? {
            rust_api::HandleReturn::Callback(callback) => {
                let proxy = callback.proxy();
                let handle = S::callback_handle(value);
                Ok(match (callback.form(), callback.presence()) {
                    (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Required) => quote! {
                        unsafe {
                            <#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                        }
                    },
                    (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Required) => quote! {
                        unsafe {
                            <#proxy as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                        }
                    },
                    (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Required) => quote! {
                        unsafe {
                            *<#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                        }
                    },
                    (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Nullable) => quote! {
                        if #handle.is_null() {
                            None
                        } else {
                            Some(unsafe {
                                <#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                            })
                        }
                    },
                    (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Nullable) => quote! {
                        if #handle.is_null() {
                            None
                        } else {
                            Some(unsafe {
                                <#proxy as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                            })
                        }
                    },
                    (rust_api::CallbackCarrier::ImplTrait, HandlePresence::Nullable) => quote! {
                        if #handle.is_null() {
                            None
                        } else {
                            Some(unsafe {
                                *<#proxy as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                            })
                        }
                    },
                    _ => {
                        return Err(Error::UnsupportedExpansion(
                            "unknown callback handle presence",
                        ));
                    }
                })
            }
            rust_api::HandleReturn::Class(_) => Err(Error::UnsupportedExpansion(
                "callback method class handle return",
            )),
        }
    }

    fn local_handle_value(
        &self,
        target: &HandleTarget,
        carrier: S::HandleCarrier,
        presence: HandlePresence,
        value: Ident,
    ) -> Result<TokenStream, Error> {
        let handle_return = self.handle_return(target, presence)?;
        S::local_handle_value(
            self.expansion,
            target,
            carrier,
            presence,
            value,
            handle_return,
        )
    }

    fn handle_return(
        &self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<rust_api::HandleReturn, Error> {
        match self.error {
            ErrorDecl::None(_) => self.source.handle_return(target, presence),
            _ => self.source.fallible()?.ok_handle_return(target, presence),
        }
    }

    fn direct_return_type(
        ty: &DirectValueType,
        source: rust_api::Return<'lowered>,
    ) -> Result<TokenStream, Error> {
        match ty {
            DirectValueType::Primitive(primitive) => {
                let ffi_type = wrapper::type_ref::Renderer.primitive(*primitive)?;
                Ok(quote! { -> #ffi_type })
            }
            _ => {
                let rust_type = Self::source_type(source)?;
                Ok(quote! { -> <#rust_type as ::boltffi::__private::Passable>::In })
            }
        }
    }

    fn direct_source_type(&self) -> Result<Type, Error> {
        Self::source_type(self.source)
    }

    fn source_type(source: rust_api::Return<'lowered>) -> Result<Type, Error> {
        source.written_type()?.ok_or(Error::SourceSyntaxMismatch(
            "callback method return requires source return type",
        ))
    }

    fn local_return(&self) -> Result<LocalReturn<'lowered, S>, Error> {
        match (self.local_plan, self.local_error) {
            (Some(plan), Some(error)) => Ok(LocalReturn { plan, error }),
            (None, None) => Err(Error::UnsupportedExpansion(
                "callback local protocol return",
            )),
            _ => Err(Error::SourceSyntaxMismatch(
                "callback local return is incomplete",
            )),
        }
    }

    fn foreign_closure_return(&self) -> Result<&'lowered ClosureReturn<S, IntoRust>, Error> {
        match InfallibleMethodReturn::new(self.plan, self.error)? {
            InfallibleMethodReturn::Closure { closure } => Ok(closure),
            _ => Err(Error::SourceSyntaxMismatch(
                "callback local proxy closure return does not match foreign return",
            )),
        }
    }

    fn foreign_closure_success(&self) -> Result<&'lowered ClosureReturn<S, IntoRust>, Error> {
        match FallibleMethodSuccess::new(self.plan)? {
            FallibleMethodSuccess::Closure { closure } => Ok(closure),
            _ => Err(Error::SourceSyntaxMismatch(
                "callback local proxy closure success does not match foreign return",
            )),
        }
    }
}

trait CallbackMethodSurface: RenderSurface {
    fn handle_carrier(
        carrier: Self::HandleCarrier,
    ) -> Result<wrapper::handle::CarrierTokens, Error>;

    fn callback_handle(value: TokenStream) -> TokenStream;

    fn foreign_closure_parameter_tokens<'lowered>(
        closure: &'lowered ClosureParameter<Self, OutOfRust>,
        source: &'lowered ParameterDef,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<ForeignMethodParameterTokens, Error>;

    fn local_parameter_tokens<'lowered>(
        local: &'lowered ParamDecl<Self, IntoRust>,
        source: &'lowered ParameterDef,
        failure: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<LocalMethodParameterTokens, Error>;

    fn local_handle_value<'lowered>(
        expansion: &Expansion<'lowered, Self>,
        target: &'lowered HandleTarget,
        carrier: Self::HandleCarrier,
        presence: HandlePresence,
        value: Ident,
        handle_return: rust_api::HandleReturn,
    ) -> Result<TokenStream, Error>;

    fn callback_encoded_parameter(
        shape: Self::BufferShape,
    ) -> Result<CallbackEncodedParameter, Error>;

    fn foreign_scalar_option_parameter_tokens(
        primitive: Primitive,
        value: &Ident,
    ) -> Result<ForeignMethodParameterTokens, Error>;

    fn callback_encoded_return(
        shape: Self::BufferShape,
        ty: &TypeRef,
    ) -> Result<CallbackEncodedReturn, Error>;

    fn callback_encoded_error(shape: Self::BufferShape) -> Result<CallbackEncodedError, Error>;

    fn callback_encoded_out_pointer(shape: Self::BufferShape) -> Result<TokenStream, Error>;

    fn callback_encoded_out_value(shape: Self::BufferShape) -> Result<TokenStream, Error>;

    fn callback_closure_out_pointer() -> TokenStream;

    fn callback_closure_return_abi() -> CallbackReturnAbi;

    fn foreign_closure_return_body<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        call: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error>;

    fn foreign_closure_success_storage<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error>;

    fn foreign_closure_success_value<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error>;

    fn decode_callback_encoded_out_pointer<'lowered>(
        shape: Self::BufferShape,
        value: TokenStream,
        codec: &'lowered CodecNode,
        rust_type: Type,
        source: &'lowered TypeExpr,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error>;

    fn encode_callback_encoded_out_pointer(
        shape: Self::BufferShape,
        buffer: TokenStream,
    ) -> Result<TokenStream, Error>;
}

impl CallbackMethodSurface for Native {
    fn handle_carrier(
        carrier: Self::HandleCarrier,
    ) -> Result<wrapper::handle::CarrierTokens, Error> {
        <wrapper::handle::Carrier as Render<Native, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(carrier),
        )
    }

    fn callback_handle(value: TokenStream) -> TokenStream {
        value
    }

    fn foreign_closure_parameter_tokens<'lowered>(
        closure: &'lowered ClosureParameter<Self, OutOfRust>,
        source: &'lowered ParameterDef,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        NativeOutgoingClosure::new(closure, source, expansion).tokens()
    }

    fn local_parameter_tokens<'lowered>(
        local: &'lowered ParamDecl<Self, IntoRust>,
        source: &'lowered ParameterDef,
        failure: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<LocalMethodParameterTokens, Error> {
        let tokens = <wrapper::param::Renderer as Render<Native, _>>::render(
            wrapper::param::Renderer,
            wrapper::param::Input::new(local, rust_api::Parameter::new(source), failure, expansion),
        )?;
        Ok(LocalMethodParameterTokens {
            ffi_parameters: tokens.ffi_parameters().to_vec(),
            setup: tokens.conversions().to_vec(),
            arguments: vec![tokens.argument().clone()],
        })
    }

    fn local_handle_value<'lowered>(
        expansion: &Expansion<'lowered, Self>,
        target: &'lowered HandleTarget,
        carrier: Self::HandleCarrier,
        presence: HandlePresence,
        value: Ident,
        handle_return: rust_api::HandleReturn,
    ) -> Result<TokenStream, Error> {
        <wrapper::returns::handle::Value as Render<Native, _>>::render(
            wrapper::returns::handle::Value,
            wrapper::returns::handle::ValueInput::new(
                expansion,
                target,
                carrier,
                presence,
                value,
                handle_return,
            ),
        )
        .map(|tokens| tokens.value().clone())
    }

    fn callback_encoded_parameter(
        shape: Self::BufferShape,
    ) -> Result<CallbackEncodedParameter, Error> {
        match shape {
            native::BufferShape::Slice => Ok(CallbackEncodedParameter::Slice),
            native::BufferShape::Buffer | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback method encoded parameter shape"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback method encoded parameter shape",
            )),
        }
    }

    fn foreign_scalar_option_parameter_tokens(
        _primitive: Primitive,
        value: &Ident,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        let parameter_names = wrapper::names::Parameter::new(value);
        let buffer = parameter_names.buffer();
        let pointer = parameter_names.pointer();
        let length = parameter_names.length();
        Ok(ForeignMethodParameterTokens::new(
            quote! { *const u8 },
            vec![quote! {
                let #buffer = if #value.is_some() {
                    ::boltffi::__private::FfiBuf::wire_encode(&#value)
                } else {
                    ::boltffi::__private::FfiBuf::default()
                };
                let #pointer = #buffer.as_ptr();
                let #length = #buffer.len();
            }],
            quote! { #pointer },
        )
        .with_extra_ffi_parameter(quote! { usize }, quote! { #length }))
    }

    fn callback_encoded_return(
        shape: Self::BufferShape,
        _ty: &TypeRef,
    ) -> Result<CallbackEncodedReturn, Error> {
        match shape {
            native::BufferShape::Buffer => Ok(CallbackEncodedReturn::NativeBuffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback method encoded return shape"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback method encoded return shape",
            )),
        }
    }

    fn callback_encoded_error(shape: Self::BufferShape) -> Result<CallbackEncodedError, Error> {
        match shape {
            native::BufferShape::Buffer => Ok(CallbackEncodedError::NativeBuffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback method error shape"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback method error shape",
            )),
        }
    }

    fn callback_encoded_out_pointer(shape: Self::BufferShape) -> Result<TokenStream, Error> {
        match shape {
            native::BufferShape::Buffer => Ok(quote! { *mut ::boltffi::__private::FfiBuf }),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback encoded out-pointer"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback encoded out-pointer",
            )),
        }
    }

    fn callback_encoded_out_value(shape: Self::BufferShape) -> Result<TokenStream, Error> {
        match shape {
            native::BufferShape::Buffer => Ok(quote! { ::boltffi::__private::FfiBuf }),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback encoded out value"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback encoded out value",
            )),
        }
    }

    fn callback_closure_out_pointer() -> TokenStream {
        quote! { *mut ::core::ffi::c_void }
    }

    fn callback_closure_return_abi() -> CallbackReturnAbi {
        let output = quote! { __boltffi_return_out };
        CallbackReturnAbi {
            foreign_ffi_parameters: vec![quote! { #output: *mut ::core::ffi::c_void }],
            foreign_arguments: vec![quote! { #output.as_mut_ptr() as *mut ::core::ffi::c_void }],
            foreign_return_type: quote! { -> ::boltffi::__private::FfiStatus },
            local_ffi_parameters: vec![quote! { #output: *mut ::core::ffi::c_void }],
            local_arguments: vec![quote! { #output.as_mut_ptr() as *mut ::core::ffi::c_void }],
            local_return_type: quote! { -> ::boltffi::__private::FfiStatus },
        }
    }

    fn foreign_closure_return_body<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        call: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        let value = wrapper::names::Locals::new(Span::call_site()).closure();
        let slot_names = wrapper::names::ClosureRegistration::new(&value);
        let invoke = slot_names.call();
        let context = slot_names.context();
        let release = slot_names.release();
        let tokens = <wrapper::param::closure::Renderer as Render<Native, _>>::render(
            wrapper::param::closure::Renderer,
            wrapper::param::closure::Input::returned(
                closure,
                source,
                value.clone(),
                quote! {
                    panic!("callback method closure return conversion failed")
                },
                expansion,
            ),
        )?;
        let mut ffi_parameter_types = tokens.ffi_parameter_types().iter().cloned();
        let invoke_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure return is missing invoke slot",
            ))?;
        let context_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure return is missing context slot",
            ))?;
        let release_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure return is missing release slot",
            ))?;
        let storage = format_ident!("__BoltffiCallbackClosureReturn{}", value);
        let conversions = tokens.conversions();
        let argument = tokens.argument();
        Ok(quote! {
            {
                #[repr(C)]
                struct #storage {
                    invoke: #invoke_type,
                    context: #context_type,
                    release: #release_type,
                }

                let mut __boltffi_return_out = ::core::mem::MaybeUninit::<#storage>::uninit();
                let __boltffi_status = unsafe { #call };
                if __boltffi_status.is_err() {
                    panic!("callback method closure return failed");
                }
                let __boltffi_returned = unsafe { __boltffi_return_out.assume_init() };
                let #invoke = __boltffi_returned.invoke;
                let #context = __boltffi_returned.context;
                let #release = __boltffi_returned.release;
                #(#conversions)*
                #argument
            }
        })
    }

    fn foreign_closure_success_storage<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        let value = wrapper::names::Locals::new(Span::call_site()).closure();
        let tokens = <wrapper::param::closure::Renderer as Render<Native, _>>::render(
            wrapper::param::closure::Renderer,
            wrapper::param::closure::Input::returned(
                closure,
                source,
                value.clone(),
                quote! {
                    panic!("callback method closure success conversion failed")
                },
                expansion,
            ),
        )?;
        let mut ffi_parameter_types = tokens.ffi_parameter_types().iter().cloned();
        let invoke_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure success is missing invoke slot",
            ))?;
        let context_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure success is missing context slot",
            ))?;
        let release_type = ffi_parameter_types
            .next()
            .ok_or(Error::SourceSyntaxMismatch(
                "native closure success is missing release slot",
            ))?;
        let storage = format_ident!("__BoltffiCallbackClosureReturn{}", value);
        Ok(quote! {
            #[repr(C)]
            struct #storage {
                invoke: #invoke_type,
                context: #context_type,
                release: #release_type,
            }
            let mut __boltffi_success_out = ::core::mem::MaybeUninit::<#storage>::uninit();
        })
    }

    fn foreign_closure_success_value<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        let value = wrapper::names::Locals::new(Span::call_site()).closure();
        let slot_names = wrapper::names::ClosureRegistration::new(&value);
        let invoke = slot_names.call();
        let context = slot_names.context();
        let release = slot_names.release();
        let tokens = <wrapper::param::closure::Renderer as Render<Native, _>>::render(
            wrapper::param::closure::Renderer,
            wrapper::param::closure::Input::returned(
                closure,
                source,
                value,
                quote! {
                    panic!("callback method closure success conversion failed")
                },
                expansion,
            ),
        )?;
        let conversions = tokens.conversions();
        let argument = tokens.argument();
        Ok(quote! {
            {
                let __boltffi_returned = unsafe { __boltffi_success_out.assume_init() };
                let #invoke = __boltffi_returned.invoke;
                let #context = __boltffi_returned.context;
                let #release = __boltffi_returned.release;
                #(#conversions)*
                #argument
            }
        })
    }

    fn decode_callback_encoded_out_pointer<'lowered>(
        shape: Self::BufferShape,
        value: TokenStream,
        codec: &'lowered CodecNode,
        rust_type: Type,
        source: &'lowered TypeExpr,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        match shape {
            native::BufferShape::Buffer => {
                let decoded = wrapper::encoded::incoming::Value::new(codec, expansion).expression(
                    wrapper::encoded::incoming::Bytes::new(
                        &rust_type,
                        source,
                        quote! { __boltffi_bytes },
                        quote! {
                            panic!("callback method success conversion failed: {:?}", error)
                        },
                    ),
                )?;
                Ok(quote! {
                    {
                        let __boltffi_buffer = #value;
                        let __boltffi_bytes = unsafe { __boltffi_buffer.as_byte_slice() };
                        #decoded
                    }
                })
            }
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback encoded out-pointer"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback encoded out-pointer",
            )),
        }
    }

    fn encode_callback_encoded_out_pointer(
        shape: Self::BufferShape,
        buffer: TokenStream,
    ) -> Result<TokenStream, Error> {
        match shape {
            native::BufferShape::Buffer => Ok(buffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native callback encoded out-pointer"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native callback encoded out-pointer",
            )),
        }
    }
}

impl CallbackMethodSurface for Wasm32 {
    fn handle_carrier(
        carrier: Self::HandleCarrier,
    ) -> Result<wrapper::handle::CarrierTokens, Error> {
        <wrapper::handle::Carrier as Render<Wasm32, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(carrier),
        )
    }

    fn callback_handle(value: TokenStream) -> TokenStream {
        quote! { ::boltffi::__private::CallbackHandle::from_wasm_handle(#value) }
    }

    fn foreign_closure_parameter_tokens<'lowered>(
        closure: &'lowered ClosureParameter<Self, OutOfRust>,
        source: &'lowered ParameterDef,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        WasmOutgoingClosure::new(closure, source, expansion).tokens()
    }

    fn local_parameter_tokens<'lowered>(
        local: &'lowered ParamDecl<Self, IntoRust>,
        source: &'lowered ParameterDef,
        failure: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<LocalMethodParameterTokens, Error> {
        let tokens = <wrapper::param::Renderer as Render<Wasm32, _>>::render(
            wrapper::param::Renderer,
            wrapper::param::Input::new(local, rust_api::Parameter::new(source), failure, expansion),
        )?;
        Ok(LocalMethodParameterTokens {
            ffi_parameters: tokens.ffi_parameters().to_vec(),
            setup: tokens.conversions().to_vec(),
            arguments: vec![tokens.argument().clone()],
        })
    }

    fn local_handle_value<'lowered>(
        expansion: &Expansion<'lowered, Self>,
        target: &'lowered HandleTarget,
        carrier: Self::HandleCarrier,
        presence: HandlePresence,
        value: Ident,
        handle_return: rust_api::HandleReturn,
    ) -> Result<TokenStream, Error> {
        <wrapper::returns::handle::Value as Render<Wasm32, _>>::render(
            wrapper::returns::handle::Value,
            wrapper::returns::handle::ValueInput::new(
                expansion,
                target,
                carrier,
                presence,
                value,
                handle_return,
            ),
        )
        .map(|tokens| tokens.value().clone())
    }

    fn callback_encoded_parameter(
        shape: Self::BufferShape,
    ) -> Result<CallbackEncodedParameter, Error> {
        match shape {
            wasm32::BufferShape::Slice => Ok(CallbackEncodedParameter::Slice),
            wasm32::BufferShape::Packed => Err(Error::UnsupportedExpansion(
                "wasm callback method encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback method encoded parameter shape",
            )),
        }
    }

    fn foreign_scalar_option_parameter_tokens(
        primitive: Primitive,
        value: &Ident,
    ) -> Result<ForeignMethodParameterTokens, Error> {
        let encoded = wrapper::names::Parameter::new(value).packed();
        let present = wrapper::names::Locals::new(value.span()).value();
        let scalar = wrapper::scalar_option::WasmScalar::new(primitive, present.clone());
        let ffi_type = scalar.carrier_type();
        let none = scalar.none();
        let some = scalar.outgoing()?;
        Ok(ForeignMethodParameterTokens::new(
            ffi_type,
            vec![quote! {
                let #encoded = match #value {
                    Some(#present) => #some,
                    None => #none,
                };
            }],
            quote! { #encoded },
        ))
    }

    fn callback_encoded_return(
        shape: Self::BufferShape,
        ty: &TypeRef,
    ) -> Result<CallbackEncodedReturn, Error> {
        match (shape, ty) {
            (wasm32::BufferShape::Packed, TypeRef::String) => {
                Ok(CallbackEncodedReturn::WasmPackedString)
            }
            (wasm32::BufferShape::Packed, _) => Ok(CallbackEncodedReturn::WasmOutBuffer),
            (wasm32::BufferShape::Slice, _) => Err(Error::UnsupportedExpansion(
                "wasm callback method encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback method encoded return shape",
            )),
        }
    }

    fn callback_encoded_error(shape: Self::BufferShape) -> Result<CallbackEncodedError, Error> {
        match shape {
            wasm32::BufferShape::Packed => Ok(CallbackEncodedError::WasmPacked),
            wasm32::BufferShape::Slice => Err(Error::UnsupportedExpansion(
                "wasm callback method error shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback method error shape",
            )),
        }
    }

    fn callback_encoded_out_pointer(shape: Self::BufferShape) -> Result<TokenStream, Error> {
        match shape {
            wasm32::BufferShape::Packed => Ok(quote! { *mut u64 }),
            wasm32::BufferShape::Slice => Err(Error::UnsupportedExpansion(
                "wasm callback encoded out-pointer",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback encoded out-pointer",
            )),
        }
    }

    fn callback_encoded_out_value(shape: Self::BufferShape) -> Result<TokenStream, Error> {
        match shape {
            wasm32::BufferShape::Packed => Ok(quote! { u64 }),
            wasm32::BufferShape::Slice => Err(Error::UnsupportedExpansion(
                "wasm callback encoded out value",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback encoded out value",
            )),
        }
    }

    fn callback_closure_out_pointer() -> TokenStream {
        quote! { *mut u32 }
    }

    fn callback_closure_return_abi() -> CallbackReturnAbi {
        let output = quote! { __boltffi_return_out };
        CallbackReturnAbi {
            foreign_ffi_parameters: vec![quote! { #output: *mut u32 }],
            foreign_arguments: vec![quote! { #output.as_mut_ptr() }],
            foreign_return_type: quote! { -> ::boltffi::__private::FfiStatus },
            local_ffi_parameters: vec![quote! { #output: *mut u32 }],
            local_arguments: vec![quote! { #output.as_mut_ptr() }],
            local_return_type: quote! { -> ::boltffi::__private::FfiStatus },
        }
    }

    fn foreign_closure_return_body<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        call: TokenStream,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        let value = wrapper::names::Locals::new(Span::call_site()).closure();
        let tokens = <wrapper::param::closure::Renderer as Render<Wasm32, _>>::render(
            wrapper::param::closure::Renderer,
            wrapper::param::closure::Input::returned(
                closure,
                source,
                value.clone(),
                quote! {
                    panic!("callback method closure return conversion failed")
                },
                expansion,
            ),
        )?;
        let conversions = tokens.conversions();
        let argument = tokens.argument();
        Ok(quote! {
            {
                let mut __boltffi_return_out = ::core::mem::MaybeUninit::<u32>::uninit();
                let __boltffi_status = unsafe { #call };
                if __boltffi_status.is_err() {
                    panic!("callback method closure return failed");
                }
                let #value = unsafe { __boltffi_return_out.assume_init() };
                #(#conversions)*
                #argument
            }
        })
    }

    fn foreign_closure_success_storage<'lowered>(
        _closure: &'lowered ClosureReturn<Self, IntoRust>,
        _source: rust_api::Closure,
        _expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        Ok(quote! {
            let mut __boltffi_success_out = ::core::mem::MaybeUninit::<u32>::uninit();
        })
    }

    fn foreign_closure_success_value<'lowered>(
        closure: &'lowered ClosureReturn<Self, IntoRust>,
        source: rust_api::Closure,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        let value = wrapper::names::Locals::new(Span::call_site()).closure();
        let tokens = <wrapper::param::closure::Renderer as Render<Wasm32, _>>::render(
            wrapper::param::closure::Renderer,
            wrapper::param::closure::Input::returned(
                closure,
                source,
                value.clone(),
                quote! {
                    panic!("callback method closure success conversion failed")
                },
                expansion,
            ),
        )?;
        let conversions = tokens.conversions();
        let argument = tokens.argument();
        Ok(quote! {
            {
                let #value = unsafe { __boltffi_success_out.assume_init() };
                #(#conversions)*
                #argument
            }
        })
    }

    fn decode_callback_encoded_out_pointer<'lowered>(
        shape: Self::BufferShape,
        value: TokenStream,
        codec: &'lowered CodecNode,
        rust_type: Type,
        source: &'lowered TypeExpr,
        expansion: &Expansion<'lowered, Self>,
    ) -> Result<TokenStream, Error> {
        match shape {
            wasm32::BufferShape::Packed => PackedEncodedValue::new(
                value,
                codec,
                rust_type,
                source,
                expansion,
                quote! {
                    panic!("callback method success conversion failed: {:?}", error)
                },
            )
            .expression(),
            wasm32::BufferShape::Slice => Err(Error::UnsupportedExpansion(
                "wasm callback encoded out-pointer",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback encoded out-pointer",
            )),
        }
    }

    fn encode_callback_encoded_out_pointer(
        shape: Self::BufferShape,
        buffer: TokenStream,
    ) -> Result<TokenStream, Error> {
        match shape {
            wasm32::BufferShape::Packed => Ok(quote! { #buffer.into_packed() }),
            wasm32::BufferShape::Slice => Err(Error::UnsupportedExpansion(
                "wasm callback encoded out-pointer",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm callback encoded out-pointer",
            )),
        }
    }
}

enum CallbackEncodedParameter {
    Slice,
}

enum CallbackEncodedReturn {
    NativeBuffer,
    WasmPackedString,
    WasmOutBuffer,
}

enum CallbackEncodedError {
    NativeBuffer,
    WasmPacked,
}

impl CallbackEncodedError {
    fn return_type(&self) -> TokenStream {
        match self {
            Self::NativeBuffer => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmPacked => quote! { -> u64 },
        }
    }

    fn empty_value(&self) -> TokenStream {
        match self {
            Self::NativeBuffer => quote! { ::boltffi::__private::FfiBuf::default() },
            Self::WasmPacked => quote! { ::boltffi::__private::FfiBuf::default().into_packed() },
        }
    }

    fn return_value(&self, buffer: TokenStream) -> TokenStream {
        match self {
            Self::NativeBuffer => buffer,
            Self::WasmPacked => quote! { #buffer.into_packed() },
        }
    }

    fn is_empty(&self, value: TokenStream) -> TokenStream {
        match self {
            Self::NativeBuffer => quote! { #value.is_empty() },
            Self::WasmPacked => quote! { #value == 0 },
        }
    }

    fn decode<'lowered, S: RenderSurface>(
        &self,
        value: TokenStream,
        codec: &CodecNode,
        rust_type: Type,
        source: &TypeExpr,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::NativeBuffer => wrapper::encoded::incoming::Value::new(codec, expansion)
                .expression(wrapper::encoded::incoming::Bytes::new(
                    &rust_type,
                    source,
                    quote! { unsafe { #value.as_byte_slice() } },
                    quote! {
                        panic!("callback method error conversion failed: {:?}", error)
                    },
                )),
            Self::WasmPacked => PackedEncodedValue::new(
                value,
                codec,
                rust_type,
                source,
                expansion,
                quote! {
                    panic!("callback method error conversion failed: {:?}", error)
                },
            )
            .expression(),
        }
    }
}

struct CallbackReturnAbi {
    foreign_ffi_parameters: Vec<TokenStream>,
    foreign_arguments: Vec<TokenStream>,
    foreign_return_type: TokenStream,
    local_ffi_parameters: Vec<TokenStream>,
    local_arguments: Vec<TokenStream>,
    local_return_type: TokenStream,
}

impl CallbackReturnAbi {
    fn empty() -> Self {
        Self {
            foreign_ffi_parameters: Vec::new(),
            foreign_arguments: Vec::new(),
            foreign_return_type: TokenStream::new(),
            local_ffi_parameters: Vec::new(),
            local_arguments: Vec::new(),
            local_return_type: TokenStream::new(),
        }
    }

    fn direct(return_type: TokenStream) -> Self {
        Self {
            foreign_return_type: return_type.clone(),
            local_return_type: return_type,
            ..Self::empty()
        }
    }

    fn direct_out(rust_type: Type) -> Self {
        let output = quote! { __boltffi_return_out };
        let parameter = quote! {
            #output: *mut <#rust_type as ::boltffi::__private::Passable>::Out
        };
        let argument = quote! { #output.as_mut_ptr() };
        Self {
            foreign_ffi_parameters: vec![parameter.clone()],
            foreign_arguments: vec![argument.clone()],
            foreign_return_type: TokenStream::new(),
            local_ffi_parameters: vec![parameter],
            local_arguments: vec![argument],
            local_return_type: TokenStream::new(),
        }
    }
}

impl CallbackEncodedReturn {
    fn abi(&self) -> CallbackReturnAbi {
        match self {
            Self::NativeBuffer => {
                CallbackReturnAbi::direct(quote! { -> ::boltffi::__private::FfiBuf })
            }
            Self::WasmPackedString => CallbackReturnAbi::direct(quote! { -> u64 }),
            Self::WasmOutBuffer => {
                let out_buffer = quote! { __boltffi_callback_out: *mut ::boltffi::__private::WasmCallbackOutBuf };
                CallbackReturnAbi {
                    foreign_ffi_parameters: vec![out_buffer],
                    foreign_arguments: vec![quote! { &mut __boltffi_callback_out }],
                    foreign_return_type: TokenStream::new(),
                    local_ffi_parameters: Vec::new(),
                    local_arguments: Vec::new(),
                    local_return_type: quote! { -> u64 },
                }
            }
        }
    }

    fn foreign_body<'lowered, S: RenderSurface>(
        &self,
        call: TokenStream,
        codec: &CodecNode,
        rust_type: Type,
        source: &TypeExpr,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::NativeBuffer => {
                let value = wrapper::encoded::incoming::Value::new(codec, expansion).expression(
                    wrapper::encoded::incoming::Bytes::new(
                        &rust_type,
                        source,
                        quote! { __boltffi_bytes },
                        quote! {
                            panic!("callback method return conversion failed: {:?}", error)
                        },
                    ),
                )?;
                Ok(quote! {
                    {
                        let __boltffi_result = unsafe { #call };
                        let __boltffi_bytes = unsafe {
                            __boltffi_result.as_byte_slice()
                        };
                        #value
                    }
                })
            }
            Self::WasmPackedString => Ok(quote! {
                unsafe {
                    ::boltffi::__private::take_packed_utf8_string(#call)
                }
            }),
            Self::WasmOutBuffer => {
                let value = wrapper::encoded::incoming::Value::new(codec, expansion).expression(
                    wrapper::encoded::incoming::Bytes::new(
                        &rust_type,
                        source,
                        quote! { __boltffi_bytes },
                        quote! {
                            panic!("callback method return conversion failed: {:?}", error)
                        },
                    ),
                )?;
                Ok(quote! {
                    {
                        let mut __boltffi_callback_out =
                            ::boltffi::__private::WasmCallbackOutBuf::empty();
                        unsafe {
                            #call;
                        }
                        let __boltffi_bytes = unsafe {
                            __boltffi_callback_out.as_slice()
                        };
                        #value
                    }
                })
            }
        }
    }

    fn local_proxy_body<'lowered, S: RenderSurface>(
        &self,
        call: TokenStream,
        codec: &CodecNode,
        rust_type: Type,
        source: &TypeExpr,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::NativeBuffer => {
                let value = wrapper::encoded::incoming::Value::new(codec, expansion).expression(
                    wrapper::encoded::incoming::Bytes::new(
                        &rust_type,
                        source,
                        quote! { __boltffi_bytes },
                        quote! {
                            panic!("callback method return conversion failed: {:?}", error)
                        },
                    ),
                )?;
                Ok(quote! {
                    {
                        let __boltffi_result = #call;
                        let __boltffi_bytes = unsafe {
                            __boltffi_result.as_byte_slice()
                        };
                        #value
                    }
                })
            }
            Self::WasmPackedString => Ok(quote! {
                unsafe {
                    ::boltffi::__private::take_packed_utf8_string(#call)
                }
            }),
            Self::WasmOutBuffer => PackedEncodedValue::new(
                call,
                codec,
                rust_type,
                source,
                expansion,
                quote! {
                    panic!("callback method return conversion failed: {:?}", error)
                },
            )
            .expression(),
        }
    }

    fn local_body<'lowered, S: RenderSurface>(
        &self,
        call: TokenStream,
        codec: &CodecNode,
        expansion: &Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error> {
        let buffer = wrapper::encoded::outgoing::Value::new(codec, expansion).buffer(call)?;
        Ok(match self {
            Self::NativeBuffer => quote! { #buffer },
            Self::WasmPackedString | Self::WasmOutBuffer => quote! { #buffer.into_packed() },
        })
    }
}

struct LocalNativeMethod {
    vtable_field: TokenStream,
    function: TokenStream,
}

struct LocalMethodBody {
    items: Vec<TokenStream>,
    body: TokenStream,
}

impl LocalMethodBody {
    fn empty() -> Self {
        Self {
            items: Vec::new(),
            body: TokenStream::new(),
        }
    }

    fn new(body: TokenStream) -> Self {
        Self {
            items: Vec::new(),
            body,
        }
    }
}

struct ReceiverTokens {
    receiver: Receiver,
}

impl ReceiverTokens {
    fn new(receiver: Receiver) -> Result<Self, Error> {
        match receiver {
            Receiver::Shared => Ok(Self { receiver }),
            Receiver::None => Err(Error::UnsupportedExpansion("callback associated function")),
            Receiver::Owned => Err(Error::UnsupportedExpansion("owned callback receiver")),
            Receiver::Mutable => Err(Error::UnsupportedExpansion("mutable callback receiver")),
        }
    }

    fn tokens(&self) -> TokenStream {
        match self.receiver {
            Receiver::Shared => quote! { &self },
            Receiver::None | Receiver::Owned | Receiver::Mutable => TokenStream::new(),
        }
    }
}

struct AsyncTraitAttribute<'source> {
    attr: &'source UserAttr,
}

impl<'source> AsyncTraitAttribute<'source> {
    fn from_trait(source: &'source TraitDef) -> Result<TokenStream, Error> {
        source
            .user_attrs
            .iter()
            .find_map(Self::new)
            .map(|attr| attr.tokens())
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn new(attr: &'source UserAttr) -> Option<Self> {
        attr.path
            .last()
            .is_some_and(|segment| segment.name.as_str() == "async_trait")
            .then_some(Self { attr })
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        match &self.attr.input {
            AttributeInput::Empty => Ok(quote! { #[::async_trait::async_trait] }),
            AttributeInput::Tokens(tokens) => {
                let tokens = tokens
                    .parse::<TokenStream>()
                    .map_err(|_| Error::SourceSyntaxMismatch("async_trait attribute input"))?;
                Ok(quote! { #[::async_trait::async_trait(#tokens)] })
            }
            AttributeInput::Value(_) | AttributeInput::List(_) => {
                Err(Error::UnsupportedExpansion("async_trait attribute input"))
            }
        }
    }
}

struct CallbackNames {
    trait_ident: Ident,
    foreign_ident: Ident,
    vtable_ident: Ident,
    foreign_vtable_static: Ident,
    local: Option<LocalCallbackNames>,
}

impl CallbackNames {
    fn new<S: RenderSurface>(
        source: &TraitDef,
        local_protocol: Option<&CallbackLocalProtocol<S>>,
    ) -> Result<Self, Error> {
        let trait_ident = RustIdent::new(source.name.spelling())?;
        let trait_fragment = trait_ident.as_ident().clone();
        let uppercase = source
            .name
            .spelling()
            .chars()
            .map(|character| match character.is_ascii_alphanumeric() {
                true => character.to_ascii_uppercase(),
                false => '_',
            })
            .collect::<String>();
        Ok(Self {
            foreign_ident: format_ident!("Foreign{}", trait_fragment),
            vtable_ident: format_ident!("{}VTable", trait_fragment),
            foreign_vtable_static: format_ident!("__BOLTFFI_FOREIGN_{}_VTABLE", uppercase),
            trait_ident: trait_ident.into_ident(),
            local: local_protocol
                .map(|protocol| LocalCallbackNames::new(protocol, &trait_fragment, &uppercase))
                .transpose()?,
        })
    }

    fn local(&self) -> Option<&LocalCallbackNames> {
        self.local.as_ref()
    }
}

struct LocalCallbackNames {
    proxy_ident: Ident,
    vtable_static: Ident,
    state_ident: Ident,
    registry_ident: Ident,
    next_ident: Ident,
    lookup_ident: Ident,
    free_ident: Ident,
    clone_ident: Ident,
    handle_ident: Ident,
}

impl LocalCallbackNames {
    fn new<S: RenderSurface>(
        protocol: &CallbackLocalProtocol<S>,
        trait_ident: &Ident,
        uppercase: &str,
    ) -> Result<Self, Error> {
        let handle_ident = RustIdent::from_local_function(protocol.handle())?;
        Ok(Self {
            proxy_ident: format_ident!("Local{}", trait_ident),
            vtable_static: format_ident!("__BOLTFFI_LOCAL_{}_VTABLE", uppercase),
            state_ident: format_ident!("__BoltffiLocal{}State", trait_ident),
            registry_ident: format_ident!("__BOLTFFI_LOCAL_{}_REGISTRY", uppercase),
            next_ident: format_ident!("__BOLTFFI_LOCAL_{}_NEXT_HANDLE", uppercase),
            lookup_ident: format_ident!("{}_lookup", handle_ident.as_ident()),
            free_ident: RustIdent::from_local_function(protocol.free())?.into_ident(),
            clone_ident: RustIdent::from_local_function(protocol.clone_fn())?.into_ident(),
            handle_ident: handle_ident.into_ident(),
        })
    }
}

struct PackedEncodedValue<'expansion, 'lowered, S: RenderSurface> {
    packed: TokenStream,
    codec: &'lowered CodecNode,
    rust_type: Type,
    source: &'lowered TypeExpr,
    expansion: &'expansion Expansion<'lowered, S>,
    failure: TokenStream,
}

impl<'expansion, 'lowered, S: RenderSurface> PackedEncodedValue<'expansion, 'lowered, S> {
    fn new(
        packed: TokenStream,
        codec: &'lowered CodecNode,
        rust_type: Type,
        source: &'lowered TypeExpr,
        expansion: &'expansion Expansion<'lowered, S>,
        failure: TokenStream,
    ) -> Self {
        Self {
            packed,
            codec,
            rust_type,
            source,
            expansion,
            failure,
        }
    }

    fn expression(self) -> Result<TokenStream, Error> {
        wrapper::encoded::incoming::Value::new(self.codec, self.expansion).packed_expression(
            &self.rust_type,
            self.source,
            self.packed,
            self.failure,
        )
    }
}

struct RustIdent(Ident);

impl RustIdent {
    fn new(name: &str) -> Result<Self, Error> {
        parse_str::<Ident>(name)
            .map(Self)
            .map_err(|_| Error::SourceSyntaxMismatch("generated callback identifier is not Rust"))
    }

    fn from_local_function(function: &CallbackLocalFunction) -> Result<Self, Error> {
        function
            .segments()
            .last()
            .map(|segment| segment.as_str())
            .ok_or(Error::SourceSyntaxMismatch(
                "callback local function path is empty",
            ))
            .and_then(Self::new)
    }

    fn as_ident(&self) -> &Ident {
        &self.0
    }

    fn into_ident(self) -> Ident {
        self.0
    }
}

impl quote::ToTokens for RustIdent {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

struct WasmImport {
    module: LitStr,
    ident: Ident,
}

impl WasmImport {
    fn new(import: &ImportSymbol) -> Result<Self, Error> {
        Ok(Self {
            module: LitStr::new(import.module().as_str(), Span::call_site()),
            ident: RustIdent::new(import.name().as_str())?.0,
        })
    }

    fn declaration(&self, item: TokenStream) -> TokenStream {
        let module = &self.module;
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[link(wasm_import_module = #module)]
            unsafe extern "C" {
                #item
            }
        }
    }

    fn ident(&self) -> &Ident {
        &self.ident
    }
}
