use crate::custom_types;
use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Pat, ReturnType, Type};

pub fn ffi_trait_impl(item: TokenStream) -> TokenStream {
    let item_trait = syn::parse_macro_input!(item as syn::ItemTrait);
    expand_ffi_trait(item_trait).unwrap_or_else(|error| error.to_compile_error()).into()
}

fn expand_ffi_trait(item_trait: syn::ItemTrait) -> Result<proc_macro2::TokenStream, syn::Error> {
    let custom_types = custom_types::registry_for_current_crate()?;
    let trait_name = &item_trait.ident;
    let trait_name_snake = to_snake_case_ident(&trait_name.to_string());
    let vtable_name = syn::Ident::new(&format!("{}VTable", trait_name), trait_name.span());
    let foreign_name = syn::Ident::new(&format!("Foreign{}", trait_name), trait_name.span());
    let vtable_static = syn::Ident::new(&format!("{}_VTABLE", trait_name_snake.to_string().to_uppercase()), trait_name.span());
    let register_fn = syn::Ident::new(&format!("{}_register_{}_vtable", naming::ffi_prefix(), trait_name_snake), trait_name.span());
    let create_fn = syn::Ident::new(&format!("{}_create_{}_handle", naming::ffi_prefix(), trait_name_snake), trait_name.span());

    let has_async_trait_attr = item_trait.attrs.iter().any(|attr| attr.path().segments.last().is_some_and(|s| s.ident == "async_trait"));

    let async_trait_attr = item_trait
        .attrs
        .iter()
        .find(|attr| attr.path().segments.last().is_some_and(|s| s.ident == "async_trait"))
        .cloned();

    let mut vtable_fields = vec![quote! { pub free: extern "C" fn(handle: u64) }, quote! { pub clone: extern "C" fn(handle: u64) -> u64 }];

    let has_async_methods = item_trait
        .items
        .iter()
        .any(|item| matches!(item, syn::TraitItem::Fn(method) if method.sig.asyncness.is_some()));

    let is_object_safe = !has_async_methods || has_async_trait_attr;

    let foreign_impls = item_trait
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(method) => Some(method),
            _ => None,
        })
        .map(|method| expand_method(method, &mut vtable_fields, &custom_types))
        .collect::<Result<Vec<_>, _>>()?;

    let wasm_expansions: Vec<WasmMethodExpansion> = item_trait
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(method) => Some(method),
            _ => None,
        })
        .map(|method| expand_method_wasm(method, &trait_name_snake, &custom_types))
        .collect::<Result<Vec<_>, _>>()?;

    let wasm_extern_imports: Vec<_> = wasm_expansions.iter().map(|e| &e.extern_import).collect();
    let wasm_impl_bodies: Vec<_> = wasm_expansions.iter().map(|e| &e.impl_body).collect();
    let wasm_complete_exports: Vec<_> = wasm_expansions.iter().filter_map(|e| e.complete_export.as_ref()).collect();

    let wasm_free_import = format_ident!("__boltffi_callback_{}_free", trait_name_snake);
    let wasm_clone_import = format_ident!("__boltffi_callback_{}_clone", trait_name_snake);
    let wasm_create_fn = format_ident!("{}_create_{}_handle", naming::ffi_prefix(), trait_name_snake);

    let expanded = quote! {
        #item_trait

        #[cfg(not(target_arch = "wasm32"))]
        #[repr(C)]
        pub struct #vtable_name {
            #(#vtable_fields),*
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[derive(Debug)]
        pub struct #foreign_name {
            vtable: *const #vtable_name,
            handle: u64,
        }

        #[cfg(target_arch = "wasm32")]
        #[derive(Debug)]
        pub struct #foreign_name {
            handle: u32,
        }

        unsafe impl Send for #foreign_name {}
        unsafe impl Sync for #foreign_name {}

        #[cfg(not(target_arch = "wasm32"))]
        impl Drop for #foreign_name {
            fn drop(&mut self) {
                unsafe { ((*self.vtable).free)(self.handle) };
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        impl Clone for #foreign_name {
            fn clone(&self) -> Self {
                let new_handle = unsafe { ((*self.vtable).clone)(self.handle) };
                Self {
                    vtable: self.vtable,
                    handle: new_handle,
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        #async_trait_attr
        impl #trait_name for #foreign_name {
            #(#foreign_impls)*
        }

        #[cfg(not(target_arch = "wasm32"))]
        static #vtable_static: std::sync::atomic::AtomicPtr<#vtable_name> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #register_fn(vtable: *const #vtable_name) {
            #vtable_static.store(vtable as *mut _, std::sync::atomic::Ordering::Release);
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn #create_fn(handle: u64) -> ::boltffi::__private::CallbackHandle {
            let vtable = #vtable_static.load(std::sync::atomic::Ordering::Acquire);
            if vtable.is_null() {
                return ::boltffi::__private::CallbackHandle::NULL;
            }
            ::boltffi::__private::CallbackHandle::new(handle, vtable as *const std::ffi::c_void)
        }

        #[cfg(target_arch = "wasm32")]
        #[link(wasm_import_module = "env")]
        unsafe extern "C" {
            fn #wasm_free_import(handle: u32);
            fn #wasm_clone_import(handle: u32) -> u32;
            #(#wasm_extern_imports)*
        }

        #[cfg(target_arch = "wasm32")]
        impl Drop for #foreign_name {
            fn drop(&mut self) {
                unsafe { #wasm_free_import(self.handle) };
            }
        }

        #[cfg(target_arch = "wasm32")]
        impl Clone for #foreign_name {
            fn clone(&self) -> Self {
                let new_handle = unsafe { #wasm_clone_import(self.handle) };
                Self { handle: new_handle }
            }
        }

        #[cfg(target_arch = "wasm32")]
        #async_trait_attr
        impl #trait_name for #foreign_name {
            #(#wasm_impl_bodies)*
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn #wasm_create_fn(js_handle: u32) -> u32 {
            js_handle
        }

        #(#wasm_complete_exports)*
    };

    let concrete_impl = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        impl ::boltffi::__private::FromCallbackHandle for #foreign_name {
            unsafe fn arc_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> std::sync::Arc<Self> {
                debug_assert!(!handle.is_null());
                std::sync::Arc::new(Self {
                    vtable: handle.vtable() as *const #vtable_name,
                    handle: handle.handle(),
                })
            }

            unsafe fn box_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> Box<Self> {
                debug_assert!(!handle.is_null());
                Box::new(Self {
                    vtable: handle.vtable() as *const #vtable_name,
                    handle: handle.handle(),
                })
            }
        }

        #[cfg(target_arch = "wasm32")]
        impl ::boltffi::__private::FromCallbackHandle for #foreign_name {
            unsafe fn arc_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> std::sync::Arc<Self> {
                debug_assert!(!handle.is_null());
                std::sync::Arc::new(Self {
                    handle: handle.handle() as u32,
                })
            }

            unsafe fn box_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> Box<Self> {
                debug_assert!(!handle.is_null());
                Box::new(Self {
                    handle: handle.handle() as u32,
                })
            }
        }
    };

    let dyn_impl = if is_object_safe {
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            impl ::boltffi::__private::FromCallbackHandle for dyn #trait_name {
                unsafe fn arc_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> std::sync::Arc<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        vtable: handle.vtable() as *const #vtable_name,
                        handle: handle.handle(),
                    };
                    std::sync::Arc::new(foreign)
                }

                unsafe fn box_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> Box<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        vtable: handle.vtable() as *const #vtable_name,
                        handle: handle.handle(),
                    };
                    Box::new(foreign)
                }
            }

            #[cfg(target_arch = "wasm32")]
            impl ::boltffi::__private::FromCallbackHandle for dyn #trait_name {
                unsafe fn arc_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> std::sync::Arc<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        handle: handle.handle() as u32,
                    };
                    std::sync::Arc::new(foreign)
                }

                unsafe fn box_from_callback_handle(handle: ::boltffi::__private::CallbackHandle) -> Box<Self> {
                    debug_assert!(!handle.is_null());
                    let foreign = #foreign_name {
                        handle: handle.handle() as u32,
                    };
                    Box::new(foreign)
                }
            }
        }
    } else {
        quote! {}
    };

    let foreign_type_impl = if is_object_safe {
        quote! {
            impl ::boltffi::__private::CallbackForeignType for dyn #trait_name {
                type Foreign = #foreign_name;
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #expanded
        #concrete_impl
        #dyn_impl
        #foreign_type_impl
    })
}

fn expand_method(
    method: &syn::TraitItemFn,
    vtable_fields: &mut Vec<proc_macro2::TokenStream>,
    custom_types: &custom_types::CustomTypeRegistry,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let method_name = &method.sig.ident;
    let method_name_snake = to_snake_case_ident(&method_name.to_string());
    let is_async = method.sig.asyncness.is_some();

    let (param_types, param_names, call_args, prelude_stmts) = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
                Pat::Ident(pat_ident) => Some((pat_ident.ident.clone(), pat_type.ty.clone())),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .map(|(param_name, param_type)| lower_callback_param(&param_name, &param_type, custom_types))
        .fold((Vec::new(), Vec::new(), Vec::new(), Vec::new()), |(mut ffi, mut rust, mut call, mut preludes), lowering| {
            ffi.push(lowering.ffi_param);
            rust.push(lowering.rust_param);
            call.push(lowering.call_arg);
            lowering.prelude.into_iter().for_each(|stmt| preludes.push(stmt));
            (ffi, rust, call, preludes)
        });

    let return_type = match &method.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty.clone()),
    };

    if is_async {
        let async_wire_return = return_type.as_deref().map(needs_wire_return).unwrap_or(false);

        let callback_type = if let Some(ref ret_ty) = return_type {
            if async_wire_return {
                quote! { extern "C" fn(callback_data: u64, result_ptr: *const u8, result_len: usize, status: ::boltffi::__private::FfiStatus) }
            } else {
                let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                quote! { extern "C" fn(callback_data: u64, result: #ffi_ret, status: ::boltffi::__private::FfiStatus) }
            }
        } else {
            quote! { extern "C" fn(callback_data: u64, status: ::boltffi::__private::FfiStatus) }
        };

        vtable_fields.push(quote! {
            pub #method_name_snake: extern "C" fn(
                handle: u64,
                #(#param_types,)*
                callback: #callback_type,
                callback_data: u64
            )
        });

        let output_type = return_type.as_ref().map(|t| quote! { -> #t }).unwrap_or_default();

        let impl_body = match return_type.as_deref() {
            Some(ret_ty) => {
                let error_expr = parse_result_type(ret_ty).map(|(_, err_ty)| {
                    quote! {
                        Err(<#err_ty as ::core::convert::From<::boltffi::UnexpectedFfiCallbackError>>::from(
                            ::boltffi::UnexpectedFfiCallbackError::new(error_msg)
                        ))
                    }
                });

                let (callback_body, poll_body) = if async_wire_return {
                    let poll_error_branch = error_expr
                        .clone()
                        .map(|expr| {
                            quote! {
                                if status.is_err() {
                                    let error_msg: String = ::boltffi::__private::wire::decode(&bytes)
                                        .unwrap_or_else(|_| "unknown callback error".into());
                                    return std::task::Poll::Ready(#expr);
                                }
                            }
                        })
                        .unwrap_or_default();

                    (
                        quote! {
                            extern "C" fn callback(data: u64, result_ptr: *const u8, result_len: usize, status: ::boltffi::__private::FfiStatus) {
                                let bytes = unsafe { ::core::slice::from_raw_parts(result_ptr, result_len) }.to_vec();
                                let ctx = unsafe { Arc::from_raw(data as *const AsyncContext) };
                                let waker = ctx
                                    .state
                                    .lock()
                                    .ok()
                                    .and_then(|mut guard| {
                                        guard.result_bytes = Some(bytes);
                                        guard.status = status;
                                        guard.waker.take()
                                    });
                                if let Some(waker) = waker {
                                    waker.wake();
                                }
                            }
                        },
                        quote! {
                            std::future::poll_fn(move |cx| {
                                let mut guard = ctx.state.lock().expect("async callback mutex poisoned");
                                if let Some(bytes) = guard.result_bytes.take() {
                                    let status = guard.status;
                                    #poll_error_branch
                                    let result: #ret_ty = ::boltffi::__private::wire::decode(&bytes)
                                        .expect("wire decode async callback return");
                                    std::task::Poll::Ready(result)
                                } else {
                                    guard.waker = Some(cx.waker().clone());
                                    std::task::Poll::Pending
                                }
                            }).await
                        },
                    )
                } else {
                    let poll_error_branch = error_expr
                        .map(|expr| {
                            quote! {
                                if status.is_err() {
                                    let error_msg = "callback returned error status".to_string();
                                    return std::task::Poll::Ready(#expr);
                                }
                            }
                        })
                        .unwrap_or_default();

                    (
                        quote! {
                            extern "C" fn callback(data: u64, result: #ret_ty, status: ::boltffi::__private::FfiStatus) {
                                let ctx = unsafe { Arc::from_raw(data as *const AsyncContext<#ret_ty>) };
                                let waker = ctx
                                    .state
                                    .lock()
                                    .ok()
                                    .and_then(|mut guard| {
                                        guard.result = Some(result);
                                        guard.status = status;
                                        guard.waker.take()
                                    });
                                if let Some(waker) = waker {
                                    waker.wake();
                                }
                            }
                        },
                        quote! {
                            std::future::poll_fn(move |cx| {
                                let mut guard = ctx.state.lock().expect("async callback mutex poisoned");
                                if let Some(result) = guard.result.take() {
                                    let status = guard.status;
                                    #poll_error_branch
                                    std::task::Poll::Ready(result)
                                } else {
                                    guard.waker = Some(cx.waker().clone());
                                    std::task::Poll::Pending
                                }
                            }).await
                        },
                    )
                };

                let async_state = if async_wire_return {
                    quote! {
                        struct AsyncState {
                            result_bytes: Option<Vec<u8>>,
                            status: ::boltffi::__private::FfiStatus,
                            waker: Option<Waker>,
                        }

                        struct AsyncContext {
                            state: Mutex<AsyncState>,
                        }

                        let ctx = Arc::new(AsyncContext {
                            state: Mutex::new(AsyncState {
                                result_bytes: None,
                                status: ::boltffi::__private::FfiStatus::OK,
                                waker: None,
                            }),
                        });
                    }
                } else {
                    quote! {
                        struct AsyncState<T> {
                            result: Option<T>,
                            status: ::boltffi::__private::FfiStatus,
                            waker: Option<Waker>,
                        }

                        struct AsyncContext<T> {
                            state: Mutex<AsyncState<T>>,
                        }

                        let ctx = Arc::new(AsyncContext::<#ret_ty> {
                            state: Mutex::new(AsyncState {
                                result: None,
                                status: ::boltffi::__private::FfiStatus::OK,
                                waker: None,
                            }),
                        });
                    }
                };

                quote! {
                    use std::sync::{Arc, Mutex};
                    use std::task::Waker;

                    #async_state

                    #callback_body

                    let ctx_ptr = Arc::into_raw(Arc::clone(&ctx)) as u64;
                    {
                        #(#prelude_stmts)*
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                callback,
                                ctx_ptr
                            );
                        }
                    }

                    #poll_body
                }
            }
            None => quote! {
                use std::sync::{Arc, Mutex};
                use std::task::Waker;

                struct AsyncState {
                    completed: bool,
                    status: ::boltffi::__private::FfiStatus,
                    waker: Option<Waker>,
                }

                struct AsyncContext {
                    state: Mutex<AsyncState>,
                }

                let ctx = Arc::new(AsyncContext {
                    state: Mutex::new(AsyncState {
                        completed: false,
                        status: ::boltffi::__private::FfiStatus::OK,
                        waker: None,
                    }),
                });

                extern "C" fn callback(data: u64, status: ::boltffi::__private::FfiStatus) {
                    let ctx = unsafe { Arc::from_raw(data as *const AsyncContext) };
                    let waker = ctx
                        .state
                        .lock()
                        .ok()
                        .and_then(|mut guard| {
                            guard.completed = true;
                            guard.status = status;
                            guard.waker.take()
                        });
                    if let Some(waker) = waker {
                        waker.wake();
                    }
                }

                let ctx_ptr = Arc::into_raw(Arc::clone(&ctx)) as u64;
                {
                    #(#prelude_stmts)*
                    unsafe {
                        ((*self.vtable).#method_name_snake)(
                            self.handle,
                            #(#call_args,)*
                            callback,
                            ctx_ptr
                        );
                    }
                }

                std::future::poll_fn(move |cx| {
                    let mut guard = ctx.state.lock().expect("async callback mutex poisoned");
                    if guard.completed {
                        std::task::Poll::Ready(())
                    } else {
                        guard.waker = Some(cx.waker().clone());
                        std::task::Poll::Pending
                    }
                }).await
            },
        };

        Ok(quote! {
            async fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        })
    } else {
        let wire_return = return_type.as_deref().map(needs_wire_return).unwrap_or(false);

        let out_params = if let Some(ref ret_ty) = return_type {
            if wire_return {
                quote! { out_ptr: *mut u8, out_len: *mut usize, }
            } else {
                let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
                quote! { out: *mut #ffi_ret, }
            }
        } else {
            quote! {}
        };

        vtable_fields.push(quote! {
            pub #method_name_snake: extern "C" fn(
                handle: u64,
                #(#param_types,)*
                #out_params
                status: *mut ::boltffi::__private::FfiStatus
            )
        });

        let output_type = return_type.as_ref().map(|t| quote! { -> #t }).unwrap_or_default();

        let impl_body = match return_type.as_deref() {
            Some(ret_ty) => {
                let error_expr = parse_result_type(ret_ty).map(|(_, err_ty)| {
                    quote! {
                        return Err(<#err_ty as ::core::convert::From<::boltffi::UnexpectedFfiCallbackError>>::from(
                            ::boltffi::UnexpectedFfiCallbackError::new("sync callback returned error status")
                        ));
                    }
                });

                if wire_return {
                    quote! {
                        #(#prelude_stmts)*
                        const CALLBACK_BUF_SIZE: usize = 4096;
                        let mut out_buf: [u8; CALLBACK_BUF_SIZE] = [0u8; CALLBACK_BUF_SIZE];
                        let mut out_len: usize = 0;
                        let mut status = ::boltffi::__private::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                out_buf.as_mut_ptr(),
                                &mut out_len,
                                &mut status
                            );
                        }
                        if status.is_err() {
                            #error_expr
                        }
                        let out_bytes = &out_buf[..out_len];
                        ::boltffi::__private::wire::decode(out_bytes).expect("wire decode callback return")
                    }
                } else {
                    quote! {
                        #(#prelude_stmts)*
                        let mut out: #ret_ty = Default::default();
                        let mut status = ::boltffi::__private::FfiStatus::default();
                        unsafe {
                            ((*self.vtable).#method_name_snake)(
                                self.handle,
                                #(#call_args,)*
                                &mut out as *mut _,
                                &mut status
                            );
                        }
                        if status.is_err() {
                            #error_expr
                        }
                        out
                    }
                }
            }
            None => quote! {
                #(#prelude_stmts)*
                let mut status = ::boltffi::__private::FfiStatus::default();
                unsafe {
                    ((*self.vtable).#method_name_snake)(
                        self.handle,
                        #(#call_args,)*
                        &mut status
                    );
                }
            },
        };

        Ok(quote! {
            fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        })
    }
}

fn to_snake_case_ident(name: &str) -> syn::Ident {
    syn::Ident::new(&naming::to_snake_case(name), proc_macro2::Span::call_site())
}

fn rust_type_to_ffi_param_type(ty: &syn::Type) -> proc_macro2::TokenStream {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    match type_str.as_str() {
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "usize" | "isize" => quote!(#ty),
        "&str" => quote!(*const std::os::raw::c_char),
        "String" => quote!(*const std::os::raw::c_char),
        _ => quote!(#ty),
    }
}

struct CallbackParamLowering {
    ffi_param: proc_macro2::TokenStream,
    rust_param: proc_macro2::TokenStream,
    call_arg: proc_macro2::TokenStream,
    prelude: Option<proc_macro2::TokenStream>,
}

fn lower_callback_param(param_name: &syn::Ident, param_type: &syn::Type, custom_types: &custom_types::CustomTypeRegistry) -> CallbackParamLowering {
    let rust_param = quote! { #param_name: #param_type };

    let type_str = quote!(#param_type).to_string().replace(' ', "");
    if is_ffi_primitive(&type_str) {
        return CallbackParamLowering {
            ffi_param: quote! { #param_name: #param_type },
            rust_param,
            call_arg: quote! { #param_name },
            prelude: None,
        };
    }

    let ptr_name = format_ident!("{}_ptr", param_name);
    let len_name = format_ident!("{}_len", param_name);
    let wire_name = format_ident!("{}_wire", param_name);

    let prelude = if custom_types::contains_custom_types(param_type, custom_types) {
        let wire_ty = custom_types::wire_type_for(param_type, custom_types);
        let wire_value_name = format_ident!("{}_wire_value", param_name);
        let to_wire = custom_types::to_wire_expr_owned(param_type, custom_types, param_name);
        quote! {
            let #wire_value_name: #wire_ty = { #to_wire };
            let #wire_name = ::boltffi::__private::wire::encode(&#wire_value_name);
        }
    } else {
        quote! { let #wire_name = ::boltffi::__private::wire::encode(&#param_name); }
    };

    CallbackParamLowering {
        ffi_param: quote! { #ptr_name: *const u8, #len_name: usize },
        rust_param,
        call_arg: quote! { #wire_name.as_ptr(), #wire_name.len() },
        prelude: Some(prelude),
    }
}

fn is_ffi_primitive(type_str: &str) -> bool {
    matches!(
        type_str,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "bool" | "usize" | "isize"
    )
}

fn needs_wire_return(ty: &syn::Type) -> bool {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    !is_ffi_primitive(&type_str)
}

fn parse_result_type(ty: &Type) -> Option<(Type, Type)> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let result_segment = type_path.path.segments.last()?;
    if result_segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &result_segment.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    });
    let ok = types.next()?;
    let err = types.next()?;
    Some((ok, err))
}

struct WasmMethodExpansion {
    extern_import: proc_macro2::TokenStream,
    impl_body: proc_macro2::TokenStream,
    complete_export: Option<proc_macro2::TokenStream>,
}

fn expand_method_wasm(method: &syn::TraitItemFn, trait_name_snake: &syn::Ident, custom_types: &custom_types::CustomTypeRegistry) -> Result<WasmMethodExpansion, syn::Error> {
    let method_name = &method.sig.ident;
    let method_name_snake = to_snake_case_ident(&method_name.to_string());

    let is_async = method.sig.asyncness.is_some();
    if is_async {
        return expand_method_wasm_async(method, trait_name_snake, custom_types);
    }

    let import_name = format_ident!("__boltffi_callback_{}_{}", trait_name_snake, method_name_snake);

    let (ffi_param_types, param_names, call_args, prelude_stmts): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
                Pat::Ident(pat_ident) => Some((pat_ident.ident.clone(), pat_type.ty.clone())),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .map(|(param_name, param_type)| lower_callback_param_wasm(&param_name, &param_type, custom_types))
        .fold((Vec::new(), Vec::new(), Vec::new(), Vec::new()), |(mut ffi, mut rust, mut call, mut preludes), lowering| {
            for p in lowering.ffi_params {
                ffi.push(p);
            }
            rust.push(lowering.rust_param);
            for a in lowering.call_args {
                call.push(a);
            }
            if let Some(stmt) = lowering.prelude {
                preludes.push(stmt);
            }
            (ffi, rust, call, preludes)
        });

    let return_type = match &method.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty.clone()),
    };

    let wire_return = return_type.as_deref().map(needs_wire_return).unwrap_or(false);

    let (extern_import, impl_body) = if let Some(ref ret_ty) = return_type {
        if wire_return {
            (
                quote! {
                    fn #import_name(
                        handle: u32,
                        out_buf_ptr: *mut ::boltffi::__private::WasmCallbackOutBuf,
                        #(#ffi_param_types),*
                    );
                },
                quote! {
                    #(#prelude_stmts)*
                    let mut out_buf = ::boltffi::__private::WasmCallbackOutBuf::empty();
                    unsafe {
                        #import_name(
                            self.handle,
                            &mut out_buf as *mut _,
                            #(#call_args),*
                        );
                    }
                    let out_bytes = unsafe { out_buf.as_slice() };
                    ::boltffi::__private::wire::decode(out_bytes)
                        .expect("wire decode wasm callback return")
                },
            )
        } else {
            let ffi_ret = rust_type_to_ffi_param_type(ret_ty);
            (
                quote! {
                    fn #import_name(handle: u32, #(#ffi_param_types),*) -> #ffi_ret;
                },
                quote! {
                    #(#prelude_stmts)*
                    unsafe { #import_name(self.handle, #(#call_args),*) }
                },
            )
        }
    } else {
        (
            quote! {
                fn #import_name(handle: u32, #(#ffi_param_types),*);
            },
            quote! {
                #(#prelude_stmts)*
                unsafe { #import_name(self.handle, #(#call_args),*) }
            },
        )
    };

    let output_type = return_type.as_ref().map(|t| quote! { -> #t }).unwrap_or_default();

    Ok(WasmMethodExpansion {
        extern_import,
        impl_body: quote! {
            fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        },
        complete_export: None,
    })
}

fn expand_method_wasm_async(method: &syn::TraitItemFn, trait_name_snake: &syn::Ident, custom_types: &custom_types::CustomTypeRegistry) -> Result<WasmMethodExpansion, syn::Error> {
    let method_name = &method.sig.ident;
    let method_name_snake = to_snake_case_ident(&method_name.to_string());

    let start_import_name = format_ident!("__boltffi_callback_{}_{}_start", trait_name_snake, method_name_snake);
    let complete_export_name = format_ident!("boltffi_callback_{}_{}_complete", trait_name_snake, method_name_snake);

    let (ffi_param_types, param_names, call_args, prelude_stmts): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
                Pat::Ident(pat_ident) => Some((pat_ident.ident.clone(), pat_type.ty.clone())),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
        .map(|(param_name, param_type)| lower_callback_param_wasm(&param_name, &param_type, custom_types))
        .fold((Vec::new(), Vec::new(), Vec::new(), Vec::new()), |(mut ffi, mut rust, mut call, mut preludes), lowering| {
            for p in lowering.ffi_params {
                ffi.push(p);
            }
            rust.push(lowering.rust_param);
            for a in lowering.call_args {
                call.push(a);
            }
            if let Some(stmt) = lowering.prelude {
                preludes.push(stmt);
            }
            (ffi, rust, call, preludes)
        });

    let return_type = match &method.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty.clone()),
    };

    let extern_import = quote! {
        fn #start_import_name(handle: u32, request_id: u32, #(#ffi_param_types),*);
    };

    let complete_export = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #complete_export_name(
            request_id: u32,
            completion_code: i32,
            data_ptr: u32,
            data_len: u32,
            data_cap: u32,
        ) -> i32 {
            ::boltffi::__private::complete_request_from_ffi(
                request_id,
                completion_code,
                data_ptr,
                data_len,
                data_cap,
            )
        }
    };

    let output_type = return_type.as_ref().map(|t| quote! { -> #t }).unwrap_or_default();

    let poll_body = match return_type.as_deref() {
        Some(ret_ty) => {
            if let Some((ok_ty, err_ty)) = parse_result_type(ret_ty) {
                quote! {
                    std::future::poll_fn(move |cx| {
                        ::boltffi::__private::set_request_waker(request_id, cx.waker().clone());
                        match ::boltffi::__private::take_request_result(request_id) {
                            Some(result) => {
                                if !result.code.is_success() {
                                    let error_msg = if result.data.is_empty() {
                                        "async callback failed".to_string()
                                    } else {
                                        ::boltffi::__private::wire::decode::<String>(&result.data)
                                            .unwrap_or_else(|_| "async callback failed".to_string())
                                    };
                                    return std::task::Poll::Ready(Err(
                                        <#err_ty as ::core::convert::From<::boltffi::UnexpectedFfiCallbackError>>::from(
                                            ::boltffi::UnexpectedFfiCallbackError::new(error_msg)
                                        )
                                    ));
                                }
                                let ok_value: #ok_ty = ::boltffi::__private::wire::decode(&result.data)
                                    .expect("wire decode async callback return");
                                std::task::Poll::Ready(Ok(ok_value))
                            }
                            None => std::task::Poll::Pending,
                        }
                    }).await
                }
            } else {
                quote! {
                    std::future::poll_fn(move |cx| {
                        ::boltffi::__private::set_request_waker(request_id, cx.waker().clone());
                        match ::boltffi::__private::take_request_result(request_id) {
                            Some(result) => {
                                if !result.code.is_success() {
                                    let error_msg = if result.data.is_empty() {
                                        "async callback failed".to_string()
                                    } else {
                                        ::boltffi::__private::wire::decode::<String>(&result.data)
                                            .unwrap_or_else(|_| "async callback failed".to_string())
                                    };
                                    panic!("async callback failed: {}", error_msg);
                                }
                                let value: #ret_ty = ::boltffi::__private::wire::decode(&result.data)
                                    .expect("wire decode async callback return");
                                std::task::Poll::Ready(value)
                            }
                            None => std::task::Poll::Pending,
                        }
                    }).await
                }
            }
        }
        None => {
            quote! {
                std::future::poll_fn(move |cx| {
                    ::boltffi::__private::set_request_waker(request_id, cx.waker().clone());
                    match ::boltffi::__private::take_request_result(request_id) {
                        Some(result) => {
                            if !result.code.is_success() {
                                let error_msg = if result.data.is_empty() {
                                    "async callback failed".to_string()
                                } else {
                                    ::boltffi::__private::wire::decode::<String>(&result.data)
                                        .unwrap_or_else(|_| "async callback failed".to_string())
                                };
                                panic!("async callback failed: {}", error_msg);
                            }
                            std::task::Poll::Ready(())
                        }
                        None => std::task::Poll::Pending,
                    }
                }).await
            }
        }
    };

    let impl_body = quote! {
        let request_id = ::boltffi::__private::allocate_request();
        let _guard = ::boltffi::__private::RequestGuard(request_id);
        {
            #(#prelude_stmts)*
            unsafe {
                #start_import_name(
                    self.handle,
                    request_id.as_u32(),
                    #(#call_args),*
                );
            }
        }
        #poll_body
    };

    Ok(WasmMethodExpansion {
        extern_import,
        impl_body: quote! {
            async fn #method_name(&self, #(#param_names,)*) #output_type {
                #impl_body
            }
        },
        complete_export: Some(complete_export),
    })
}

struct WasmCallbackParamLowering {
    ffi_params: Vec<proc_macro2::TokenStream>,
    rust_param: proc_macro2::TokenStream,
    call_args: Vec<proc_macro2::TokenStream>,
    prelude: Option<proc_macro2::TokenStream>,
}

fn lower_callback_param_wasm(param_name: &syn::Ident, param_type: &syn::Type, custom_types: &custom_types::CustomTypeRegistry) -> WasmCallbackParamLowering {
    let rust_param = quote! { #param_name: #param_type };
    let type_str = quote!(#param_type).to_string().replace(' ', "");

    if is_ffi_primitive(&type_str) {
        return WasmCallbackParamLowering {
            ffi_params: vec![quote! { #param_name: #param_type }],
            rust_param,
            call_args: vec![quote! { #param_name }],
            prelude: None,
        };
    }

    let ptr_name = format_ident!("{}_ptr", param_name);
    let len_name = format_ident!("{}_len", param_name);
    let wire_name = format_ident!("{}_wire", param_name);

    let prelude = if custom_types::contains_custom_types(param_type, custom_types) {
        let wire_ty = custom_types::wire_type_for(param_type, custom_types);
        let wire_value_name = format_ident!("{}_wire_value", param_name);
        let to_wire = custom_types::to_wire_expr_owned(param_type, custom_types, param_name);
        quote! {
            let #wire_value_name: #wire_ty = { #to_wire };
            let #wire_name = ::boltffi::__private::wire::encode(&#wire_value_name);
        }
    } else {
        quote! { let #wire_name = ::boltffi::__private::wire::encode(&#param_name); }
    };

    WasmCallbackParamLowering {
        ffi_params: vec![quote! { #ptr_name: *const u8 }, quote! { #len_name: u32 }],
        rust_param,
        call_args: vec![quote! { #wire_name.as_ptr() }, quote! { #wire_name.len() as u32 }],
        prelude: Some(prelude),
    }
}
