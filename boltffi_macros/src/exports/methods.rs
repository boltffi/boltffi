use super::common::{impl_type_name, is_factory_constructor, is_result_of_self_type_path};

use boltffi_ffi_rules::callable::{CallableForm, ExecutionKind};
use boltffi_ffi_rules::naming;
use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{FnArg, ReturnType, Type};

use crate::exports::async_export::{
    AsyncExportNames, AsyncRuntimeExports, AsyncWasmCompleteExport,
};
use crate::exports::callable::MethodCallable;
use crate::exports::callback_return::resolve_sync_callback_return;
use crate::exports::extern_export::{
    DirectBufferCarrier, DualPlatformExternExport, ExportBody, ExportCondition, ExportSafety,
    ExternExport, ReceiverParameter,
};
use crate::index::CrateIndex;
use crate::index::callback_traits::CallbackTraitRegistry;
use crate::lowering::params::{FfiParams, transform_method_params, transform_method_params_async};
use crate::lowering::returns::lower::{encoded_return_body, encoded_return_buffer_expression};
use crate::lowering::returns::model::{
    ResolvedReturn, ReturnInvocationContext, ReturnLoweringContext, ReturnPlatform,
    WasmOptionScalarEncoding,
};
use boltffi_ffi_rules::transport::EncodedReturnStrategy;

struct ClassExportConfig {
    single_threaded: bool,
}

struct ExportableMethod<'a> {
    method: &'a syn::ImplItemFn,
}

struct InstanceMethodExport<'a> {
    visibility: &'a syn::Visibility,
    export_name: &'a syn::Ident,
    type_name: &'a syn::Ident,
    ffi_params: &'a [proc_macro2::TokenStream],
}

struct StaticMethodExport<'a> {
    visibility: &'a syn::Visibility,
    export_name: &'a syn::Ident,
    ffi_params: &'a [proc_macro2::TokenStream],
}

struct StaticCallbackReturnPlan<'a> {
    safety: ExportSafety,
    wasm_params: &'a [proc_macro2::TokenStream],
    native_params: &'a [proc_macro2::TokenStream],
    wasm_return_type: proc_macro2::TokenStream,
    native_return_type: proc_macro2::TokenStream,
    wasm_body: proc_macro2::TokenStream,
    native_body: proc_macro2::TokenStream,
}

impl ClassExportConfig {
    fn from_attr(attr: &TokenStream) -> Self {
        use syn::parse::Parser;
        let parser = syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated;
        let single_threaded = parser
            .parse(attr.clone())
            .map(|args| {
                args.iter()
                    .any(|ident| ident == "single_threaded" || ident == "thread_unsafe")
            })
            .unwrap_or(false);
        Self { single_threaded }
    }

    fn validate(&self, item_impl: &syn::ItemImpl) -> syn::Result<()> {
        if self.single_threaded || !Self::has_mut_self_methods(item_impl) {
            return Ok(());
        }

        Err(syn::Error::new_spanned(
            item_impl,
            "BoltFFI: `&mut self` methods are not thread-safe in FFI contexts\n\n\
             Two threads calling `&mut self` on the same instance = undefined behavior.\n\n\
             Options:\n\
             1. Use `&self` with interior mutability (Mutex, RwLock, atomics) [recommended]\n\
             2. Add #[export(single_threaded)] ONLY if you enforce thread safety in the target \
                language and want to avoid synchronization overhead you don't need",
        ))
    }

    fn thread_safety_assertion(&self, type_name: &syn::Ident) -> proc_macro2::TokenStream {
        if self.single_threaded {
            return quote! {};
        }

        let span = type_name.span();
        quote_spanned! {span=>
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
                fn _check() { _assert::<#type_name>(); }
            };
        }
    }

    fn has_mut_self_methods(item_impl: &syn::ItemImpl) -> bool {
        item_impl
            .items
            .iter()
            .filter_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(ExportableMethod { method }),
                _ => None,
            })
            .any(|method| method.is_public_mut_self())
    }
}

#[cfg(test)]
fn has_mut_self_methods(input: &syn::ItemImpl) -> bool {
    ClassExportConfig::has_mut_self_methods(input)
}

impl<'a> ExportableMethod<'a> {
    fn from_item(item: &'a syn::ImplItem) -> Option<Self> {
        match item {
            syn::ImplItem::Fn(method) => Some(Self { method }),
            _ => None,
        }
    }

    fn is_public(&self) -> bool {
        matches!(self.method.vis, syn::Visibility::Public(_))
    }

    fn is_skipped(&self) -> bool {
        self.method
            .attrs
            .iter()
            .any(|attribute| attribute.path().is_ident("skip"))
    }

    fn is_exported(&self) -> bool {
        self.is_public() && !self.is_skipped()
    }

    fn is_public_mut_self(&self) -> bool {
        self.is_exported()
            && self.method.sig.inputs.first().is_some_and(
                |arg| matches!(arg, FnArg::Receiver(receiver) if receiver.mutability.is_some()),
            )
    }

    fn stream_item_type(&self) -> Option<syn::Type> {
        extract_ffi_stream_item(&self.method.attrs)
    }

    fn callable(&self) -> MethodCallable<'a> {
        MethodCallable::new(self.method)
    }
}

impl<'a> InstanceMethodExport<'a> {
    fn new(
        visibility: &'a syn::Visibility,
        export_name: &'a syn::Ident,
        type_name: &'a syn::Ident,
        ffi_params: &'a [proc_macro2::TokenStream],
    ) -> Self {
        Self {
            visibility,
            export_name,
            type_name,
            ffi_params,
        }
    }

    fn render_dual_platform(
        self,
        wasm: ExportBody,
        native: ExportBody,
    ) -> proc_macro2::TokenStream {
        DualPlatformExternExport {
            wasm: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety: ExportSafety::Unsafe,
                receiver: ReceiverParameter::Handle(self.type_name),
                params: self.ffi_params,
                allow_ptr_deref: false,
                body: wasm,
            },
            native: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety: ExportSafety::Unsafe,
                receiver: ReceiverParameter::Handle(self.type_name),
                params: self.ffi_params,
                allow_ptr_deref: false,
                body: native,
            },
        }
        .render()
    }

    fn render_encoded_return(
        self,
        resolved_return: &ResolvedReturn,
        encode_body: proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let wasm_return_carrier = DirectBufferCarrier::new(
            resolved_return
                .direct_buffer_return_method(
                    ReturnInvocationContext::SyncExport,
                    ReturnPlatform::Wasm,
                )
                .unwrap_or_else(|| {
                    panic!(
                        "encoded instance sync export must use a direct wasm buffer return carrier: {:?}",
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
                        "encoded instance sync export must use a direct native buffer return carrier: {:?}",
                        resolved_return.value_return_strategy()
                    )
                }),
        );
        let wasm_return_type = wasm_return_carrier.return_type();
        let native_return_type = native_return_carrier.return_type();

        self.render_dual_platform(
            ExportBody {
                return_type: quote! { -> #wasm_return_type },
                body: wasm_return_carrier.lower_body(encode_body.clone()),
            },
            ExportBody {
                return_type: quote! { -> #native_return_type },
                body: native_return_carrier.lower_body(encode_body),
            },
        )
    }

    fn render_callback_return(
        self,
        wasm_params: &'a [proc_macro2::TokenStream],
        native_params: &'a [proc_macro2::TokenStream],
        wasm_return_type: proc_macro2::TokenStream,
        native_return_type: proc_macro2::TokenStream,
        wasm_body: proc_macro2::TokenStream,
        native_body: proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        DualPlatformExternExport {
            wasm: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety: ExportSafety::Unsafe,
                receiver: ReceiverParameter::Handle(self.type_name),
                params: wasm_params,
                allow_ptr_deref: false,
                body: ExportBody {
                    return_type: quote! { -> #wasm_return_type },
                    body: wasm_body,
                },
            },
            native: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety: ExportSafety::Unsafe,
                receiver: ReceiverParameter::Handle(self.type_name),
                params: native_params,
                allow_ptr_deref: false,
                body: ExportBody {
                    return_type: quote! { -> #native_return_type },
                    body: native_body,
                },
            },
        }
        .render()
    }

    fn render_async_entry(self, body: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        ExternExport {
            visibility: self.visibility,
            export_name: self.export_name,
            safety: ExportSafety::Unsafe,
            receiver: ReceiverParameter::Handle(self.type_name),
            params: self.ffi_params,
            allow_ptr_deref: false,
            body: ExportBody {
                return_type: quote! { -> ::boltffi::__private::RustFutureHandle },
                body,
            },
        }
        .render(ExportCondition::Always)
    }
}

impl<'a> StaticMethodExport<'a> {
    fn new(
        visibility: &'a syn::Visibility,
        export_name: &'a syn::Ident,
        ffi_params: &'a [proc_macro2::TokenStream],
    ) -> Self {
        Self {
            visibility,
            export_name,
            ffi_params,
        }
    }

    fn render_dual_platform(
        self,
        safety: ExportSafety,
        wasm: ExportBody,
        native: ExportBody,
    ) -> proc_macro2::TokenStream {
        DualPlatformExternExport {
            wasm: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety,
                receiver: ReceiverParameter::None,
                params: self.ffi_params,
                allow_ptr_deref: false,
                body: wasm,
            },
            native: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety,
                receiver: ReceiverParameter::None,
                params: self.ffi_params,
                allow_ptr_deref: false,
                body: native,
            },
        }
        .render()
    }

    fn render_encoded_return(
        self,
        resolved_return: &ResolvedReturn,
        encode_body: proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        let wasm_return_carrier = DirectBufferCarrier::new(
            resolved_return
                .direct_buffer_return_method(
                    ReturnInvocationContext::SyncExport,
                    ReturnPlatform::Wasm,
                )
                .unwrap_or_else(|| {
                    panic!(
                        "encoded static sync export must use a direct wasm buffer return carrier: {:?}",
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
                        "encoded static sync export must use a direct native buffer return carrier: {:?}",
                        resolved_return.value_return_strategy()
                    )
                }),
        );
        let wasm_return_type = wasm_return_carrier.return_type();
        let native_return_type = native_return_carrier.return_type();
        let safety = if self.ffi_params.is_empty() {
            ExportSafety::Safe
        } else {
            ExportSafety::Unsafe
        };

        self.render_dual_platform(
            safety,
            ExportBody {
                return_type: quote! { -> #wasm_return_type },
                body: wasm_return_carrier.lower_body(encode_body.clone()),
            },
            ExportBody {
                return_type: quote! { -> #native_return_type },
                body: native_return_carrier.lower_body(encode_body),
            },
        )
    }

    fn render_callback_return(
        self,
        callback_return_plan: StaticCallbackReturnPlan<'a>,
    ) -> proc_macro2::TokenStream {
        let StaticCallbackReturnPlan {
            safety,
            wasm_params,
            native_params,
            wasm_return_type,
            native_return_type,
            wasm_body,
            native_body,
        } = callback_return_plan;

        DualPlatformExternExport {
            wasm: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety,
                receiver: ReceiverParameter::None,
                params: wasm_params,
                allow_ptr_deref: false,
                body: ExportBody {
                    return_type: quote! { -> #wasm_return_type },
                    body: wasm_body,
                },
            },
            native: ExternExport {
                visibility: self.visibility,
                export_name: self.export_name,
                safety,
                receiver: ReceiverParameter::None,
                params: native_params,
                allow_ptr_deref: false,
                body: ExportBody {
                    return_type: quote! { -> #native_return_type },
                    body: native_body,
                },
            },
        }
        .render()
    }
}

pub fn export_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let export_config = ClassExportConfig::from_attr(&attr);
    let input = syn::parse_macro_input!(item as syn::ItemImpl);

    if let Err(error) = export_config.validate(&input) {
        return error.to_compile_error().into();
    }

    let crate_index = match CrateIndex::for_current_crate() {
        Ok(crate_index) => crate_index,
        Err(error) => return error.to_compile_error().into(),
    };
    let class_types = crate_index.class_types().clone();
    let custom_types = crate_index.custom_types().clone();
    let callback_registry = crate_index.callback_traits().clone();
    let data_types = crate_index.data_types().clone();

    let type_name = match impl_type_name(&input) {
        Some(name) => name,
        None => {
            return syn::Error::new_spanned(&input, "export requires a named type")
                .to_compile_error()
                .into();
        }
    };

    let self_type: Type = syn::parse_quote!(#type_name);
    let base_return_lowering = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);
    let return_lowering = base_return_lowering.with_self_type(&self_type);

    let type_name_str = type_name.to_string();
    let free_ident = syn::Ident::new(
        naming::class_ffi_free(&type_name_str).as_str(),
        type_name.span(),
    );

    let method_exports: Vec<_> = input
        .items
        .iter()
        .filter_map(ExportableMethod::from_item)
        .filter(|method| method.is_exported())
        .filter_map(|exportable_method| {
            let callable = exportable_method.callable();
            if let Some(item_type) = exportable_method.stream_item_type() {
                return Some(generate_stream_exports(
                    &type_name,
                    &type_name_str,
                    callable,
                    &return_lowering,
                    &item_type,
                ));
            }
            match (callable.form(), callable.execution_kind()) {
                (CallableForm::InstanceMethod, ExecutionKind::Sync)
                | (CallableForm::StaticMethod, ExecutionKind::Sync) => generate_sync_method_export(
                    callable,
                    &type_name,
                    &type_name_str,
                    &return_lowering,
                    &callback_registry,
                ),
                (CallableForm::InstanceMethod, ExecutionKind::Async) => {
                    generate_async_method_export(
                        callable,
                        &type_name,
                        &type_name_str,
                        &return_lowering,
                        &callback_registry,
                    )
                }
                (CallableForm::StaticMethod, ExecutionKind::Async) => None,
                (CallableForm::Function, _) => None,
            }
        })
        .collect();

    let thread_safety_assertion = export_config.thread_safety_assertion(&type_name);

    let expanded = quote! {
        #input

        #thread_safety_assertion

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(handle: *mut #type_name) {
            if !handle.is_null() {
                drop(Box::from_raw(handle));
            }
        }

        #(#method_exports)*
    };

    TokenStream::from(expanded)
}

fn generate_factory_constructor_export(
    callable: MethodCallable<'_>,
    type_name: &syn::Ident,
    class_name: &str,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method = callable.method();
    let method_name = &method.sig.ident;
    let export_name = if method_name == "new" {
        naming::class_ffi_new(class_name)
    } else {
        naming::method_ffi_name(class_name, &method_name.to_string())
    };
    let export_name = syn::Ident::new(export_name.as_str(), method_name.span());

    let inputs = method.sig.inputs.iter().cloned();
    let on_wire_record_error = quote! { return ::core::ptr::null_mut(); };
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let is_fallible = matches!(
        &method.sig.output,
        ReturnType::Type(_, ty)
            if matches!(ty.as_ref(), Type::Path(type_path) if is_result_of_self_type_path(&type_path.path, type_name))
    );

    let body = if is_fallible {
        let call = quote! { #call_expr };
        if conversions.is_empty() {
            quote! {
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        } else {
            quote! {
                #(#conversions)*
                match #call {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        }
    } else if conversions.is_empty() {
        quote! { Box::into_raw(Box::new(#call_expr)) }
    } else {
        quote! {
            #(#conversions)*
            Box::into_raw(Box::new(#call_expr))
        }
    };

    if ffi_params.is_empty() {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #export_name() -> *mut #type_name {
                #body
            }
        })
    } else {
        Some(quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #export_name(
                #(#ffi_params),*
            ) -> *mut #type_name {
                #body
            }
        })
    }
}

fn generate_sync_method_export(
    callable: MethodCallable<'_>,
    type_name: &syn::Ident,
    class_name: &str,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let custom_types = return_lowering.custom_types();
    let method = callable.method();
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );
    let visibility: syn::Visibility = syn::parse_quote! { pub };

    if callable.form() == CallableForm::StaticMethod {
        if is_factory_constructor(method, type_name) {
            return generate_factory_constructor_export(
                callable,
                type_name,
                class_name,
                return_lowering,
                callback_registry,
            );
        }
        return generate_static_method_export(
            callable,
            type_name,
            class_name,
            return_lowering,
            callback_registry,
        );
    }

    let sync_callback_return =
        match resolve_sync_callback_return(&method.sig.output, callback_registry) {
            Ok(resolved_return) => resolved_return,
            Err(error) => return Some(error.to_compile_error()),
        };
    let return_abi = return_lowering.lower_output(&method.sig.output);
    if let Some(callback_return) = sync_callback_return {
        let other_inputs = method.sig.inputs.iter().skip(1).cloned();
        let native_on_wire_record_error =
            callback_return.native_invalid_arg_early_return_statement();
        let wasm_on_wire_record_error = callback_return.wasm_invalid_arg_early_return_statement();
        let FfiParams {
            ffi_params: native_ffi_params,
            conversions: native_conversions,
            call_args: native_call_args,
        } = transform_method_params(
            other_inputs.clone(),
            return_lowering,
            callback_registry,
            &native_on_wire_record_error,
        );
        let FfiParams {
            ffi_params: wasm_ffi_params,
            conversions: wasm_conversions,
            call_args: wasm_call_args,
        } = transform_method_params(
            other_inputs,
            return_lowering,
            callback_registry,
            &wasm_on_wire_record_error,
        );
        let native_body = callback_return.lower_native_result_expression(quote! {
            {
                #(#native_conversions)*
                (*handle).#method_name(#(#native_call_args),*)
            }
        });
        let wasm_body = callback_return.lower_wasm_result_expression(quote! {
            {
                #(#wasm_conversions)*
                (*handle).#method_name(#(#wasm_call_args),*)
            }
        });
        let native_return_type = callback_return.native_ffi_return_type();
        let wasm_return_type = callback_return.wasm_ffi_return_type();

        return Some(
            InstanceMethodExport::new(&visibility, &export_name, type_name, &[])
                .render_callback_return(
                    &wasm_ffi_params,
                    &native_ffi_params,
                    quote! { #wasm_return_type },
                    quote! { #native_return_type },
                    wasm_body,
                    native_body,
                ),
        );
    }

    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        other_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let has_conversions = !conversions.is_empty();
    let call_expr = quote! { (*handle).#method_name(#(#call_args),*) };

    let (body, return_type, is_wire_encoded) = if return_abi.is_unit() {
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        };
        (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &method.sig.output;
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (body, quote! { #fn_output }, false)
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", method_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", method_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
            };

            let wasm_body = quote! {
                #call_and_bind
                match #result_ident {
                    Some(#option_value_ident) => #some_expression,
                    None => f64::NAN,
                }
            };

            let native_body = quote! {
                #call_and_bind
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            };

            return Some(
                InstanceMethodExport::new(&visibility, &export_name, type_name, &ffi_params)
                    .render_dual_platform(
                        ExportBody {
                            return_type: quote! { -> f64 },
                            body: wasm_body,
                        },
                        ExportBody {
                            return_type: quote! { -> ::boltffi::__private::FfiBuf },
                            body: native_body,
                        },
                    ),
            );
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let native_on_error = return_abi.native_invalid_arg_early_return_statement();
            let wasm_on_error = return_abi.wasm_invalid_arg_early_return_statement();
            let other_inputs_native = method.sig.inputs.iter().skip(1).cloned();
            let FfiParams {
                conversions: native_conversions,
                call_args: native_call_args,
                ..
            } = transform_method_params(
                other_inputs_native,
                return_lowering,
                callback_registry,
                &native_on_error,
            );
            let other_inputs_wasm = method.sig.inputs.iter().skip(1).cloned();
            let FfiParams {
                conversions: wasm_conversions,
                call_args: wasm_call_args,
                ..
            } = transform_method_params(
                other_inputs_wasm,
                return_lowering,
                callback_registry,
                &wasm_on_error,
            );

            let native_call = quote! { (*handle).#method_name(#(#native_call_args),*) };
            let wasm_call = quote! { (*handle).#method_name(#(#wasm_call_args),*) };

            let native_body = quote! {
                #(#native_conversions)*
                let #result_ident: #inner_ty = #native_call;
                <_ as ::boltffi::__private::VecTransport>::pack_vec(#result_ident)
            };

            let wasm_body = quote! {
                #(#wasm_conversions)*
                let #result_ident: #inner_ty = #wasm_call;
                let __buf = ::boltffi::__private::FfiBuf::from_vec(#result_ident);
                ::boltffi::__private::write_return_slot(__buf.as_ptr() as u32, __buf.len() as u32, __buf.cap() as u32, __buf.align() as u32);
                core::mem::forget(__buf);
            };

            return Some(
                InstanceMethodExport::new(&visibility, &export_name, type_name, &ffi_params)
                    .render_dual_platform(
                        ExportBody {
                            return_type: quote! {},
                            body: wasm_body,
                        },
                        ExportBody {
                            return_type: quote! { -> ::boltffi::__private::FfiBuf },
                            body: native_body,
                        },
                    ),
            );
        }

        let body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #call_expr },
            &conversions,
            custom_types,
        );
        (body, quote! { -> ::boltffi::__private::FfiBuf }, true)
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        } else {
            quote! {
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        };
        let return_type = quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out };
        (body, return_type, false)
    } else if return_abi.is_object_handle() {
        let object_handle = return_abi
            .object_handle()
            .expect("object handle return must carry handle metadata");
        let pointer_expression = object_handle.raw_pointer_expression(quote! { #call_expr });
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #pointer_expression
            }
        } else {
            pointer_expression
        };
        (body, object_handle.ffi_return_type(), false)
    } else {
        unreachable!(
            "unsupported instance method return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    };

    if is_wire_encoded {
        return Some(
            InstanceMethodExport::new(&visibility, &export_name, type_name, &ffi_params)
                .render_encoded_return(&return_abi, body),
        );
    }

    Some(
        ExternExport {
            visibility: &visibility,
            export_name: &export_name,
            safety: ExportSafety::Unsafe,
            receiver: ReceiverParameter::Handle(type_name),
            params: &ffi_params,
            allow_ptr_deref: false,
            body: ExportBody { return_type, body },
        }
        .render(ExportCondition::Always),
    )
}

fn generate_static_method_export(
    callable: MethodCallable<'_>,
    type_name: &syn::Ident,
    class_name: &str,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let custom_types = return_lowering.custom_types();
    let method = callable.method();
    let method_name = &method.sig.ident;
    let export_name = syn::Ident::new(
        naming::method_ffi_name(class_name, &method_name.to_string()).as_str(),
        method_name.span(),
    );
    let visibility: syn::Visibility = syn::parse_quote! { pub };

    let sync_callback_return =
        match resolve_sync_callback_return(&method.sig.output, callback_registry) {
            Ok(resolved_return) => resolved_return,
            Err(error) => return Some(error.to_compile_error()),
        };
    let return_abi = return_lowering.lower_output(&method.sig.output);
    if let Some(callback_return) = sync_callback_return {
        let all_inputs = method.sig.inputs.iter().cloned();
        let native_on_wire_record_error =
            callback_return.native_invalid_arg_early_return_statement();
        let wasm_on_wire_record_error = callback_return.wasm_invalid_arg_early_return_statement();
        let FfiParams {
            ffi_params: native_ffi_params,
            conversions: native_conversions,
            call_args: native_call_args,
        } = transform_method_params(
            all_inputs.clone(),
            return_lowering,
            callback_registry,
            &native_on_wire_record_error,
        );
        let FfiParams {
            ffi_params: wasm_ffi_params,
            conversions: wasm_conversions,
            call_args: wasm_call_args,
        } = transform_method_params(
            all_inputs,
            return_lowering,
            callback_registry,
            &wasm_on_wire_record_error,
        );
        let native_body = callback_return.lower_native_result_expression(quote! {
            {
                #(#native_conversions)*
                #type_name::#method_name(#(#native_call_args),*)
            }
        });
        let wasm_body = callback_return.lower_wasm_result_expression(quote! {
            {
                #(#wasm_conversions)*
                #type_name::#method_name(#(#wasm_call_args),*)
            }
        });
        let native_return_type = callback_return.native_ffi_return_type();
        let wasm_return_type = callback_return.wasm_ffi_return_type();
        let safety = if native_ffi_params.is_empty() {
            ExportSafety::Safe
        } else {
            ExportSafety::Unsafe
        };

        return Some(
            StaticMethodExport::new(&visibility, &export_name, &[]).render_callback_return(
                StaticCallbackReturnPlan {
                    safety,
                    wasm_params: &wasm_ffi_params,
                    native_params: &native_ffi_params,
                    wasm_return_type: quote! { #wasm_return_type },
                    native_return_type: quote! { #native_return_type },
                    wasm_body,
                    native_body,
                },
            ),
        );
    }

    let on_wire_record_error = return_abi.invalid_arg_early_return_statement();
    let all_inputs = method.sig.inputs.iter().cloned();
    let FfiParams {
        ffi_params,
        conversions,
        call_args,
    } = transform_method_params(
        all_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    );

    let has_conversions = !conversions.is_empty();
    let call_expr = quote! { #type_name::#method_name(#(#call_args),*) };

    let (body, return_type, is_wire_encoded) = if return_abi.is_unit() {
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        } else {
            quote! {
                #call_expr;
                ::boltffi::__private::FfiStatus::OK
            }
        };
        (body, quote! { -> ::boltffi::__private::FfiStatus }, false)
    } else if return_abi.is_primitive_scalar() {
        let fn_output = &method.sig.output;
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #call_expr
            }
        } else {
            call_expr
        };
        (body, quote! { #fn_output }, false)
    } else if let Some(strategy) = return_abi.encoded_return_strategy() {
        let inner_ty = return_abi.rust_type();
        let result_ident = syn::Ident::new("result", method_name.span());

        if matches!(strategy, EncodedReturnStrategy::OptionScalar) {
            let option_value_ident = syn::Ident::new("value", method_name.span());
            let option_scalar_encoding = WasmOptionScalarEncoding::from_option_rust_type(inner_ty)
                .expect("OptionScalar return must have a primitive Option inner type");
            let some_expression = option_scalar_encoding.some_expression(&option_value_ident);
            let call_and_bind = if has_conversions {
                quote! {
                    #(#conversions)*
                    let #result_ident: #inner_ty = #call_expr;
                }
            } else {
                quote! {
                    let #result_ident: #inner_ty = #call_expr;
                }
            };

            let wasm_body = quote! {
                #call_and_bind
                match #result_ident {
                    Some(#option_value_ident) => #some_expression,
                    None => f64::NAN,
                }
            };

            let native_body = quote! {
                #call_and_bind
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            };

            let safety = if ffi_params.is_empty() {
                ExportSafety::Safe
            } else {
                ExportSafety::Unsafe
            };
            return Some(
                StaticMethodExport::new(&visibility, &export_name, &ffi_params)
                    .render_dual_platform(
                        safety,
                        ExportBody {
                            return_type: quote! { -> f64 },
                            body: wasm_body,
                        },
                        ExportBody {
                            return_type: quote! { -> ::boltffi::__private::FfiBuf },
                            body: native_body,
                        },
                    ),
            );
        }

        if matches!(strategy, EncodedReturnStrategy::DirectVec) {
            let native_on_error = return_abi.native_invalid_arg_early_return_statement();
            let wasm_on_error = return_abi.wasm_invalid_arg_early_return_statement();
            let all_inputs_native = method.sig.inputs.iter().cloned();
            let FfiParams {
                conversions: native_conversions,
                call_args: native_call_args,
                ..
            } = transform_method_params(
                all_inputs_native,
                return_lowering,
                callback_registry,
                &native_on_error,
            );
            let all_inputs_wasm = method.sig.inputs.iter().cloned();
            let FfiParams {
                conversions: wasm_conversions,
                call_args: wasm_call_args,
                ..
            } = transform_method_params(
                all_inputs_wasm,
                return_lowering,
                callback_registry,
                &wasm_on_error,
            );

            let native_call = quote! { #type_name::#method_name(#(#native_call_args),*) };
            let wasm_call = quote! { #type_name::#method_name(#(#wasm_call_args),*) };

            let native_body = quote! {
                #(#native_conversions)*
                let #result_ident: #inner_ty = #native_call;
                <_ as ::boltffi::__private::VecTransport>::pack_vec(#result_ident)
            };

            let wasm_body = quote! {
                #(#wasm_conversions)*
                let #result_ident: #inner_ty = #wasm_call;
                let __buf = ::boltffi::__private::FfiBuf::from_vec(#result_ident);
                ::boltffi::__private::write_return_slot(__buf.as_ptr() as u32, __buf.len() as u32, __buf.cap() as u32, __buf.align() as u32);
                core::mem::forget(__buf);
            };

            let safety = if ffi_params.is_empty() {
                ExportSafety::Safe
            } else {
                ExportSafety::Unsafe
            };
            return Some(
                StaticMethodExport::new(&visibility, &export_name, &ffi_params)
                    .render_dual_platform(
                        safety,
                        ExportBody {
                            return_type: quote! {},
                            body: wasm_body,
                        },
                        ExportBody {
                            return_type: quote! { -> ::boltffi::__private::FfiBuf },
                            body: native_body,
                        },
                    ),
            );
        }

        let body = encoded_return_body(
            inner_ty,
            strategy,
            &result_ident,
            quote! { #call_expr },
            &conversions,
            custom_types,
        );
        (body, quote! { -> ::boltffi::__private::FfiBuf }, true)
    } else if return_abi.is_passable_value() {
        let rust_type = return_abi.rust_type();
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        } else {
            quote! {
                ::boltffi::__private::Passable::pack(#call_expr)
            }
        };
        let return_type = quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out };
        (body, return_type, false)
    } else if return_abi.is_object_handle() {
        let object_handle = return_abi
            .object_handle()
            .expect("object handle return must carry handle metadata");
        let pointer_expression = object_handle.raw_pointer_expression(quote! { #call_expr });
        let body = if has_conversions {
            quote! {
                #(#conversions)*
                #pointer_expression
            }
        } else {
            pointer_expression
        };
        (body, object_handle.ffi_return_type(), false)
    } else {
        unreachable!(
            "unsupported static method return strategy: {:?}",
            return_abi.value_return_strategy()
        )
    };

    if is_wire_encoded {
        return Some(
            StaticMethodExport::new(&visibility, &export_name, &ffi_params)
                .render_encoded_return(&return_abi, body),
        );
    }

    Some(
        ExternExport {
            visibility: &visibility,
            export_name: &export_name,
            safety: if ffi_params.is_empty() {
                ExportSafety::Safe
            } else {
                ExportSafety::Unsafe
            },
            receiver: ReceiverParameter::None,
            params: &ffi_params,
            allow_ptr_deref: false,
            body: ExportBody { return_type, body },
        }
        .render(ExportCondition::Always),
    )
}

fn generate_async_method_export(
    callable: MethodCallable<'_>,
    type_name: &syn::Ident,
    class_name: &str,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
) -> Option<proc_macro2::TokenStream> {
    let method = callable.method();
    let method_name = &method.sig.ident;
    let method_name_str = method_name.to_string();
    let receiver = match method.sig.inputs.first() {
        Some(FnArg::Receiver(receiver)) => receiver,
        _ => return None,
    };
    let needs_mut = receiver.mutability.is_some();

    let base_name = naming::method_ffi_name(class_name, &method_name_str);
    let export_names = AsyncExportNames::new(base_name.as_str(), method_name.span());
    let visibility: syn::Visibility = syn::parse_quote! { pub };
    let fn_output = &method.sig.output;
    let return_abi = return_lowering.lower_output(fn_output);

    let other_inputs = method.sig.inputs.iter().skip(1).cloned();
    let on_wire_record_error = return_abi.async_invalid_arg_early_return_statement();
    let params = match transform_method_params_async(
        other_inputs,
        return_lowering,
        callback_registry,
        &on_wire_record_error,
    ) {
        Ok(params) => params,
        Err(error) => return Some(error.to_compile_error()),
    };

    let ffi_return_type = return_abi.async_ffi_return_type();
    let rust_return_type = return_abi.async_rust_return_type();
    let complete_conversion = return_abi.async_complete_conversion(return_lowering);
    let default_value = return_abi.async_default_ffi_value();

    let ffi_params = &params.ffi_params;
    let pre_spawn = &params.pre_spawn;
    let thread_setup = &params.thread_setup;
    let call_args = &params.call_args;
    let move_vars = &params.move_vars;

    let future_body = quote! {
        #(#thread_setup)*
        instance.#method_name(#(#call_args),*).await
    };

    let instance_binding = if needs_mut {
        quote! { let instance = &mut *handle; }
    } else {
        quote! { let instance = &*handle; }
    };

    let entry_body = quote! {
        #instance_binding
        #(#pre_spawn)*
        #(let _ = &#move_vars;)*
        ::boltffi::__private::rustfuture::rust_future_new(async move {
            #future_body
        })
    };
    let entry_fn =
        InstanceMethodExport::new(&visibility, export_names.entry(), type_name, ffi_params)
            .render_async_entry(entry_body);

    let wasm_complete =
        AsyncWasmCompleteExport::from_resolved_return(&return_abi, &rust_return_type);
    let runtime_exports = AsyncRuntimeExports {
        visibility: &visibility,
        names: &export_names,
        rust_return_type: quote! { #rust_return_type },
        ffi_return_type: quote! { #ffi_return_type },
        complete_conversion,
        default_value,
    }
    .render(wasm_complete);

    Some(quote! {
        #entry_fn

        #runtime_exports
    })
}

fn extract_ffi_stream_item(attrs: &[syn::Attribute]) -> Option<syn::Type> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("ffi_stream") {
            return None;
        }

        let mut item_type: Option<syn::Type> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("item") {
                let value: syn::Type = meta.value()?.parse()?;
                item_type = Some(value);
            }
            Ok(())
        });

        item_type
    })
}

fn generate_stream_exports(
    type_name: &syn::Ident,
    class_name: &str,
    callable: MethodCallable<'_>,
    return_lowering: &ReturnLoweringContext<'_>,
    item_type: &syn::Type,
) -> proc_macro2::TokenStream {
    let method = callable.method();
    let method_name = &method.sig.ident;
    let stream_name = method_name.to_string();
    let item_abi = return_lowering.lower_type(item_type);

    let subscribe_ident = syn::Ident::new(
        naming::stream_ffi_subscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let pop_batch_ident = syn::Ident::new(
        naming::stream_ffi_pop_batch(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let wait_ident = syn::Ident::new(
        naming::stream_ffi_wait(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let poll_ident = syn::Ident::new(
        naming::stream_ffi_poll(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let unsubscribe_ident = syn::Ident::new(
        naming::stream_ffi_unsubscribe(class_name, &stream_name).as_str(),
        method_name.span(),
    );
    let free_ident = syn::Ident::new(
        naming::stream_ffi_free(class_name, &stream_name).as_str(),
        method_name.span(),
    );

    let pop_batch_export = if item_abi.is_primitive_scalar() || item_abi.is_passable_value() {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #pop_batch_ident(
                subscription_handle: ::boltffi::__private::SubscriptionHandle,
                output_ptr: *mut <#item_type as ::boltffi::__private::Passable>::Out,
                output_capacity: usize,
            ) -> usize {
                if subscription_handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
                    return 0;
                }
                let subscription = unsafe {
                    &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
                };
                let output_slots = unsafe {
                    ::core::slice::from_raw_parts_mut(
                        output_ptr as *mut ::core::mem::MaybeUninit<<#item_type as ::boltffi::__private::Passable>::Out>,
                        output_capacity,
                    )
                };

                output_slots
                    .iter_mut()
                    .map_while(|slot| {
                        let item = subscription.pop_event()?;
                        slot.write(<#item_type as ::boltffi::__private::Passable>::pack(item));
                        Some(())
                    })
                    .count()
            }
        }
    } else {
        let items_ident = syn::Ident::new("__boltffi_stream_items", method_name.span());
        let batch_type: syn::Type = syn::parse_quote! { Vec<#item_type> };
        let encoded_batch = encoded_return_buffer_expression(
            &batch_type,
            EncodedReturnStrategy::WireEncoded,
            &items_ident,
            Some(return_lowering.custom_types()),
        );

        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #pop_batch_ident(
                subscription_handle: ::boltffi::__private::SubscriptionHandle,
                max_count: usize,
            ) -> ::boltffi::__private::FfiBuf {
                if subscription_handle.is_null() || max_count == 0 {
                    return ::boltffi::__private::FfiBuf::empty();
                }
                let subscription = unsafe {
                    &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
                };
                let #items_ident: Vec<#item_type> = ::core::iter::from_fn(|| subscription.pop_event())
                    .take(max_count)
                    .collect();

                if #items_ident.is_empty() {
                    ::boltffi::__private::FfiBuf::empty()
                } else {
                    #encoded_batch
                }
            }
        }
    };

    quote! {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #subscribe_ident(
            handle: *const #type_name,
        ) -> ::boltffi::__private::SubscriptionHandle {
            if handle.is_null() {
                return std::ptr::null_mut();
            }
            let instance = unsafe { &*handle };
            let subscription = instance.#method_name();
            std::sync::Arc::into_raw(subscription) as ::boltffi::__private::SubscriptionHandle
        }

        #pop_batch_export

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #wait_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            timeout_milliseconds: u32,
        ) -> i32 {
            if subscription_handle.is_null() {
                return ::boltffi::__private::WaitResult::Unsubscribed as i32;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.wait_for_events(timeout_milliseconds) as i32
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #poll_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
            callback_data: u64,
            callback: ::boltffi::__private::StreamContinuationCallback,
        ) {
            if subscription_handle.is_null() {
                callback(callback_data, ::boltffi::__private::StreamPollResult::Closed);
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.poll(callback_data, callback);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #unsubscribe_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            let subscription = unsafe {
                &*(subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>)
            };
            subscription.unsubscribe();
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #free_ident(
            subscription_handle: ::boltffi::__private::SubscriptionHandle,
        ) {
            if subscription_handle.is_null() {
                return;
            }
            drop(unsafe {
                std::sync::Arc::from_raw(
                    subscription_handle as *const ::boltffi::__private::EventSubscription<#item_type>
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::callback_traits::CallbackTraitRegistry;
    use crate::index::class_types::ClassTypeRegistry;
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::{DataTypeCategory, DataTypeRegistry};
    use crate::lowering::returns::model::ReturnLoweringContext;
    use syn::parse_quote;

    fn parse_impl(code: &str) -> syn::ItemImpl {
        syn::parse_str(code).expect("failed to parse impl block")
    }

    fn return_lowering() -> ReturnLoweringContext<'static> {
        let class_types = Box::leak(Box::new(ClassTypeRegistry::default()));
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::with_entries(&[
            ("UserProfile", DataTypeCategory::WireEncoded),
            ("Filter", DataTypeCategory::WireEncoded),
        ])));
        ReturnLoweringContext::new(custom_types, data_types, class_types)
    }

    fn class_return_lowering() -> ReturnLoweringContext<'static> {
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::default()));
        let class_types = Box::leak(Box::new(ClassTypeRegistry::with_entries(&["Marker"])));
        ReturnLoweringContext::new(custom_types, data_types, class_types)
    }

    fn self_return_lowering() -> ReturnLoweringContext<'static> {
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::default()));
        let class_types = Box::leak(Box::new(ClassTypeRegistry::with_entries(&["Map"])));
        let self_type = Box::leak(Box::new(parse_quote!(Map)));
        let base_context = Box::leak(Box::new(ReturnLoweringContext::new(
            custom_types,
            data_types,
            class_types,
        )));
        base_context.with_self_type(self_type)
    }

    fn callback_registry() -> &'static CallbackTraitRegistry {
        Box::leak(Box::new(CallbackTraitRegistry::default()))
    }

    fn extract_receiver_mutability(method: &syn::ImplItemFn) -> Option<bool> {
        match method.sig.inputs.first() {
            Some(FnArg::Receiver(r)) => Some(r.mutability.is_some()),
            _ => None,
        }
    }

    #[test]
    fn receiver_detection_ref_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn get(&self) -> i32 { self.value }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), Some(false));
    }

    #[test]
    fn receiver_detection_ref_mut_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), Some(true));
    }

    #[test]
    fn receiver_detection_static_method() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new(initial: i32) -> Self { Self { value: initial } }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert_eq!(extract_receiver_mutability(method), None);
    }

    #[test]
    fn async_method_ref_self_detected() {
        let impl_block = parse_impl(
            r#"
            impl Database {
                pub async fn query(&self, sql: String) -> String { sql }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert!(method.sig.asyncness.is_some());
        assert_eq!(extract_receiver_mutability(method), Some(false));
    }

    #[test]
    fn async_method_ref_mut_self_detected() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub async fn async_increment(&mut self) -> i32 { self.value += 1; self.value }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(m) => Some(m),
                _ => None,
            })
            .unwrap();

        assert!(method.sig.asyncness.is_some());
        assert_eq!(extract_receiver_mutability(method), Some(true));
    }

    #[test]
    fn instance_binding_generation_immutable() {
        let needs_mut = false;
        let instance_binding = if needs_mut {
            quote! { let instance = &mut *handle; }
        } else {
            quote! { let instance = &*handle; }
        };

        let output = instance_binding.to_string();
        assert!(output.contains("& * handle"));
        assert!(!output.contains("& mut"));
    }

    #[test]
    fn instance_binding_generation_mutable() {
        let needs_mut = true;
        let instance_binding = if needs_mut {
            quote! { let instance = &mut *handle; }
        } else {
            quote! { let instance = &*handle; }
        };

        let output = instance_binding.to_string();
        assert!(output.contains("& mut * handle"));
    }

    #[test]
    fn has_mut_self_methods_detects_mut_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_ref_self() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn get(&self) -> i32 { self.value }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_static() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new() -> Self { Counter { value: 0 } }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_private() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                fn private_mut(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_mixed_methods() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                pub fn new() -> Self { Counter { value: 0 } }
                pub fn get(&self) -> i32 { self.value }
                pub fn increment(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(has_mut_self_methods(&impl_block));
    }

    #[test]
    fn has_mut_self_methods_ignores_skipped() {
        let impl_block = parse_impl(
            r#"
            impl Counter {
                #[skip]
                pub fn internal_mut(&mut self) { self.value += 1; }
            }
            "#,
        );
        assert!(!has_mut_self_methods(&impl_block));
    }

    #[test]
    fn factory_constructor_with_wire_encoded_param_returns_early_on_decode_error() {
        let impl_block = parse_impl(
            r#"
            impl Inventory {
                pub fn from_person(person: Person) -> Self {
                    let _ = person;
                    Self
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("constructor method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_factory_constructor_export(
            MethodCallable::new(method),
            &type_name,
            "Inventory",
            &return_lowering(),
            callback_registry(),
        )
        .expect("constructor export should be generated")
        .to_string();

        assert!(generated.contains("let person : Person"));
        assert!(generated.contains("return :: core :: ptr :: null_mut ()"));
    }

    #[test]
    fn instance_method_with_borrowed_wire_params_lowers_to_buffers_and_storage() {
        let impl_block = parse_impl(
            r#"
            impl Inventory {
                pub fn summarize(&self, profile: &UserProfile, filter: &Filter) -> String {
                    let _ = (profile, filter);
                    String::new()
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Inventory",
            &return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(generated.contains("profile_storage_ptr : * const u8"));
        assert!(generated.contains("profile_storage_len : usize"));
        assert!(generated.contains("filter_storage_ptr : * const u8"));
        assert!(generated.contains("filter_storage_len : usize"));
        assert!(generated.contains("let profile_storage : UserProfile"));
        assert!(generated.contains("let filter_storage : Filter"));
        assert!(generated.contains("let profile = & profile_storage"));
        assert!(generated.contains("let filter = & filter_storage"));
        assert!(!generated.contains("profile : & UserProfile"));
        assert!(!generated.contains("filter : & Filter"));
    }

    #[test]
    fn instance_method_returning_exported_class_lowers_to_object_handle() {
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn add_marker(&self) -> Marker {
                    Marker
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &class_return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(generated.contains("-> * mut Marker"));
        assert!(generated.contains("Box :: into_raw (Box :: new ((* handle) . add_marker ()))"));
        assert!(!generated.contains("FfiBuf :: wire_encode"));
    }

    #[test]
    fn instance_method_returning_result_of_exported_class_does_not_wire_encode() {
        // Regression: `Result<Class, E>` returns must not lower to a
        // whole-Result `wire_encode`, which would require the class type
        // to implement `WireEncode`/`WireDecode`. The class is a handle,
        // not a wire value, so the success payload must cross as an
        // object handle and the error must cross via the last-error
        // channel — mirroring the factory-constructor fallible path.
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn try_make_marker(&self, value: i32) -> Result<Marker, String> {
                    if value < 0 {
                        Err("negative".to_string())
                    } else {
                        Ok(Marker)
                    }
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &class_return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        // The bug: the whole Result was wire-encoded, emitting
        // `FfiBuf :: wire_encode (& result)` which fails to compile
        // because `Marker` (a class) does not implement WireEncode /
        // WireDecode.
        assert!(
            !generated.contains("FfiBuf :: wire_encode"),
            "Result<Class, E> must not wire-encode the whole Result; got:\n{generated}"
        );
        // The success payload must cross as an object handle.
        assert!(
            generated.contains("Box :: into_raw (Box :: new (value))"),
            "Result<Class, E> Ok payload must become an object handle; got:\n{generated}"
        );
        // The error must cross via the last-error channel.
        assert!(
            generated.contains("set_last_error"),
            "Result<Class, E> Err payload must use set_last_error; got:\n{generated}"
        );
    }

    #[test]
    fn instance_method_returning_result_of_self_does_not_wire_encode() {
        // `Result<Self, E>` on an instance method (not a factory
        // constructor) must lower the same way as `Result<Class, E>`:
        // handle out + last-error, never whole-Result wire_encode.
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn try_clone(&self) -> Result<Self, String> {
                    Ok(Map)
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &self_return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(
            !generated.contains("FfiBuf :: wire_encode"),
            "Result<Self, E> must not wire-encode the whole Result; got:\n{generated}"
        );
        assert!(
            generated.contains("Box :: into_raw (Box :: new (value))"),
            "Result<Self, E> Ok payload must become an object handle; got:\n{generated}"
        );
        assert!(
            generated.contains("set_last_error"),
            "Result<Self, E> Err payload must use set_last_error; got:\n{generated}"
        );
    }

    #[test]
    fn static_method_returning_result_of_exported_class_does_not_wire_encode() {
        // A static (non-constructor) method returning
        // `Result<OtherClass, E>` must also avoid whole-Result
        // wire_encode and route through handle + last-error.
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn borrow_marker(id: i32) -> Result<Marker, String> {
                    if id < 0 {
                        Err("bad id".to_string())
                    } else {
                        Ok(Marker)
                    }
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("static method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &class_return_lowering(),
            callback_registry(),
        )
        .expect("static export should be generated")
        .to_string();

        assert!(
            !generated.contains("FfiBuf :: wire_encode"),
            "static Result<Class, E> must not wire-encode the whole Result; got:\n{generated}"
        );
        assert!(
            generated.contains("Box :: into_raw (Box :: new (value))"),
            "static Result<Class, E> Ok payload must become an object handle; got:\n{generated}"
        );
        assert!(
            generated.contains("set_last_error"),
            "static Result<Class, E> Err payload must use set_last_error; got:\n{generated}"
        );
    }

    #[test]
    fn instance_method_returning_result_of_record_still_wire_encodes() {
        // Regression guard: `Result<Record, E>` must keep wire-encoding
        // the whole Result, because records implement WireEncode /
        // WireDecode. Only class/Self payloads are object handles and
        // must bypass the wire path. `UserProfile` is a #[data] record
        // (WireEncoded), not a class, so this must NOT lower to an
        // object handle.
        let impl_block = parse_impl(
            r#"
            impl Service {
                pub fn fetch_profile(&self, id: i32) -> Result<UserProfile, String> {
                    if id < 0 {
                        Err("bad id".to_string())
                    } else {
                        Ok(UserProfile)
                    }
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Service",
            &return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        // Records cross as wire-encoded values, so the whole Result
        // must be wire-encoded — the opposite of the class case.
        assert!(
            generated.contains("FfiBuf :: wire_encode"),
            "Result<Record, E> must wire-encode the whole Result; got:\n{generated}"
        );
        assert!(
            !generated.contains("Box :: into_raw"),
            "Result<Record, E> must not lower to an object handle; got:\n{generated}"
        );
        assert!(
            !generated.contains("set_last_error"),
            "Result<Record, E> must not use the last-error handle path; got:\n{generated}"
        );
    }

    #[test]
    fn instance_method_returning_self_lowers_to_object_handle() {
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn clone_handle(&self) -> Self {
                    Map
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &self_return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(generated.contains("-> * mut Map"));
        assert!(generated.contains("Box :: into_raw (Box :: new ((* handle) . clone_handle ()))"));
        assert!(!generated.contains("* mut Self"));
        assert!(!generated.contains("FfiBuf :: wire_encode"));
    }

    #[test]
    fn instance_method_returning_optional_exported_class_lowers_to_nullable_object_handle() {
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn find_marker(&self) -> Option<Marker> {
                    Some(Marker)
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &class_return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(generated.contains("-> * mut Marker"));
        assert!(generated.contains("Some (value) => Box :: into_raw (Box :: new (value))"));
        assert!(generated.contains("None => :: core :: ptr :: null_mut ()"));
        assert!(!generated.contains("* mut Option < Marker >"));
        assert!(!generated.contains("FfiBuf :: wire_encode"));
    }

    #[test]
    fn static_method_returning_exported_class_lowers_to_object_handle() {
        let impl_block = parse_impl(
            r#"
            impl Map {
                pub fn default_marker() -> Marker {
                    Marker
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("static method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Map",
            &class_return_lowering(),
            callback_registry(),
        )
        .expect("static export should be generated")
        .to_string();

        assert!(generated.contains("-> * mut Marker"));
        assert!(generated.contains("Box :: into_raw (Box :: new (Map :: default_marker ()))"));
        assert!(!generated.contains("FfiBuf :: wire_encode"));
    }

    #[test]
    fn async_method_with_borrowed_wire_params_lowers_to_buffers_and_storage() {
        let impl_block = parse_impl(
            r#"
            impl Inventory {
                pub async fn summarize_async(&self, profile: &UserProfile, filter: &Filter) -> String {
                    let _ = (profile, filter);
                    String::new()
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("async instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_async_method_export(
            MethodCallable::new(method),
            &type_name,
            "Inventory",
            &return_lowering(),
            callback_registry(),
        )
        .expect("async instance export should be generated")
        .to_string();

        assert!(generated.contains("profile_storage_ptr : * const u8"));
        assert!(generated.contains("profile_storage_len : usize"));
        assert!(generated.contains("filter_storage_ptr : * const u8"));
        assert!(generated.contains("filter_storage_len : usize"));
        assert!(generated.contains("let profile_storage : UserProfile"));
        assert!(generated.contains("let filter_storage : Filter"));
        assert!(generated.contains("let profile = & profile_storage"));
        assert!(generated.contains("let filter = & filter_storage"));
        assert!(!generated.contains("profile : & UserProfile"));
        assert!(!generated.contains("filter : & Filter"));
    }

    #[test]
    fn instance_method_with_borrowed_wire_params_and_encoded_return_keeps_storage_binding() {
        let impl_block = parse_impl(
            r#"
            impl Inventory {
                pub fn summarize(&self, profile: &UserProfile) -> Vec<Summary> {
                    let _ = profile;
                    Vec::new()
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("instance method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");

        let generated = generate_sync_method_export(
            MethodCallable::new(method),
            &type_name,
            "Inventory",
            &return_lowering(),
            callback_registry(),
        )
        .expect("instance export should be generated")
        .to_string();

        assert!(generated.contains("profile_storage_ptr : * const u8"));
        assert!(generated.contains("profile_storage_len : usize"));
        assert!(generated.contains("let profile_storage : UserProfile"));
        assert!(generated.contains("let profile = & profile_storage"));
        assert!(
            generated.contains("let result : Vec < Summary > = (* handle) . summarize (profile) ;")
        );
    }

    #[test]
    fn direct_stream_pop_batch_uses_passable_output_slots() {
        let impl_block = parse_impl(
            r#"
            impl EventBus {
                #[ffi_stream(item = i32)]
                pub fn subscribe_values(&self) -> std::sync::Arc<EventSubscription<i32>> {
                    todo!()
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("stream method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");
        let item_type: syn::Type = syn::parse_quote! { i32 };

        let generated = generate_stream_exports(
            &type_name,
            "EventBus",
            MethodCallable::new(method),
            &return_lowering(),
            &item_type,
        )
        .to_string();

        assert!(
            generated.contains(
                "output_ptr : * mut < i32 as :: boltffi :: __private :: Passable > :: Out"
            )
        );
        assert!(
            generated.contains("< i32 as :: boltffi :: __private :: Passable > :: pack (item)")
        );
        assert!(!generated.contains("VecTransport"));
    }

    #[test]
    fn encoded_stream_pop_batch_returns_wire_encoded_batch() {
        let impl_block = parse_impl(
            r#"
            impl EventBus {
                #[ffi_stream(item = UserProfile)]
                pub fn subscribe_profiles(&self) -> std::sync::Arc<EventSubscription<UserProfile>> {
                    todo!()
                }
            }
            "#,
        );
        let method = impl_block
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) => Some(method),
                _ => None,
            })
            .expect("stream method should exist");
        let type_name = impl_type_name(&impl_block).expect("impl type name should resolve");
        let item_type: syn::Type = syn::parse_quote! { UserProfile };

        let generated = generate_stream_exports(
            &type_name,
            "EventBus",
            MethodCallable::new(method),
            &return_lowering(),
            &item_type,
        )
        .to_string();

        assert!(generated.contains("max_count : usize"));
        assert!(generated.contains("-> :: boltffi :: __private :: FfiBuf"));
        assert!(generated.contains(
            ":: boltffi :: __private :: FfiBuf :: wire_encode (& __boltffi_stream_items)"
        ));
        assert!(!generated.contains("VecTransport"));
    }
}
