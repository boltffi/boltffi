use crate::{
    callback_registry, custom_types,
    params::{FfiParams, transform_params, transform_params_async},
    returns::{ReturnAbi, classify_return, encoded_return_body, lower_return_abi},
    safety,
};
use boltffi_ffi_rules::{naming, transport::EncodedReturnStrategy};
use proc_macro::TokenStream;
use quote::quote;
use syn::ItemFn;

fn build_encoded_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    encode_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> u64 {
                let __boltffi_buf: ::boltffi::__private::FfiBuf<u8> = { #encode_body };
                __boltffi_buf.into_packed()
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> u64 {
                let __boltffi_buf: ::boltffi::__private::FfiBuf<u8> = { #encode_body };
                __boltffi_buf.into_packed()
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiBuf<u8> {
                #encode_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf<u8> {
                #encode_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

fn build_f64_wasm_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> f64 {
                #wasm_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> f64 {
                #wasm_body
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiBuf<u8> {
                #native_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf<u8> {
                #native_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

fn build_void_wasm_return_exports(
    input: &ItemFn,
    fn_vis: &syn::Visibility,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() {
                #wasm_body
            }
        },
        false => quote! {
            #[cfg(target_arch = "wasm32")]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) {
                #wasm_body
            }
        },
    };

    let non_wasm_export = match ffi_params.is_empty() {
        true => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiBuf<u8> {
                #native_body
            }
        },
        false => quote! {
            #[cfg(not(target_arch = "wasm32"))]
            #[unsafe(no_mangle)]
            #fn_vis unsafe extern "C" fn #export_ident(
                #(#ffi_params),*
            ) -> ::boltffi::__private::FfiBuf<u8> {
                #native_body
            }
        },
    };

    TokenStream::from(quote! {
        #input

        #wasm_export
        #non_wasm_export
    })
}

pub fn ffi_export_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);

    let violations = safety::scan_function(&input);
    if !violations.is_empty() {
        return TokenStream::from(safety::violations_to_compile_errors(&violations));
    }

    let custom_types = match custom_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let callback_registry = match callback_registry::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };

    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let is_async = input.sig.asyncness.is_some();

    if is_async {
        return generate_async_export(&input, &custom_types, &callback_registry);
    }

    let export_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(fn_inputs, &custom_types, &callback_registry);

    let has_params = !ffi_params.is_empty();
    let has_conversions = !conversions.is_empty();

    let return_abi = lower_return_abi(classify_return(fn_output));

    let expanded = match return_abi {
        ReturnAbi::Unit => {
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*);
                    ::boltffi::__private::FfiStatus::OK
                }
            } else {
                quote! {
                    #fn_name(#(#call_args),*);
                    ::boltffi::__private::FfiStatus::OK
                }
            };

            if has_params {
                quote! {
                        #input

                        #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                        #fn_vis unsafe extern "C" fn #export_ident(
                            #(#ffi_params),*
                        ) -> ::boltffi::__private::FfiStatus {
                            #body
                        }
                    }
            } else {
                quote! {
                        #input

                        #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                        #fn_vis extern "C" fn #export_ident() -> ::boltffi::__private::FfiStatus {
                            #body
                        }
                    }
            }
        }
        ReturnAbi::Scalar { .. } => {
            let fn_output = &input.sig.output;
            let body = if has_conversions {
                quote! {
                    #(#conversions)*
                    #fn_name(#(#call_args),*)
                }
            } else {
                quote! { #fn_name(#(#call_args),*) }
            };

            if has_params {
                quote! {
                        #input

                        #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                        #fn_vis unsafe extern "C" fn #export_ident(
                            #(#ffi_params),*
                        ) #fn_output {
                            #body
                        }
                    }
            } else {
                quote! {
                        #input

                        #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                        #fn_vis extern "C" fn #export_ident() #fn_output {
                            #body
                        }
                    }
            }
        }
        ReturnAbi::Encoded { rust_type: inner_ty, strategy } => {
            let result_ident = syn::Ident::new("result", fn_name.span());

            if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
                let call_and_bind = if has_conversions {
                    quote! {
                        #(#conversions)*
                        let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    }
                } else {
                    quote! {
                        let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    }
                };

                let wasm_body = quote! {
                    #call_and_bind
                    match #result_ident {
                        Some(v) => v as f64,
                        None => f64::NAN,
                    }
                };

                let native_body = quote! {
                    #call_and_bind
                    ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
                };

                return build_f64_wasm_return_exports(&input, fn_vis, &export_ident, &ffi_params, wasm_body, native_body);
            }

            if matches!(strategy, EncodedReturnStrategy::PrimitiveVec) {
                let call_and_bind = if has_conversions {
                    quote! {
                        #(#conversions)*
                        let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    }
                } else {
                    quote! {
                        let #result_ident: #inner_ty = #fn_name(#(#call_args),*);
                    }
                };

                let wasm_body = quote! {
                    #call_and_bind
                    let __buf = ::boltffi::__private::FfiBuf::from_vec(#result_ident);
                    let __byte_len = __buf.len() * core::mem::size_of::<<#inner_ty as IntoIterator>::Item>();
                    ::boltffi::__private::write_return_slot(__buf.as_ptr() as u32, __byte_len as u32);
                    core::mem::forget(__buf);
                };

                let native_body = quote! {
                    #call_and_bind
                    ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
                };

                return build_void_wasm_return_exports(&input, fn_vis, &export_ident, &ffi_params, wasm_body, native_body);
            }

            let encode_body = encoded_return_body(&inner_ty, strategy, &result_ident, quote! { #fn_name(#(#call_args),*) }, &conversions, &custom_types);

            return build_encoded_return_exports(&input, fn_vis, &export_ident, &ffi_params, encode_body);
        }
    };

    TokenStream::from(expanded)
}

fn generate_async_export(input: &ItemFn, custom_types: &custom_types::CustomTypeRegistry, callback_registry: &callback_registry::CallbackTraitRegistry) -> TokenStream {
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;
    let fn_block = &input.block;

    let base_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let entry_ident = syn::Ident::new(&base_name, fn_name.span());
    let poll_ident = syn::Ident::new(&format!("{}_poll", base_name), fn_name.span());
    let poll_sync_ident = syn::Ident::new(&format!("{}_poll_sync", base_name), fn_name.span());
    let complete_ident = syn::Ident::new(&format!("{}_complete", base_name), fn_name.span());
    let panic_message_ident = syn::Ident::new(&format!("{}_panic_message", base_name), fn_name.span());
    let cancel_ident = syn::Ident::new(&format!("{}_cancel", base_name), fn_name.span());
    let free_ident = syn::Ident::new(&format!("{}_free", base_name), fn_name.span());

    let params = match transform_params_async(fn_inputs, custom_types, callback_registry) {
        Ok(params) => params,
        Err(error) => return error.to_compile_error().into(),
    };
    let return_abi = ReturnAbi::from_output(fn_output);

    let ffi_return_type = return_abi.async_ffi_return_type();
    let rust_return_type = return_abi.async_rust_return_type();
    let complete_conversion = return_abi.async_complete_conversion();
    let default_value = return_abi.async_default_ffi_value();

    let ffi_params = &params.ffi_params;
    let pre_spawn = &params.pre_spawn;
    let thread_setup = &params.thread_setup;
    let call_args = &params.call_args;
    let move_vars = &params.move_vars;

    let future_body = quote! {
        #(#thread_setup)*
        #fn_name(#(#call_args),*).await
    };

    let entry_fn = if ffi_params.is_empty() {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident() -> ::boltffi::__private::RustFutureHandle {
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    } else {
        quote! {
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #entry_ident(#(#ffi_params),*) -> ::boltffi::__private::RustFutureHandle {
                #(#pre_spawn)*
                #(let _ = &#move_vars;)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #future_body
                })
            }
        }
    };

    let native_complete_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #complete_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            out_status: *mut ::boltffi::__private::FfiStatus,
        ) -> #ffi_return_type {
            match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                Some(result) => { #complete_conversion }
                None => {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::CANCELLED; }
                    #default_value
                }
            }
        }
    };

    let wasm_complete_fn = match &return_abi {
        ReturnAbi::Scalar { rust_type } => {
            quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #complete_ident(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) -> #rust_type {
                    match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                        Some(result) => result,
                        None => Default::default(),
                    }
                }
            }
        }
        ReturnAbi::Unit => {
            quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #complete_ident(
                    handle: ::boltffi::__private::RustFutureHandle,
                ) {
                    let _ = ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle);
                }
            }
        }
        _ => {
            quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #complete_ident(
                    out: *mut ::boltffi::__private::FfiBuf<u8>,
                    handle: ::boltffi::__private::RustFutureHandle,
                    _out_status: *mut ::boltffi::__private::FfiStatus,
                ) {
                    if out.is_null() {
                        return;
                    }
                    let buf = match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                        Some(result) => ::boltffi::__private::FfiBuf::wire_encode(&result),
                        None => ::boltffi::__private::FfiBuf::empty(),
                    };
                    out.write(buf);
                }
            }
        }
    };

    let native_poll_fn = quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #poll_ident(
            handle: ::boltffi::__private::RustFutureHandle,
            callback_data: u64,
            callback: ::boltffi::__private::RustFutureContinuationCallback,
        ) {
            ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(handle, callback, callback_data)
        }
    };

    let wasm_poll_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #poll_sync_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> i32 {
            ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
        }
    };

    let wasm_panic_message_fn = quote! {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #panic_message_ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> ::boltffi::__private::FfiBuf<u8> {
            match ::boltffi::__private::rust_future_panic_message::<#rust_return_type>(handle) {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    };

    let expanded = quote! {
        #fn_vis async fn #fn_name(#fn_inputs) #fn_output #fn_block

        #entry_fn

        #native_poll_fn
        #wasm_poll_fn
        #wasm_panic_message_fn

        #native_complete_fn
        #wasm_complete_fn

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #cancel_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
        }

        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #free_ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
        }
    };

    TokenStream::from(expanded)
}
