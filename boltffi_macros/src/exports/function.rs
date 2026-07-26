use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::naming;
use boltffi_ffi_rules::transport::{EncodedReturnStrategy, ValueReturnStrategy};
use proc_macro::TokenStream;
use quote::quote;
use syn::ItemFn;

use crate::exports::async_export::{
    AsyncExportNames, AsyncRuntimeExports, AsyncWasmCompleteExport,
};
use crate::exports::callable::FunctionCallable;
use crate::exports::callback_return::resolve_sync_callback_return;
use crate::exports::extern_export::{
    DirectBufferCarrier, DualPlatformExternExport, ExportBody, ExportCondition, ExportSafety,
    ExternExport, ReceiverParameter,
};
use crate::index::callback_traits::CallbackTraitRegistry;
use crate::index::{CrateIndex, custom_types, data_types};
use crate::lowering::params::{FfiParams, transform_params, transform_params_async};
use crate::lowering::returns::lower::encoded_return_body;
use crate::lowering::returns::model::{
    ResolvedReturn, ReturnInvocationContext, ReturnLoweringContext, ReturnPlatform,
    WasmOptionScalarEncoding,
};
use crate::safety;

fn build_encoded_return_exports(
    callable: FunctionCallable<'_>,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    resolved_return: &ResolvedReturn,
    encode_body: proc_macro2::TokenStream,
) -> TokenStream {
    let input = callable.item();
    let fn_vis = &input.vis;
    let wasm_return_carrier = DirectBufferCarrier::new(
        resolved_return
            .direct_buffer_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm)
            .unwrap_or_else(|| {
                panic!(
                    "encoded sync export must use a direct wasm buffer return carrier: {:?}",
                    resolved_return.value_return_strategy()
                )
            }),
    );
    let native_return_carrier = DirectBufferCarrier::new(
        resolved_return
            .direct_buffer_return_method(
                ReturnInvocationContext::SyncExport,
                ReturnPlatform::Native,
            )
            .unwrap_or_else(|| {
                panic!(
                    "encoded sync export must use a direct native buffer return carrier: {:?}",
                    resolved_return.value_return_strategy()
                )
            }),
    );
    let wasm_return_type = wasm_return_carrier.return_type();
    let native_return_type = native_return_carrier.return_type();
    let safety = if ffi_params.is_empty() {
        ExportSafety::Safe
    } else {
        ExportSafety::Unsafe
    };
    let export_pair = DualPlatformExternExport {
        wasm: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> #wasm_return_type },
                body: wasm_return_carrier.lower_body(encode_body.clone()),
            },
        },
        native: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> #native_return_type },
                body: native_return_carrier.lower_body(encode_body),
            },
        },
    };
    let rendered_exports = export_pair.render();

    TokenStream::from(quote! {
        #input

        #rendered_exports
    })
}

fn build_f64_wasm_return_exports(
    callable: FunctionCallable<'_>,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let input = callable.item();
    let fn_vis = &input.vis;
    let safety = if ffi_params.is_empty() {
        ExportSafety::Safe
    } else {
        ExportSafety::Unsafe
    };
    let export_pair = DualPlatformExternExport {
        wasm: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> f64 },
                body: wasm_body,
            },
        },
        native: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> ::boltffi::__private::FfiBuf },
                body: native_body,
            },
        },
    };
    let rendered_exports = export_pair.render();

    TokenStream::from(quote! {
        #input

        #rendered_exports
    })
}

fn build_void_wasm_return_exports(
    callable: FunctionCallable<'_>,
    export_ident: &syn::Ident,
    ffi_params: &[proc_macro2::TokenStream],
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
) -> TokenStream {
    let input = callable.item();
    let fn_vis = &input.vis;
    let safety = if ffi_params.is_empty() {
        ExportSafety::Safe
    } else {
        ExportSafety::Unsafe
    };
    let export_pair = DualPlatformExternExport {
        wasm: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! {},
                body: wasm_body,
            },
        },
        native: ExternExport {
            visibility: fn_vis,
            export_name: export_ident,
            safety,
            receiver: ReceiverParameter::None,
            params: ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> ::boltffi::__private::FfiBuf },
                body: native_body,
            },
        },
    };
    let rendered_exports = export_pair.render();

    TokenStream::from(quote! {
        #input

        #rendered_exports
    })
}

fn ffi_export_item_impl(input: ItemFn) -> proc_macro2::TokenStream {
    let violations = safety::scan_function(&input);
    if !violations.is_empty() {
        return safety::violations_to_compile_errors(&violations);
    }
    let callable = FunctionCallable::new(&input);

    let crate_index = match CrateIndex::for_current_crate() {
        Ok(crate_index) => crate_index,
        Err(error) => return error.to_compile_error(),
    };
    let class_types = crate_index.class_types().clone();
    let custom_types = crate_index.custom_types().clone();
    let callback_registry = crate_index.callback_traits().clone();
    let data_types = crate_index.data_types().clone();
    let return_lowering = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);

    let input = callable.item();
    debug_assert_eq!(callable.form(), CallableForm::Function);
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;

    if callable.execution_kind() == ExecutionKind::Async {
        return generate_async_export(callable, &class_types, &custom_types, &callback_registry)
            .into();
    }

    let export_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_ident = syn::Ident::new(&export_name, fn_name.span());

    let sync_callback_return = match resolve_sync_callback_return(fn_output, &callback_registry) {
        Ok(resolved_return) => resolved_return,
        Err(error) => return error.to_compile_error(),
    };
    let return_abi = return_lowering.lower_output(fn_output);

    if let Some(callback_return) = sync_callback_return {
        let native_on_wire_record_error =
            callback_return.native_invalid_arg_early_return_statement();
        let wasm_on_wire_record_error = callback_return.wasm_invalid_arg_early_return_statement();
        let FfiParams {
            ffi_params: native_ffi_params,
            conversions: native_conversions,
            call_args: native_call_args,
        } = transform_params(
            fn_inputs,
            &return_lowering,
            &callback_registry,
            &native_on_wire_record_error,
        );
        let FfiParams {
            ffi_params: wasm_ffi_params,
            conversions: wasm_conversions,
            call_args: wasm_call_args,
        } = transform_params(
            fn_inputs,
            &return_lowering,
            &callback_registry,
            &wasm_on_wire_record_error,
        );
        let native_body = callback_return.lower_native_result_expression(quote! {
            {
                #(#native_conversions)*
                #fn_name(#(#native_call_args),*)
            }
        });
        let wasm_body = callback_return.lower_wasm_result_expression(quote! {
            {
                #(#wasm_conversions)*
                #fn_name(#(#wasm_call_args),*)
            }
        });
        let native_return_type = callback_return.native_ffi_return_type();
        let wasm_return_type = callback_return.wasm_ffi_return_type();

        return if native_ffi_params.is_empty() {
            quote! {
                #input

                #[cfg(target_arch = "wasm32")]
                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis extern "C" fn #export_ident() -> #wasm_return_type {
                    #wasm_body
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis extern "C" fn #export_ident() -> #native_return_type {
                    #native_body
                }
            }
        } else {
            quote! {
                #input

                #[cfg(target_arch = "wasm32")]
                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#wasm_ffi_params),*
                ) -> #wasm_return_type {
                    #wasm_body
                }

                #[cfg(not(target_arch = "wasm32"))]
                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#native_ffi_params),*
                ) -> #native_return_type {
                    #native_body
                }
            }
        };
    }

    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_params(
        fn_inputs,
        &return_lowering,
        &callback_registry,
        &on_wire_record_error,
    );

    let has_params = !ffi_params.is_empty();

    if return_abi.is_unit() {
        let body = quote! {
            #(#conversions)*
            #fn_name(#(#call_args),*);
            ::boltffi::__private::FfiStatus::OK
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
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &input.sig.output;
        let body = quote! {
            #(#conversions)*
            #fn_name(#(#call_args),*)
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
    } else if matches!(
        return_abi.value_return_strategy(),
        ValueReturnStrategy::ObjectHandle
    ) {
        let object_handle = return_abi
            .object_handle()
            .expect("object handle return must carry handle metadata");
        let return_type = object_handle.ffi_return_type();
        let pointer_expression =
            object_handle.raw_pointer_expression(quote! { #fn_name(#(#call_args),*) });
        let body = quote! {
            #(#conversions)*
            #pointer_expression
        };

        if has_params {
            quote! {
                #input

                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#ffi_params),*
                ) #return_type {
                    #body
                }
            }
        } else {
            quote! {
                #input

                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis extern "C" fn #export_ident() #return_type {
                    #body
                }
            }
        }
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", fn_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", fn_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);

            let native_on_error = return_abi.native_invalid_arg_early_return_statement();
            let wasm_on_error = return_abi.wasm_invalid_arg_early_return_statement();
            let FfiParams {
                conversions: native_conversions,
                call_args: native_call_args,
                ..
            } = transform_params(
                fn_inputs,
                &return_lowering,
                &callback_registry,
                &native_on_error,
            );
            let FfiParams {
                conversions: wasm_conversions,
                call_args: wasm_call_args,
                ..
            } = transform_params(
                fn_inputs,
                &return_lowering,
                &callback_registry,
                &wasm_on_error,
            );

            let wasm_body = quote! {
                #(#wasm_conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#wasm_call_args),*);
                match #result_ident {
                    Some(#option_value_ident) => #some_expression,
                    None => f64::NAN,
                }
            };

            let native_body = quote! {
                #(#native_conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#native_call_args),*);
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            };

            return build_f64_wasm_return_exports(
                callable,
                &export_ident,
                &ffi_params,
                wasm_body,
                native_body,
            )
            .into();
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let native_on_error = return_abi.native_invalid_arg_early_return_statement();
            let wasm_on_error = return_abi.wasm_invalid_arg_early_return_statement();
            let FfiParams {
                conversions: native_conversions,
                call_args: native_call_args,
                ..
            } = transform_params(
                fn_inputs,
                &return_lowering,
                &callback_registry,
                &native_on_error,
            );
            let FfiParams {
                conversions: wasm_conversions,
                call_args: wasm_call_args,
                ..
            } = transform_params(
                fn_inputs,
                &return_lowering,
                &callback_registry,
                &wasm_on_error,
            );

            let native_body = quote! {
                #(#native_conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#native_call_args),*);
                <_ as ::boltffi::__private::VecTransport>::pack_vec(#result_ident)
            };

            let wasm_body = quote! {
                #(#wasm_conversions)*
                let #result_ident: #inner_ty = #fn_name(#(#wasm_call_args),*);
                let __buf = ::boltffi::__private::FfiBuf::from_vec(#result_ident);
                ::boltffi::__private::write_return_slot(__buf.as_ptr() as u32, __buf.len() as u32, __buf.cap() as u32, __buf.align() as u32);
                core::mem::forget(__buf);
            };

            return build_void_wasm_return_exports(
                callable,
                &export_ident,
                &ffi_params,
                wasm_body,
                native_body,
            )
            .into();
        }

        let encode_body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #fn_name(#(#call_args),*) },
            &conversions,
            &custom_types,
        );

        build_encoded_return_exports(
            callable,
            &export_ident,
            &ffi_params,
            &return_abi,
            encode_body,
        )
        .into()
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = quote! {
            #(#conversions)*
            ::boltffi::__private::Passable::pack(#fn_name(#(#call_args),*))
        };

        let return_type = quote! { <#rust_type as ::boltffi::__private::Passable>::Out };

        if has_params {
            quote! {
                #input

                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#ffi_params),*
                ) -> #return_type {
                    #body
                }
            }
        } else {
            quote! {
            #input

            #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> #return_type {
                #body
                }
            }
        }
    } else if return_abi.is_object_handle() {
        let object_handle = return_abi
            .object_handle()
            .expect("object handle return must carry handle metadata");
        let pointer_type = object_handle.pointer_type();
        let pointer_expression =
            object_handle.raw_pointer_expression(quote! { #fn_name(#(#call_args),*) });
        let body = quote! {
            #(#conversions)*
            #pointer_expression
        };

        if has_params {
            quote! {
                #input

                #[allow(clippy::not_unsafe_ptr_arg_deref)]
                #[unsafe(no_mangle)]
                #fn_vis unsafe extern "C" fn #export_ident(
                    #(#ffi_params),*
                ) -> #pointer_type {
                    #body
                }
            }
        } else {
            quote! {
            #input

            #[allow(clippy::not_unsafe_ptr_arg_deref)]
            #[unsafe(no_mangle)]
            #fn_vis extern "C" fn #export_ident() -> #pointer_type {
                #body
            }
            }
        }
    } else {
        unreachable!(
            "unsupported function export return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    }
}

pub fn ffi_export_impl(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as ItemFn);
    TokenStream::from(ffi_export_item_impl(input))
}

fn generate_async_export(
    callable: FunctionCallable<'_>,
    class_types: &crate::index::class_types::ClassTypeRegistry,
    custom_types: &custom_types::CustomTypeRegistry,
    callback_registry: &CallbackTraitRegistry,
) -> TokenStream {
    let input = callable.item();
    let fn_name = &input.sig.ident;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;
    let fn_vis = &input.vis;

    let base_name = format!("{}_{}", naming::ffi_prefix(), fn_name);
    let export_names = AsyncExportNames::new(&base_name, fn_name.span());
    let data_types = match data_types::registry_for_current_crate() {
        Ok(registry) => registry,
        Err(error) => return error.to_compile_error().into(),
    };
    let return_lowering = ReturnLoweringContext::new(custom_types, &data_types, class_types);
    let return_abi = return_lowering.lower_output(fn_output);

    let on_wire_record_error = return_abi.async_invalid_arg_early_return_statement();
    let params = match transform_params_async(
        fn_inputs,
        &return_lowering,
        callback_registry,
        &on_wire_record_error,
    ) {
        Ok(params) => params,
        Err(error) => return error.to_compile_error().into(),
    };

    let ffi_return_type = return_abi.async_ffi_return_type();
    let rust_return_type = return_abi.async_rust_return_type();
    let complete_conversion = return_abi.async_complete_conversion(&return_lowering);
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

    let entry_body = quote! {
        #(#pre_spawn)*
        #(let _ = &#move_vars;)*
        ::boltffi::__private::rustfuture::rust_future_new(async move {
            #future_body
        })
    };
    let entry_fn = ExternExport::async_entry(fn_vis, export_names.entry(), ffi_params, entry_body)
        .render(ExportCondition::Always);

    let wasm_complete =
        AsyncWasmCompleteExport::from_resolved_return(&return_abi, &rust_return_type);
    let runtime_exports = AsyncRuntimeExports {
        visibility: fn_vis,
        names: &export_names,
        rust_return_type: quote! { #rust_return_type },
        ffi_return_type: quote! { #ffi_return_type },
        complete_conversion,
        default_value,
    }
    .render(wasm_complete);

    let expanded = quote! {
        #input

        #entry_fn

        #runtime_exports
    };

    TokenStream::from(expanded)
}
