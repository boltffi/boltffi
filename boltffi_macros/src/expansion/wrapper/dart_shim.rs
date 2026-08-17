//! Per-callback dual-path stubs compiled into the user's crate when
//! `cfg(boltffi_dart)` is set (`pack dart` / `build dart`).
//!
//! Same-thread: call the Dart `fromFunction` pointer.
//! Other thread: post a listener and wait on a gate. Wire buffers stay
//! alive because the rust caller does not drop them until this stub
//! returns.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

pub struct DartShimMethod {
    pub slot: Ident,
    pub fast_field: Ident,
    pub listener_field: Ident,
    pub fast_fn_type: TokenStream,
    pub listener_fn_type: TokenStream,
    pub params: Vec<TokenStream>,
    pub args: Vec<TokenStream>,
    pub is_void_return: bool,
    pub listener_has_out: bool,
    pub return_type: TokenStream,
}

/// Builds the no_mangle prefix from the C register symbol
/// (`boltffi_register_callback_<path>`), so two traits with the same leaf
/// name in different modules/crates do not collide at link time.
pub fn dart_shim_prefix(register_symbol: &str) -> String {
    const REGISTER_PREFIX: &str = "boltffi_register_callback_";
    let path = register_symbol
        .strip_prefix(REGISTER_PREFIX)
        .unwrap_or(register_symbol);
    format!("BoltFFIDartShim_{path}")
}

pub fn render(
    register_symbol: &str,
    trait_name: &Ident,
    methods: &[DartShimMethod],
) -> TokenStream {
    let prefix = dart_shim_prefix(register_symbol);
    let register_ident = format_ident!("{prefix}_register");
    let release_ident = format_ident!("{prefix}_release");
    let free_ident = format_ident!("{prefix}_free");
    let clone_ident = format_ident!("{prefix}_clone");
    let hooks_ident = format_ident!("__BoltffiDartHooks_{trait_name}");

    let method_fields: Vec<TokenStream> = methods
        .iter()
        .map(|method| {
            let fast = &method.fast_field;
            let listener = &method.listener_field;
            let fast_ty = &method.fast_fn_type;
            let listener_ty = &method.listener_fn_type;
            quote! {
                #fast: #fast_ty,
                #listener: #listener_ty,
            }
        })
        .collect();

    let register_params: Vec<TokenStream> = methods
        .iter()
        .flat_map(|method| {
            let fast = &method.fast_field;
            let listener = &method.listener_field;
            let fast_ty = &method.fast_fn_type;
            let listener_ty = &method.listener_fn_type;
            [
                quote! { #fast: #fast_ty },
                quote! { #listener: #listener_ty },
            ]
        })
        .collect();

    let register_field_inits: Vec<TokenStream> = methods
        .iter()
        .flat_map(|method| {
            let fast = &method.fast_field;
            let listener = &method.listener_field;
            [quote! { #fast }, quote! { #listener }]
        })
        .collect();

    let method_shims: Vec<TokenStream> = methods
        .iter()
        .map(|method| render_method_shim(&prefix, &hooks_ident, method))
        .collect();

    quote! {
        #[allow(unexpected_cfgs)]
        const _: () = {
            #[cfg(boltffi_dart)]
            const _: () = {
            #[repr(C)]
            struct #hooks_ident {
                header: ::boltffi::__dart_runtime::HooksHeader,
                free_fast: unsafe extern "C" fn(u64),
                free_listener: unsafe extern "C" fn(u64, *mut ::core::ffi::c_void),
                clone_fast: unsafe extern "C" fn(u64) -> u64,
                clone_listener: unsafe extern "C" fn(u64, *mut ::core::ffi::c_void, *mut u64),
                #(#method_fields)*
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #register_ident(
                free_fast: unsafe extern "C" fn(u64),
                free_listener: unsafe extern "C" fn(u64, *mut ::core::ffi::c_void),
                clone_fast: unsafe extern "C" fn(u64) -> u64,
                clone_listener: unsafe extern "C" fn(u64, *mut ::core::ffi::c_void, *mut u64),
                #(#register_params),*
            ) -> u64 {
                let hooks = ::std::boxed::Box::new(#hooks_ident {
                    header: ::boltffi::__dart_runtime::HooksHeader::new(),
                    free_fast,
                    free_listener,
                    clone_fast,
                    clone_listener,
                    #(#register_field_inits),*
                });
                ::std::boxed::Box::into_raw(hooks) as u64
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #release_ident(handle: u64) {
                if handle == 0 {
                    return;
                }
                let hooks = unsafe { &*(handle as *const #hooks_ident) };
                hooks.header.shutdown();
                drop(unsafe { ::std::boxed::Box::from_raw(handle as *mut #hooks_ident) });
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #free_ident(handle: u64) {
                dispatch_void(handle, |hooks| hooks.free_fast, |hooks| hooks.free_listener);
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #clone_ident(handle: u64) -> u64 {
                dispatch_ret(handle, 0, |hooks| hooks.clone_fast, |hooks| hooks.clone_listener)
            }

            #(#method_shims)*

            unsafe fn dispatch_void(
                handle: u64,
                fast: impl FnOnce(&#hooks_ident) -> unsafe extern "C" fn(u64),
                listener: impl FnOnce(&#hooks_ident) -> unsafe extern "C" fn(u64, *mut ::core::ffi::c_void),
            ) {
                let Some(hooks) = (unsafe { handle_hooks::<#hooks_ident>(handle) }) else {
                    return;
                };
                if hooks.header.is_owner() {
                    unsafe { (fast(hooks))(handle) };
                    return;
                }
                let Some(pending) = hooks.header.create_gate() else {
                    return;
                };
                unsafe { (listener(hooks))(handle, pending.raw()) };
                pending.wait();
            }

            unsafe fn dispatch_ret(
                handle: u64,
                zero: u64,
                fast: impl FnOnce(&#hooks_ident) -> unsafe extern "C" fn(u64) -> u64,
                listener: impl FnOnce(&#hooks_ident) -> unsafe extern "C" fn(u64, *mut ::core::ffi::c_void, *mut u64),
            ) -> u64 {
                let Some(hooks) = (unsafe { handle_hooks::<#hooks_ident>(handle) }) else {
                    return zero;
                };
                if hooks.header.is_owner() {
                    return unsafe { (fast(hooks))(handle) };
                }
                let Some(pending) = hooks.header.create_gate() else {
                    return zero;
                };
                let out = ::std::boxed::Box::into_raw(::std::boxed::Box::new(zero));
                unsafe { pending.own_out_ptr(out) };
                unsafe { (listener(hooks))(handle, pending.raw(), out) };
                let status = pending.wait();
                if !matches!(status, ::boltffi::__dart_runtime::CallStatus::Ok) {
                    return 0;
                }
                pending.disarm_out();
                *unsafe { ::std::boxed::Box::from_raw(out) }
            }

            unsafe fn handle_hooks<'a, T>(handle: u64) -> Option<&'a T> {
                if handle == 0 {
                    None
                } else {
                    Some(unsafe { &*(handle as *const T) })
                }
            }
            };
        };
    }
}

fn render_method_shim(prefix: &str, hooks_ident: &Ident, method: &DartShimMethod) -> TokenStream {
    let symbol = format_ident!("{prefix}_{}", method.slot);
    let fast_field = &method.fast_field;
    let listener_field = &method.listener_field;
    let params = &method.params;
    let args = &method.args;
    let is_void = method.is_void_return;
    let listener_has_out = method.listener_has_out;

    if is_void {
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #symbol(handle: u64, #(#params),*) {
                let Some(hooks) = (unsafe { handle_hooks::<#hooks_ident>(handle) }) else {
                    return;
                };
                if hooks.header.is_owner() {
                    unsafe { (hooks.#fast_field)(handle, #(#args),*) };
                    return;
                }
                let Some(pending) = hooks.header.create_gate() else {
                    return;
                };
                unsafe { (hooks.#listener_field)(handle, #(#args,)* pending.raw()) };
                let status = pending.wait();
                if !matches!(status, ::boltffi::__dart_runtime::CallStatus::Ok) {
                    ::core::panic!("Dart callback failed on a foreign thread");
                }
            }
        }
    } else if listener_has_out {
        let ret = &method.return_type;
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #symbol(handle: u64, #(#params),*) -> #ret {
                let Some(hooks) = (unsafe { handle_hooks::<#hooks_ident>(handle) }) else {
                    return unsafe { ::core::mem::zeroed() };
                };
                if hooks.header.is_owner() {
                    return unsafe { (hooks.#fast_field)(handle, #(#args),*) };
                }
                let Some(pending) = hooks.header.create_gate() else {
                    return unsafe { ::core::mem::zeroed() };
                };
                let out = ::std::boxed::Box::into_raw(::std::boxed::Box::new(unsafe {
                    ::core::mem::zeroed()
                }));
                unsafe { pending.own_out_ptr(out) };
                unsafe { (hooks.#listener_field)(handle, #(#args,)* pending.raw(), out) };
                let status = pending.wait();
                if !matches!(status, ::boltffi::__dart_runtime::CallStatus::Ok) {
                    return unsafe { ::core::mem::zeroed() };
                }
                pending.disarm_out();
                *unsafe { ::std::boxed::Box::from_raw(out) }
            }
        }
    } else {
        let ret = &method.return_type;
        quote! {
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #symbol(handle: u64, #(#params),*) -> #ret {
                let Some(hooks) = (unsafe { handle_hooks::<#hooks_ident>(handle) }) else {
                    return unsafe { ::core::mem::zeroed() };
                };
                if hooks.header.is_owner() {
                    return unsafe { (hooks.#fast_field)(handle, #(#args),*) };
                }
                let Some(pending) = hooks.header.create_gate() else {
                    return unsafe { ::core::mem::zeroed() };
                };
                unsafe { (hooks.#listener_field)(handle, #(#args,)* pending.raw()) };
                let status = pending.wait();
                if !matches!(status, ::boltffi::__dart_runtime::CallStatus::Ok) {
                    return unsafe { ::core::mem::zeroed() };
                }
                unsafe { ::core::mem::zeroed() }
            }
        }
    }
}
