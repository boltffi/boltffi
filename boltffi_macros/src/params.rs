use crate::{
    callback_registry::CallbackTraitRegistry,
    custom_types::{CustomTypeRegistry, contains_custom_types, from_wire_expr_owned, to_wire_expr_owned, wire_type_for},
    util::{ParamTransform, WireEncodedParam, WireEncodedParamKind, classify_param_transform, foreign_trait_path, is_primitive_vec_inner, len_ident, ptr_ident},
};
use boltffi_ffi_rules::callback as cb_naming;
use proc_macro2::Span;
use quote::quote;
use syn::{FnArg, Pat};

fn generate_wasm_closure_codegen(
    name: &syn::Ident,
    arg_types: &[syn::Type],
    returns: Option<&syn::Type>,
    ffi_cb_args: &[proc_macro2::TokenStream],
    custom_types: &CustomTypeRegistry,
) -> proc_macro2::TokenStream {
    let type_ids: Vec<cb_naming::TypeId> = arg_types
        .iter()
        .map(|ty| {
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            cb_naming::TypeId::from_rust_type_str(&ty_str)
        })
        .collect();

    let return_type_id = returns
        .map(|ty| {
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            cb_naming::TypeId::from_rust_type_str(&ty_str)
        })
        .unwrap_or(cb_naming::TypeId::Void);

    let callback_id_snake = cb_naming::closure_callback_id_snake(&type_ids, &return_type_id);
    let call_import_name = cb_naming::callback_wasm_import_call(&callback_id_snake);
    let free_import_name = cb_naming::callback_wasm_import_free(&callback_id_snake);

    let call_import_ident = syn::Ident::new(&call_import_name, name.span());
    let free_import_ident = syn::Ident::new(&free_import_name, name.span());
    let owner_name = syn::Ident::new(&format!("__{}_owner", name), name.span());

    let (arg_names, wire_vars, call_args): (Vec<_>, Vec<_>, Vec<_>) = arg_types
        .iter()
        .enumerate()
        .map(|(index, arg_ty)| {
            let arg_name = syn::Ident::new(&format!("__arg{}", index), name.span());
            let arg_ty_str = quote!(#arg_ty).to_string().replace(' ', "");

            if is_primitive_vec_inner(&arg_ty_str) {
                (arg_name.clone(), quote! {}, quote! { #arg_name })
            } else {
                let wire_name = syn::Ident::new(&format!("__wire{}", index), name.span());
                let wire_var = if contains_custom_types(arg_ty, custom_types) {
                    let wire_ty = wire_type_for(arg_ty, custom_types);
                    let wire_value_ident = syn::Ident::new(&format!("__wire_value{}", index), name.span());
                    let to_wire = to_wire_expr_owned(arg_ty, custom_types, &arg_name);
                    quote! {
                        let #wire_value_ident: #wire_ty = { #to_wire };
                        let #wire_name = ::boltffi::__private::wire::encode(&#wire_value_ident);
                    }
                } else {
                    quote! {
                        let #wire_name = ::boltffi::__private::wire::encode(&#arg_name);
                    }
                };
                (arg_name, wire_var, quote! { #wire_name.as_ptr(), #wire_name.len() })
            }
        })
        .fold((vec![], vec![], vec![]), |(mut names, mut vars, mut args), (n, v, a)| {
            names.push(n);
            vars.push(v);
            args.push(a);
            (names, vars, args)
        });

    let closure_params: Vec<proc_macro2::TokenStream> = arg_names.iter().zip(arg_types.iter()).map(|(n, t)| quote! { #n: #t }).collect();

    let closure_params_tokens = if closure_params.is_empty() {
        quote! {}
    } else {
        let first = &closure_params[0];
        let rest = &closure_params[1..];
        quote! { #first #(, #rest)* }
    };

    let mut extern_param_idx = 0;
    let extern_params: Vec<proc_macro2::TokenStream> = ffi_cb_args
        .iter()
        .map(|t| {
            let param_name = syn::Ident::new(&format!("__p{}", extern_param_idx), name.span());
            extern_param_idx += 1;
            quote! { #param_name: #t }
        })
        .collect();

    let extern_params_tokens = if extern_params.is_empty() {
        quote! {}
    } else {
        let first = &extern_params[0];
        let rest = &extern_params[1..];
        quote! { , #first #(, #rest)* }
    };

    let return_is_primitive = returns
        .map(|ty| {
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            is_primitive_vec_inner(&ty_str)
        })
        .unwrap_or(true);

    if return_is_primitive {
        let ffi_return_type = returns.map(|ty| quote! { -> #ty }).unwrap_or_default();
        let closure_return_type = ffi_return_type.clone();

        quote! {
            #[cfg(target_arch = "wasm32")]
            let #name = {
                #[allow(improper_ctypes)]
                unsafe extern "C" {
                    fn #call_import_ident(handle: u32 #extern_params_tokens) #ffi_return_type;
                    fn #free_import_ident(handle: u32);
                }
                let #owner_name = ::boltffi::__private::WasmCallbackOwner::new(#name, #free_import_ident);
                move |#closure_params_tokens| #closure_return_type {
                    #(#wire_vars)*
                    unsafe { #call_import_ident(#owner_name.handle() #(, #call_args)*) }
                }
            };
        }
    } else {
        let return_ty = returns.unwrap();
        let from_wire = if contains_custom_types(return_ty, custom_types) {
            let wire_ty = wire_type_for(return_ty, custom_types);
            let wire_result_ident = syn::Ident::new("__wire_result", name.span());
            let from_wire_conversion = from_wire_expr_owned(return_ty, custom_types, &wire_result_ident);
            quote! {
                let #wire_result_ident: #wire_ty = ::boltffi::__private::wire::decode(__result_bytes)
                    .expect("closure return: wire decode failed");
                #from_wire_conversion
            }
        } else {
            quote! {
                ::boltffi::__private::wire::decode(__result_bytes)
                    .expect("closure return: wire decode failed")
            }
        };

        quote! {
            #[cfg(target_arch = "wasm32")]
            let #name = {
                #[allow(improper_ctypes)]
                unsafe extern "C" {
                    fn #call_import_ident(handle: u32, out: *mut ::boltffi::__private::FfiBuf<u8> #extern_params_tokens);
                    fn #free_import_ident(handle: u32);
                }
                let #owner_name = ::boltffi::__private::WasmCallbackOwner::new(#name, #free_import_ident);
                move |#closure_params_tokens| -> #return_ty {
                    #(#wire_vars)*
                    let mut __out_buf = ::boltffi::__private::FfiBuf::<u8>::empty();
                    unsafe { #call_import_ident(#owner_name.handle(), &mut __out_buf #(, #call_args)*) };
                    let __result_bytes = unsafe {
                        ::core::slice::from_raw_parts(__out_buf.as_ptr(), __out_buf.len())
                    };
                    #from_wire
                }
            };
        }
    }
}

pub struct FfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub conversions: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
}

struct ImplTraitResolution {
    foreign_type: proc_macro2::TokenStream,
    error: Option<proc_macro2::TokenStream>,
}

fn impl_trait_resolution(trait_path: &syn::Path, callback_registry: &CallbackTraitRegistry) -> ImplTraitResolution {
    if let Some(resolution) = callback_registry.resolve(trait_path) {
        let foreign_path = resolution.foreign_path;
        if resolution.is_object_safe {
            return ImplTraitResolution {
                foreign_type: quote! {
                    <dyn #trait_path as ::boltffi::__private::CallbackForeignType>::Foreign
                },
                error: None,
            };
        }
        return ImplTraitResolution {
            foreign_type: quote! { #foreign_path },
            error: None,
        };
    }

    let foreign_path = foreign_trait_path(trait_path);
    let trait_name = quote!(#trait_path).to_string();
    let message = format!(
        "boltffi: cannot resolve callback trait `impl {}`. If this is a cross-crate async callback, use the full module path or make the trait object-safe with #[async_trait], e.g. `impl crate::path::to::{}` or `Box<dyn {}>`.",
        trait_name, trait_name, trait_name
    );
    let message_lit = syn::LitStr::new(&message, Span::call_site());
    ImplTraitResolution {
        foreign_type: quote! { #foreign_path },
        error: Some(quote! { compile_error!(#message_lit); }),
    }
}

fn wire_bytes_expression(ptr_name: &syn::Ident, len_name: &syn::Ident, requires_unsafe: bool) -> proc_macro2::TokenStream {
    if requires_unsafe {
        quote! { unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) } }
    } else {
        quote! { ::core::slice::from_raw_parts(#ptr_name, #len_name) }
    }
}

fn utf8_str_expression(name: &syn::Ident, ptr_name: &syn::Ident, len_name: &syn::Ident, requires_unsafe: bool) -> proc_macro2::TokenStream {
    let bytes_expr = wire_bytes_expression(ptr_name, len_name, requires_unsafe);
    quote! {
        match ::core::str::from_utf8(#bytes_expr) {
            Ok(value) => value,
            Err(error) => {
                ::boltffi::__private::set_last_error(format!(
                    "{}: invalid UTF-8: {} (buf_len={})",
                    stringify!(#name),
                    error,
                    #len_name
                ));
                ""
            }
        }
    }
}

fn utf8_string_expression(name: &syn::Ident, ptr_name: &syn::Ident, len_name: &syn::Ident, requires_unsafe: bool) -> proc_macro2::TokenStream {
    let bytes_expr = wire_bytes_expression(ptr_name, len_name, requires_unsafe);
    quote! {
        match ::core::str::from_utf8(#bytes_expr) {
            Ok(value) => value.to_string(),
            Err(error) => {
                ::boltffi::__private::set_last_error(format!(
                    "{}: invalid UTF-8: {} (buf_len={})",
                    stringify!(#name),
                    error,
                    #len_name
                ));
                String::new()
            }
        }
    }
}

fn wire_empty_value_expression(kind: WireEncodedParamKind) -> proc_macro2::TokenStream {
    match kind {
        WireEncodedParamKind::Vec => quote! { Vec::new() },
        WireEncodedParamKind::Option => quote! { None },
        WireEncodedParamKind::Record => quote! {},
    }
}

fn wire_decode_conversion(
    name: &syn::Ident,
    wire_param: &WireEncodedParam,
    ptr_name: &syn::Ident,
    len_name: &syn::Ident,
    custom_types: &CustomTypeRegistry,
    requires_unsafe: bool,
) -> proc_macro2::TokenStream {
    let rust_type = &wire_param.rust_type;
    let bytes_expr = wire_bytes_expression(ptr_name, len_name, requires_unsafe);

    if contains_custom_types(rust_type, custom_types) {
        let wire_ty = wire_type_for(rust_type, custom_types);
        let wire_value_ident = syn::Ident::new("__boltffi_wire_value", name.span());
        let from_wire = from_wire_expr_owned(rust_type, custom_types, &wire_value_ident);

        return match wire_param.kind {
            WireEncodedParamKind::Vec | WireEncodedParamKind::Option => {
                let empty_value = wire_empty_value_expression(wire_param.kind);
                quote! {
                    let #name: #rust_type = if #ptr_name.is_null() || #len_name == 0 {
                        #empty_value
                    } else {
                        let __bytes = #bytes_expr;
                        match ::boltffi::__private::wire::decode::<#wire_ty>(__bytes) {
                            Ok(#wire_value_ident) => { #from_wire },
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: wire decode failed: {} (buf_len={})",
                                    stringify!(#name),
                                    error,
                                    #len_name
                                ));
                                #empty_value
                            }
                        }
                    };
                }
            }
            WireEncodedParamKind::Record => quote! {
                let #name: #rust_type = {
                    assert!(!#ptr_name.is_null(), concat!(stringify!(#name), ": null pointer"));
                    let __bytes = #bytes_expr;
                    let #wire_value_ident: #wire_ty = ::boltffi::__private::wire::decode(__bytes)
                        .unwrap_or_else(|error| {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: wire decode failed: {} (buf_len={})",
                                stringify!(#name),
                                error,
                                #len_name
                            ));
                            panic!(
                                "{}: wire decode failed: {} (buf_len={})",
                                stringify!(#name),
                                error,
                                #len_name
                            )
                        });
                    #from_wire
                };
            },
        };
    }

    match wire_param.kind {
        WireEncodedParamKind::Vec | WireEncodedParamKind::Option => {
            let empty_value = wire_empty_value_expression(wire_param.kind);
            quote! {
                let #name: #rust_type = if #ptr_name.is_null() || #len_name == 0 {
                    #empty_value
                } else {
                    let __bytes = #bytes_expr;
                    match ::boltffi::__private::wire::decode::<#rust_type>(__bytes) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: wire decode failed: {} (buf_len={})",
                                stringify!(#name),
                                error,
                                #len_name
                            ));
                            #empty_value
                        }
                    }
                };
            }
        }
        WireEncodedParamKind::Record => quote! {
            let #name: #rust_type = {
                assert!(!#ptr_name.is_null(), concat!(stringify!(#name), ": null pointer"));
                let __bytes = #bytes_expr;
                ::boltffi::__private::wire::decode(__bytes).unwrap_or_else(|error| {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: wire decode failed: {} (buf_len={})",
                        stringify!(#name),
                        error,
                        #len_name
                    ));
                    panic!(
                        "{}: wire decode failed: {} (buf_len={})",
                        stringify!(#name),
                        error,
                        #len_name
                    )
                })
            };
        },
    }
}

fn push_wire_encoded_param(
    ffi_params: &mut Vec<proc_macro2::TokenStream>,
    conversions: &mut Vec<proc_macro2::TokenStream>,
    name: &syn::Ident,
    wire_param: &WireEncodedParam,
    custom_types: &CustomTypeRegistry,
    requires_unsafe: bool,
) {
    let ptr_name = ptr_ident(name);
    let len_name = len_ident(name);
    ffi_params.push(quote! { #ptr_name: *const u8 });
    ffi_params.push(quote! { #len_name: usize });
    conversions.push(wire_decode_conversion(name, wire_param, &ptr_name, &len_name, custom_types, requires_unsafe));
}

pub struct AsyncFfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub pre_spawn: Vec<proc_macro2::TokenStream>,
    pub thread_setup: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
    pub move_vars: Vec<syn::Ident>,
}

#[derive(Clone, Copy)]
enum UnsupportedAsyncParam {
    Callback,
    MutableSlice,
    TraitObject,
}

impl UnsupportedAsyncParam {
    fn error_message(self) -> &'static str {
        match self {
            Self::Callback => "boltffi: async exports do not support closure callback parameters yet",
            Self::MutableSlice => "boltffi: async exports do not support mutable slice parameters (`&mut [T]`)",
            Self::TraitObject => "boltffi: async exports do not support trait object callback parameters (`Box<dyn Trait>`, `Arc<dyn Trait>`, `Option<Arc<dyn Trait>>`) yet",
        }
    }
}

fn unsupported_async_param(transform: &ParamTransform) -> Option<UnsupportedAsyncParam> {
    match transform {
        ParamTransform::Callback { .. } => Some(UnsupportedAsyncParam::Callback),
        ParamTransform::SliceMut(_) => Some(UnsupportedAsyncParam::MutableSlice),
        ParamTransform::BoxedDynTrait(_) | ParamTransform::ArcDynTrait(_) | ParamTransform::OptionArcDynTrait(_) => Some(UnsupportedAsyncParam::TraitObject),
        ParamTransform::StrRef
        | ParamTransform::OwnedString
        | ParamTransform::SliceRef(_)
        | ParamTransform::VecPrimitive(_)
        | ParamTransform::WireEncoded(_)
        | ParamTransform::ImplTrait(_)
        | ParamTransform::PassThrough => None,
    }
}

fn validate_async_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>) -> syn::Result<()> {
    inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(pat_type),
            FnArg::Receiver(_) => None,
        })
        .filter_map(|pat_type| {
            let param_transform = classify_param_transform(&pat_type.ty);
            unsupported_async_param(&param_transform).map(|unsupported| syn::Error::new_spanned(&pat_type.ty, unsupported.error_message()))
        })
        .reduce(|mut left, right| {
            left.combine(right);
            left
        })
        .map_or(Ok(()), Err)
}

#[derive(Clone, Copy)]
enum ParamExecutionMode {
    Sync,
    Async,
}

impl ParamExecutionMode {
    fn requires_unsafe_wire_decode(&self) -> bool {
        matches!(self, Self::Async)
    }
}

struct ParamLoweringState {
    ffi_params: Vec<proc_macro2::TokenStream>,
    setup: Vec<proc_macro2::TokenStream>,
    thread_setup: Vec<proc_macro2::TokenStream>,
    call_args: Vec<proc_macro2::TokenStream>,
    move_vars: Vec<syn::Ident>,
}

impl ParamLoweringState {
    fn into_sync(self) -> FfiParams {
        FfiParams {
            ffi_params: self.ffi_params,
            conversions: self.setup,
            call_args: self.call_args,
        }
    }

    fn into_async(self) -> AsyncFfiParams {
        AsyncFfiParams {
            ffi_params: self.ffi_params,
            pre_spawn: self.setup,
            thread_setup: self.thread_setup,
            call_args: self.call_args,
            move_vars: self.move_vars,
        }
    }
}

fn lower_callback_param_transform(
    acc: &mut ParamLoweringState,
    name: &syn::Ident,
    arg_types: &[syn::Type],
    returns: &Option<syn::Type>,
    mode: ParamExecutionMode,
    custom_types: &CustomTypeRegistry,
) {
    match mode {
        ParamExecutionMode::Async => {
            unreachable!("async callback params are rejected during macro validation");
        }
        ParamExecutionMode::Sync => {
            let cb_name = syn::Ident::new(&format!("{}_cb", name), name.span());
            let ud_name = syn::Ident::new(&format!("{}_ud", name), name.span());

            let (ffi_cb_args, arg_names, cb_call_args, wire_vars) = arg_types.iter().enumerate().fold(
                (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                |(mut ffi_cb_args, mut arg_names, mut cb_call_args, mut wire_vars), (index, arg_ty)| {
                    let arg_name = syn::Ident::new(&format!("__arg{}", index), name.span());
                    let arg_ty_str = quote!(#arg_ty).to_string().replace(' ', "");

                    if is_primitive_vec_inner(&arg_ty_str) {
                        ffi_cb_args.push(quote! { #arg_ty });
                        cb_call_args.push(quote! { #arg_name });
                    } else {
                        let wire_name = syn::Ident::new(&format!("__wire{}", index), name.span());
                        ffi_cb_args.push(quote! { *const u8 });
                        ffi_cb_args.push(quote! { usize });
                        let wire_vars_expr = if contains_custom_types(arg_ty, custom_types) {
                            let wire_ty = wire_type_for(arg_ty, custom_types);
                            let wire_value_ident = syn::Ident::new(&format!("__wire_value{}", index), name.span());
                            let to_wire = to_wire_expr_owned(arg_ty, custom_types, &arg_name);
                            quote! {
                                let #wire_value_ident: #wire_ty = { #to_wire };
                                let #wire_name = ::boltffi::__private::wire::encode(&#wire_value_ident);
                            }
                        } else {
                            quote! {
                                let #wire_name = ::boltffi::__private::wire::encode(&#arg_name);
                            }
                        };
                        wire_vars.push(wire_vars_expr);
                        cb_call_args.push(quote! { #wire_name.as_ptr() });
                        cb_call_args.push(quote! { #wire_name.len() });
                    }

                    arg_names.push(arg_name);
                    (ffi_cb_args, arg_names, cb_call_args, wire_vars)
                },
            );

            let ffi_return_type = returns.as_ref().map(|ty| quote! { -> #ty }).unwrap_or_default();
            let closure_return_type = returns.as_ref().map(|ty| quote! { -> #ty }).unwrap_or_default();

            let closure_params: Vec<proc_macro2::TokenStream> = arg_names.iter().zip(arg_types.iter()).map(|(n, t)| quote! { #n: #t }).collect();

            acc.ffi_params.push(quote! {
                #[cfg(not(target_arch = "wasm32"))]
                #cb_name: extern "C" fn(*mut ::core::ffi::c_void, #(#ffi_cb_args),*) #ffi_return_type,
                #[cfg(not(target_arch = "wasm32"))]
                #ud_name: *mut ::core::ffi::c_void,
                #[cfg(target_arch = "wasm32")]
                #name: u32
            });

            let wasm_codegen = generate_wasm_closure_codegen(name, arg_types, returns.as_ref(), &ffi_cb_args, custom_types);

            acc.setup.push(quote! {
                #[cfg(not(target_arch = "wasm32"))]
                let #name = |#(#closure_params),*| #closure_return_type {
                    #(#wire_vars)*
                    #cb_name(#ud_name, #(#cb_call_args),*)
                };
                #wasm_codegen
            });

            acc.call_args.push(quote! { #name });
        }
    }
}

fn lower_impl_trait_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, trait_path: &syn::Path, mode: ParamExecutionMode, callback_registry: &CallbackTraitRegistry) {
    let resolution = impl_trait_resolution(trait_path, callback_registry);
    let foreign_type = resolution.foreign_type;

    acc.ffi_params.push(quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #name: ::boltffi::__private::CallbackHandle,
        #[cfg(target_arch = "wasm32")]
        #name: u32
    });

    match mode {
        ParamExecutionMode::Sync => {
            if let Some(error) = resolution.error {
                acc.setup.push(error);
            }
            acc.setup.push(quote! {
                #[cfg(not(target_arch = "wasm32"))]
                assert!(!#name.is_null(), concat!(stringify!(#name), ": null callback handle"));
                #[cfg(target_arch = "wasm32")]
                let #name = ::boltffi::__private::CallbackHandle::from_wasm_handle(#name);
                let #name = unsafe {
                    <#foreign_type as ::boltffi::__private::FromCallbackHandle>::box_from_callback_handle(#name)
                };
            });
            acc.call_args.push(quote! { *#name });
        }
        ParamExecutionMode::Async => {
            if let Some(error) = resolution.error {
                acc.setup.push(error);
            }
            let boxed_name = syn::Ident::new(&format!("{}_boxed", name), name.span());
            acc.setup.push(quote! {
                #[cfg(not(target_arch = "wasm32"))]
                assert!(!#name.is_null(), concat!(stringify!(#name), ": null callback handle"));
                #[cfg(target_arch = "wasm32")]
                let #name = ::boltffi::__private::CallbackHandle::from_wasm_handle(#name);
                let #boxed_name = unsafe {
                    <#foreign_type as ::boltffi::__private::FromCallbackHandle>::box_from_callback_handle(#name)
                };
            });
            acc.move_vars.push(boxed_name.clone());
            acc.call_args.push(quote! { *#boxed_name });
        }
    }
}

fn lower_str_ref_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, mode: ParamExecutionMode) {
    let ptr_name = ptr_ident(name);
    let len_name = len_ident(name);
    let sync_str_expr = utf8_str_expression(name, &ptr_name, &len_name, false);
    let async_string_expr = utf8_string_expression(name, &ptr_name, &len_name, true);
    acc.ffi_params.push(quote! { #ptr_name: *const u8 });
    acc.ffi_params.push(quote! { #len_name: usize });

    match mode {
        ParamExecutionMode::Sync => {
            acc.setup.push(quote! {
                let #name: &str = if #ptr_name.is_null() {
                    ""
                } else {
                    #sync_str_expr
                };
            });
        }
        ParamExecutionMode::Async => {
            let owned_name = syn::Ident::new(&format!("{}_owned", name), name.span());
            acc.setup.push(quote! {
                let #owned_name: String = if #ptr_name.is_null() {
                    String::new()
                } else {
                    #async_string_expr
                };
            });
            acc.thread_setup.push(quote! {
                let #name: &str = &#owned_name;
            });
            acc.move_vars.push(owned_name);
        }
    }

    acc.call_args.push(quote! { #name });
}

fn lower_owned_string_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, mode: ParamExecutionMode) {
    let ptr_name = ptr_ident(name);
    let len_name = len_ident(name);
    let sync_string_expr = utf8_string_expression(name, &ptr_name, &len_name, false);
    let async_string_expr = utf8_string_expression(name, &ptr_name, &len_name, true);
    acc.ffi_params.push(quote! { #ptr_name: *const u8 });
    acc.ffi_params.push(quote! { #len_name: usize });

    match mode {
        ParamExecutionMode::Sync => {
            acc.setup.push(quote! {
                let #name: String = if #ptr_name.is_null() {
                    String::new()
                } else {
                    #sync_string_expr
                };
            });
        }
        ParamExecutionMode::Async => {
            acc.setup.push(quote! {
                let #name: String = if #ptr_name.is_null() {
                    String::new()
                } else {
                    #async_string_expr
                };
            });
            acc.move_vars.push(name.clone());
        }
    }

    acc.call_args.push(quote! { #name });
}

fn lower_slice_ref_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, inner_ty: &syn::Type, mode: ParamExecutionMode) {
    let ptr_name = ptr_ident(name);
    let len_name = len_ident(name);
    acc.ffi_params.push(quote! { #ptr_name: *const #inner_ty });
    acc.ffi_params.push(quote! { #len_name: usize });

    match mode {
        ParamExecutionMode::Sync => {
            acc.setup.push(quote! {
                let #name: &[#inner_ty] = if #ptr_name.is_null() {
                    &[]
                } else {
                    ::core::slice::from_raw_parts(#ptr_name, #len_name)
                };
            });
        }
        ParamExecutionMode::Async => {
            let owned_name = syn::Ident::new(&format!("{}_vec", name), name.span());
            acc.setup.push(quote! {
                let #owned_name: Vec<#inner_ty> = if #ptr_name.is_null() {
                    Vec::new()
                } else {
                    unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
                };
            });
            acc.thread_setup.push(quote! {
                let #name: &[#inner_ty] = &#owned_name;
            });
            acc.move_vars.push(owned_name);
        }
    }

    acc.call_args.push(quote! { #name });
}

fn lower_slice_mut_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, inner_ty: &syn::Type, mode: ParamExecutionMode) {
    match mode {
        ParamExecutionMode::Sync => {
            let ptr_name = ptr_ident(name);
            let len_name = len_ident(name);
            acc.ffi_params.push(quote! { #ptr_name: *mut #inner_ty });
            acc.ffi_params.push(quote! { #len_name: usize });
            acc.setup.push(quote! {
                let #name: &mut [#inner_ty] = if #ptr_name.is_null() {
                    &mut []
                } else {
                    ::core::slice::from_raw_parts_mut(#ptr_name, #len_name)
                };
            });
            acc.call_args.push(quote! { #name });
        }
        ParamExecutionMode::Async => {
            unreachable!("async mutable slices are rejected during macro validation");
        }
    }
}

fn lower_vec_primitive_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, inner_ty: &syn::Type, mode: ParamExecutionMode) {
    let ptr_name = ptr_ident(name);
    let len_name = len_ident(name);
    acc.ffi_params.push(quote! { #ptr_name: *const #inner_ty });
    acc.ffi_params.push(quote! { #len_name: usize });

    match mode {
        ParamExecutionMode::Sync => {
            acc.setup.push(quote! {
                let #name: Vec<#inner_ty> = if #ptr_name.is_null() {
                    Vec::new()
                } else {
                    ::core::slice::from_raw_parts(#ptr_name, #len_name).to_vec()
                };
            });
        }
        ParamExecutionMode::Async => {
            acc.setup.push(quote! {
                let #name: Vec<#inner_ty> = if #ptr_name.is_null() {
                    Vec::new()
                } else {
                    unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
                };
            });
            acc.move_vars.push(name.clone());
        }
    }

    acc.call_args.push(quote! { #name });
}

fn lower_wire_encoded_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, wire_param: &WireEncodedParam, custom_types: &CustomTypeRegistry, mode: ParamExecutionMode) {
    push_wire_encoded_param(&mut acc.ffi_params, &mut acc.setup, name, wire_param, custom_types, mode.requires_unsafe_wire_decode());
    if matches!(mode, ParamExecutionMode::Async) {
        acc.move_vars.push(name.clone());
    }
    acc.call_args.push(quote! { #name });
}

#[derive(Clone, Copy)]
enum TraitObjectParamKind {
    Boxed,
    Arc,
    OptionArc,
}

fn lower_trait_object_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, trait_path: &syn::Path, mode: ParamExecutionMode, kind: TraitObjectParamKind) {
    match mode {
        ParamExecutionMode::Async => {
            unreachable!("async trait object params are rejected during macro validation");
        }
        ParamExecutionMode::Sync => {
            acc.ffi_params.push(quote! {
                #[cfg(not(target_arch = "wasm32"))]
                #name: ::boltffi::__private::CallbackHandle,
                #[cfg(target_arch = "wasm32")]
                #name: u32
            });
            let setup = match kind {
                TraitObjectParamKind::Boxed => quote! {
                    #[cfg(not(target_arch = "wasm32"))]
                    assert!(!#name.is_null(), concat!(stringify!(#name), ": null callback handle"));
                    #[cfg(target_arch = "wasm32")]
                    let #name = ::boltffi::__private::CallbackHandle::from_wasm_handle(#name);
                    let #name: Box<dyn #trait_path> = unsafe {
                        <dyn #trait_path as ::boltffi::__private::FromCallbackHandle>::box_from_callback_handle(#name)
                    };
                },
                TraitObjectParamKind::Arc => quote! {
                    #[cfg(not(target_arch = "wasm32"))]
                    assert!(!#name.is_null(), concat!(stringify!(#name), ": null callback handle"));
                    #[cfg(target_arch = "wasm32")]
                    let #name = ::boltffi::__private::CallbackHandle::from_wasm_handle(#name);
                    let #name: ::std::sync::Arc<dyn #trait_path> = unsafe {
                        <dyn #trait_path as ::boltffi::__private::FromCallbackHandle>::arc_from_callback_handle(#name)
                    };
                },
                TraitObjectParamKind::OptionArc => quote! {
                    #[cfg(target_arch = "wasm32")]
                    let #name = ::boltffi::__private::CallbackHandle::from_wasm_handle(#name);
                    let #name: Option<::std::sync::Arc<dyn #trait_path>> = if #name.is_null() {
                        None
                    } else {
                        Some(unsafe {
                            <dyn #trait_path as ::boltffi::__private::FromCallbackHandle>::arc_from_callback_handle(#name)
                        })
                    };
                },
            };
            acc.setup.push(setup);
            acc.call_args.push(quote! { #name });
        }
    }
}

fn lower_pass_through_param_transform(acc: &mut ParamLoweringState, name: &syn::Ident, ty: &syn::Type, mode: ParamExecutionMode) {
    acc.ffi_params.push(quote! { #name: #ty });
    if matches!(mode, ParamExecutionMode::Async) {
        acc.move_vars.push(name.clone());
    }
    acc.call_args.push(quote! { #name });
}

fn transform_params_with_mode(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    custom_types: &CustomTypeRegistry,
    callback_registry: &CallbackTraitRegistry,
    mode: ParamExecutionMode,
) -> ParamLoweringState {
    inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(pat_type),
            FnArg::Receiver(_) => None,
        })
        .fold(
            ParamLoweringState {
                ffi_params: Vec::new(),
                setup: Vec::new(),
                thread_setup: Vec::new(),
                call_args: Vec::new(),
                move_vars: Vec::new(),
            },
            |mut acc, pat_type| {
                let Some(name) = (match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                }) else {
                    return acc;
                };

                match classify_param_transform(&pat_type.ty) {
                    ParamTransform::StrRef => lower_str_ref_param_transform(&mut acc, &name, mode),
                    ParamTransform::OwnedString => lower_owned_string_param_transform(&mut acc, &name, mode),
                    ParamTransform::Callback { params: arg_types, returns } => lower_callback_param_transform(&mut acc, &name, &arg_types, &returns, mode, custom_types),
                    ParamTransform::SliceRef(inner_ty) => lower_slice_ref_param_transform(&mut acc, &name, &inner_ty, mode),
                    ParamTransform::SliceMut(inner_ty) => lower_slice_mut_param_transform(&mut acc, &name, &inner_ty, mode),
                    ParamTransform::BoxedDynTrait(trait_path) => lower_trait_object_param_transform(&mut acc, &name, &trait_path, mode, TraitObjectParamKind::Boxed),
                    ParamTransform::ArcDynTrait(trait_path) => lower_trait_object_param_transform(&mut acc, &name, &trait_path, mode, TraitObjectParamKind::Arc),
                    ParamTransform::OptionArcDynTrait(trait_path) => lower_trait_object_param_transform(&mut acc, &name, &trait_path, mode, TraitObjectParamKind::OptionArc),
                    ParamTransform::VecPrimitive(inner_ty) => lower_vec_primitive_param_transform(&mut acc, &name, &inner_ty, mode),
                    ParamTransform::WireEncoded(wire_param) => lower_wire_encoded_param_transform(&mut acc, &name, &wire_param, custom_types, mode),
                    ParamTransform::ImplTrait(trait_path) => {
                        lower_impl_trait_param_transform(&mut acc, &name, &trait_path, mode, callback_registry);
                    }
                    ParamTransform::PassThrough => lower_pass_through_param_transform(&mut acc, &name, &pat_type.ty, mode),
                }

                acc
            },
        )
}

pub fn transform_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>, custom_types: &CustomTypeRegistry, callback_registry: &CallbackTraitRegistry) -> FfiParams {
    transform_params_with_mode(inputs, custom_types, callback_registry, ParamExecutionMode::Sync).into_sync()
}

pub fn transform_params_async(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    custom_types: &CustomTypeRegistry,
    callback_registry: &CallbackTraitRegistry,
) -> syn::Result<AsyncFfiParams> {
    validate_async_params(inputs)?;
    Ok(transform_params_with_mode(inputs, custom_types, callback_registry, ParamExecutionMode::Async).into_async())
}

pub fn transform_method_params(inputs: impl Iterator<Item = syn::FnArg>, custom_types: &CustomTypeRegistry, callback_registry: &CallbackTraitRegistry) -> FfiParams {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params(&function_like_inputs, custom_types, callback_registry)
}

pub fn transform_method_params_async(
    inputs: impl Iterator<Item = syn::FnArg>,
    custom_types: &CustomTypeRegistry,
    callback_registry: &CallbackTraitRegistry,
) -> syn::Result<AsyncFfiParams> {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params_async(&function_like_inputs, custom_types, callback_registry)
}

#[cfg(test)]
mod tests {
    use super::validate_async_params;
    use syn::parse_quote;

    #[test]
    fn rejects_async_callback_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(callback: impl Fn(i32) -> i32) {}
        };

        let error = validate_async_params(&function.sig.inputs).expect_err("expected rejection");
        assert!(error.to_string().contains("do not support closure callback parameters yet"));
    }

    #[test]
    fn rejects_async_mutable_slice_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(values: &mut [i32]) {}
        };

        let error = validate_async_params(&function.sig.inputs).expect_err("expected rejection");
        assert!(error.to_string().contains("do not support mutable slice parameters"));
    }

    #[test]
    fn rejects_async_trait_object_params() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(
                boxed: Box<dyn ExampleTrait>,
                shared: std::sync::Arc<dyn ExampleTrait>,
                optional: Option<std::sync::Arc<dyn ExampleTrait>>
            ) {}
        };

        let error = validate_async_params(&function.sig.inputs).expect_err("expected rejection");
        assert!(error.to_string().contains("do not support trait object callback parameters"));
    }

    #[test]
    fn accepts_supported_async_params() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(name: String, ids: Vec<i32>, scores: &[i32], id: i64) {}
        };

        assert!(validate_async_params(&function.sig.inputs).is_ok());
    }
}
