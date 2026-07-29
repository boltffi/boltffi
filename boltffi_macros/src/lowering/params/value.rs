use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use super::ParamLoweringState;
use super::transform::{
    ClassHandleParam, ClassHandleParamKind, ParamTransform, PassableParam, PassablePassing,
    WireEncodedParam, WireEncodedParamKind, WireEncodedPassing, len_ident, ptr_ident,
};
use crate::index::custom_types::{
    CustomTypeRegistry, contains_custom_types, from_wire_expr_owned, wire_type_for,
};

struct ValueParamDecoder<'a> {
    custom_types: &'a CustomTypeRegistry,
    on_wire_record_error: &'a TokenStream,
}

impl<'a> ValueParamDecoder<'a> {
    fn new(custom_types: &'a CustomTypeRegistry, on_wire_record_error: &'a TokenStream) -> Self {
        Self {
            custom_types,
            on_wire_record_error,
        }
    }

    fn wire_bytes_expression(
        &self,
        ptr_name: &Ident,
        len_name: &Ident,
        requires_unsafe: bool,
    ) -> TokenStream {
        if requires_unsafe {
            quote! { unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) } }
        } else {
            quote! { ::core::slice::from_raw_parts(#ptr_name, #len_name) }
        }
    }

    fn utf8_str_expression(
        &self,
        name: &Ident,
        ptr_name: &Ident,
        len_name: &Ident,
        requires_unsafe: bool,
    ) -> TokenStream {
        let bytes_expr = self.wire_bytes_expression(ptr_name, len_name, requires_unsafe);
        quote! {
            match ::core::str::from_utf8(#bytes_expr) {
                Ok(value) => value,
                Err(error) => {
                    ::boltffi::__private::set_last_error_display(stringify!(#name), "invalid UTF-8", &error, #len_name as usize);
                    ""
                }
            }
        }
    }

    fn utf8_string_expression(
        &self,
        name: &Ident,
        ptr_name: &Ident,
        len_name: &Ident,
        requires_unsafe: bool,
    ) -> TokenStream {
        let bytes_expr = self.wire_bytes_expression(ptr_name, len_name, requires_unsafe);
        quote! {
            match ::core::str::from_utf8(#bytes_expr) {
                Ok(value) => value.to_string(),
                Err(error) => {
                    ::boltffi::__private::set_last_error_display(stringify!(#name), "invalid UTF-8", &error, #len_name as usize);
                    String::new()
                }
            }
        }
    }

    fn wire_empty_value_expression(&self, kind: WireEncodedParamKind) -> TokenStream {
        match kind {
            WireEncodedParamKind::Required => quote! { unreachable!() },
            WireEncodedParamKind::Vec => quote! { Vec::new() },
            WireEncodedParamKind::Option => quote! { None },
        }
    }

    fn wire_decode_conversion(
        &self,
        name: &Ident,
        wire_param: &WireEncodedParam,
        ptr_name: &Ident,
        len_name: &Ident,
        requires_unsafe: bool,
    ) -> TokenStream {
        let rust_type = &wire_param.decoded_type;
        let bytes_expr = self.wire_bytes_expression(ptr_name, len_name, requires_unsafe);
        let on_wire_record_error = self.on_wire_record_error;

        if contains_custom_types(rust_type, self.custom_types) {
            let wire_type = wire_type_for(rust_type, self.custom_types);
            let wire_value_ident = Ident::new("__boltffi_wire_value", name.span());
            let from_wire = from_wire_expr_owned(rust_type, self.custom_types, &wire_value_ident);

            if matches!(wire_param.kind, WireEncodedParamKind::Required) {
                return quote! {
                    let #name: #rust_type = {
                        if #ptr_name.is_null() && #len_name > 0 {
                            ::boltffi::__private::set_last_error_len(stringify!(#name), "null pointer with non-zero length", #len_name as usize);
                            #on_wire_record_error
                        }
                        let __bytes: &[u8] = if #len_name == 0 {
                            &[]
                        } else {
                            #bytes_expr
                        };
                        let #wire_value_ident: #wire_type = match ::boltffi::__private::wire::decode(__bytes) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error_display(stringify!(#name), "wire decode failed", &error, #len_name as usize);
                                #on_wire_record_error
                            }
                        };
                        #from_wire
                    };
                };
            }

            let empty_value = self.wire_empty_value_expression(wire_param.kind);
            return quote! {
                let #name: #rust_type = if #ptr_name.is_null() || #len_name == 0 {
                    #empty_value
                } else {
                    let __bytes = #bytes_expr;
                    match ::boltffi::__private::wire::decode::<#wire_type>(__bytes) {
                        Ok(#wire_value_ident) => { #from_wire },
                        Err(error) => {
                            ::boltffi::__private::set_last_error_display(stringify!(#name), "wire decode failed", &error, #len_name as usize);
                            #empty_value
                        }
                    }
                };
            };
        }

        if matches!(wire_param.kind, WireEncodedParamKind::Required) {
            return quote! {
                let #name: #rust_type = {
                    if #ptr_name.is_null() && #len_name > 0 {
                        ::boltffi::__private::set_last_error_len(stringify!(#name), "null pointer with non-zero length", #len_name as usize);
                        #on_wire_record_error
                    }
                    let __bytes: &[u8] = if #len_name == 0 {
                        &[]
                    } else {
                        #bytes_expr
                    };
                    match ::boltffi::__private::wire::decode(__bytes) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error_display(stringify!(#name), "wire decode failed", &error, #len_name as usize);
                            #on_wire_record_error
                        }
                    }
                };
            };
        }

        let empty_value = self.wire_empty_value_expression(wire_param.kind);
        quote! {
            let #name: #rust_type = if #ptr_name.is_null() || #len_name == 0 {
                #empty_value
            } else {
                let __bytes = #bytes_expr;
                match ::boltffi::__private::wire::decode::<#rust_type>(__bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        ::boltffi::__private::set_last_error_display(stringify!(#name), "wire decode failed", &error, #len_name as usize);
                        #empty_value
                    }
                }
            };
        }
    }

    fn push_wire_encoded_param(
        &self,
        ffi_params: &mut Vec<TokenStream>,
        conversions: &mut Vec<TokenStream>,
        name: &Ident,
        wire_param: &WireEncodedParam,
        requires_unsafe: bool,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        ffi_params.push(quote! { #ptr_name: *const u8 });
        ffi_params.push(quote! { #len_name: usize });
        conversions.push(self.wire_decode_conversion(
            name,
            wire_param,
            &ptr_name,
            &len_name,
            requires_unsafe,
        ));
    }

    fn lower_sync_str_ref_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let sync_str_expr = self.utf8_str_expression(name, &ptr_name, &len_name, false);
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: &str = if #ptr_name.is_null() {
                ""
            } else {
                #sync_str_expr
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_str_ref_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let async_string_expr = self.utf8_string_expression(name, &ptr_name, &len_name, true);
        let owned_name = Ident::new(&format!("{}_owned", name), name.span());
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
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
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_string_ref_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let sync_string_expr = self.utf8_string_expression(name, &ptr_name, &len_name, false);
        let owned_name = Ident::new(&format!("{}_owned", name), name.span());
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #owned_name: String = if #ptr_name.is_null() {
                String::new()
            } else {
                #sync_string_expr
            };
            let #name: &String = &#owned_name;
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_string_ref_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let async_string_expr = self.utf8_string_expression(name, &ptr_name, &len_name, true);
        let owned_name = Ident::new(&format!("{}_owned", name), name.span());
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #owned_name: String = if #ptr_name.is_null() {
                String::new()
            } else {
                #async_string_expr
            };
        });
        acc.thread_setup.push(quote! {
            let #name: &String = &#owned_name;
        });
        acc.move_vars.push(owned_name);
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_owned_string_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let sync_string_expr = self.utf8_string_expression(name, &ptr_name, &len_name, false);
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: String = if #ptr_name.is_null() {
                String::new()
            } else {
                #sync_string_expr
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_owned_string_param(&self, acc: &mut ParamLoweringState, name: &Ident) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let async_string_expr = self.utf8_string_expression(name, &ptr_name, &len_name, true);
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: String = if #ptr_name.is_null() {
                String::new()
            } else {
                #async_string_expr
            };
        });
        acc.move_vars.push(name.clone());
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_slice_ref_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        acc.ffi_params
            .push(quote! { #ptr_name: *const #inner_type });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: &[#inner_type] = if #ptr_name.is_null() {
                &[]
            } else {
                ::core::slice::from_raw_parts(#ptr_name, #len_name)
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_slice_ref_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let owned_name = Ident::new(&format!("{}_vec", name), name.span());
        acc.ffi_params
            .push(quote! { #ptr_name: *const #inner_type });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #owned_name: Vec<#inner_type> = if #ptr_name.is_null() {
                Vec::new()
            } else {
                unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
            };
        });
        acc.thread_setup.push(quote! {
            let #name: &[#inner_type] = &#owned_name;
        });
        acc.move_vars.push(owned_name);
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_slice_mut_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        acc.ffi_params.push(quote! { #ptr_name: *mut #inner_type });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: &mut [#inner_type] = if #ptr_name.is_null() {
                &mut []
            } else {
                ::core::slice::from_raw_parts_mut(#ptr_name, #len_name)
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_vec_primitive_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        acc.ffi_params
            .push(quote! { #ptr_name: *const #inner_type });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: Vec<#inner_type> = if #ptr_name.is_null() {
                Vec::new()
            } else {
                ::core::slice::from_raw_parts(#ptr_name, #len_name).to_vec()
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_vec_primitive_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        acc.ffi_params
            .push(quote! { #ptr_name: *const #inner_type });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: Vec<#inner_type> = if #ptr_name.is_null() {
                Vec::new()
            } else {
                unsafe { ::core::slice::from_raw_parts(#ptr_name, #len_name) }.to_vec()
            };
        });
        acc.move_vars.push(name.clone());
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_vec_passable_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let on_wire_record_error = self.on_wire_record_error;
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: Vec<#inner_type> = if #ptr_name.is_null() {
                Vec::new()
            } else {
                let raw_byte_len = #len_name;
                let element_size = ::core::mem::size_of::<<#inner_type as ::boltffi::__private::Passable>::In>();
                if raw_byte_len % element_size != 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                        raw_byte_len,
                        ::core::any::type_name::<#inner_type>(),
                        element_size
                    ));
                    #on_wire_record_error
                }
                unsafe {
                    <#inner_type as ::boltffi::__private::VecTransport>::unpack_vec(
                        #ptr_name,
                        raw_byte_len
                    )
                }
            };
        });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_vec_passable_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        inner_type: &syn::Type,
    ) {
        let ptr_name = ptr_ident(name);
        let len_name = len_ident(name);
        let on_wire_record_error = self.on_wire_record_error;
        acc.ffi_params.push(quote! { #ptr_name: *const u8 });
        acc.ffi_params.push(quote! { #len_name: usize });
        acc.setup.push(quote! {
            let #name: Vec<#inner_type> = if #ptr_name.is_null() {
                Vec::new()
            } else {
                let raw_byte_len = #len_name;
                let element_size = ::core::mem::size_of::<<#inner_type as ::boltffi::__private::Passable>::In>();
                if raw_byte_len % element_size != 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                        raw_byte_len,
                        ::core::any::type_name::<#inner_type>(),
                        element_size
                    ));
                    #on_wire_record_error
                }
                unsafe {
                    <#inner_type as ::boltffi::__private::VecTransport>::unpack_vec(
                        #ptr_name,
                        raw_byte_len
                    )
                }
            };
        });
        acc.move_vars.push(name.clone());
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_wire_encoded_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        wire_param: &WireEncodedParam,
    ) {
        match wire_param.passing {
            WireEncodedPassing::ByValue => {
                self.push_wire_encoded_param(
                    &mut acc.ffi_params,
                    &mut acc.setup,
                    name,
                    wire_param,
                    false,
                );
                acc.call_args.push(quote! { #name });
            }
            WireEncodedPassing::SharedRef | WireEncodedPassing::MutableRef => {
                let storage_name = Ident::new(&format!("{}_storage", name), name.span());
                self.push_wire_encoded_param(
                    &mut acc.ffi_params,
                    &mut acc.setup,
                    &storage_name,
                    wire_param,
                    false,
                );
                let binding = match wire_param.passing {
                    WireEncodedPassing::SharedRef => quote! { let #name = &#storage_name; },
                    WireEncodedPassing::MutableRef => quote! { let #name = &mut #storage_name; },
                    WireEncodedPassing::ByValue => unreachable!(),
                };
                acc.setup.push(binding);
                acc.call_args.push(quote! { #name });
            }
        }
    }

    fn lower_async_wire_encoded_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        wire_param: &WireEncodedParam,
    ) {
        match wire_param.passing {
            WireEncodedPassing::ByValue => {
                self.push_wire_encoded_param(
                    &mut acc.ffi_params,
                    &mut acc.setup,
                    name,
                    wire_param,
                    true,
                );
                acc.move_vars.push(name.clone());
                acc.call_args.push(quote! { #name });
            }
            WireEncodedPassing::SharedRef | WireEncodedPassing::MutableRef => {
                let storage_name = Ident::new(&format!("{}_storage", name), name.span());
                self.push_wire_encoded_param(
                    &mut acc.ffi_params,
                    &mut acc.setup,
                    &storage_name,
                    wire_param,
                    true,
                );
                let binding = match wire_param.passing {
                    WireEncodedPassing::SharedRef => quote! { let #name = &#storage_name; },
                    WireEncodedPassing::MutableRef => quote! { let #name = &mut #storage_name; },
                    WireEncodedPassing::ByValue => unreachable!(),
                };
                acc.thread_setup.push(binding);
                acc.move_vars.push(storage_name);
                acc.call_args.push(quote! { #name });
            }
        }
    }

    fn lower_sync_passable_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        passable_param: &PassableParam,
    ) {
        let rust_type = &passable_param.rust_type;
        acc.ffi_params
            .push(quote! { #name: <#rust_type as ::boltffi::__private::Passable>::In });
        match passable_param.passing {
            PassablePassing::ByValue => {
                acc.setup.push(passable_param.unpack_binding(name, name));
            }
            PassablePassing::SharedRef | PassablePassing::MutableRef => {
                let storage_name = Ident::new(&format!("{}_storage", name), name.span());
                acc.setup
                    .push(passable_param.unpack_binding(name, &storage_name));
                let binding = match passable_param.passing {
                    PassablePassing::SharedRef => quote! { let #name = &#storage_name; },
                    PassablePassing::MutableRef => quote! { let #name = &mut #storage_name; },
                    PassablePassing::ByValue => unreachable!(),
                };
                acc.setup.push(binding);
            }
        }
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_passable_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        passable_param: &PassableParam,
    ) {
        let rust_type = &passable_param.rust_type;
        acc.ffi_params
            .push(quote! { #name: <#rust_type as ::boltffi::__private::Passable>::In });
        match passable_param.passing {
            PassablePassing::ByValue => {
                acc.setup.push(passable_param.unpack_binding(name, name));
                acc.move_vars.push(name.clone());
            }
            PassablePassing::SharedRef | PassablePassing::MutableRef => {
                let storage_name = Ident::new(&format!("{}_storage", name), name.span());
                acc.setup
                    .push(passable_param.unpack_binding(name, &storage_name));
                let binding = match passable_param.passing {
                    PassablePassing::SharedRef => quote! { let #name = &#storage_name; },
                    PassablePassing::MutableRef => quote! { let #name = &mut #storage_name; },
                    PassablePassing::ByValue => unreachable!(),
                };
                acc.thread_setup.push(binding);
                acc.move_vars.push(storage_name);
            }
        }
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_class_handle_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        class_param: &ClassHandleParam,
    ) {
        let rust_type = &class_param.rust_type;
        let on_wire_record_error = self.on_wire_record_error;
        acc.ffi_params.push(quote! { #name: *mut #rust_type });
        let binding = class_handle_binding(name, class_param, on_wire_record_error);
        acc.setup.push(binding);
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_class_handle_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        class_param: &ClassHandleParam,
    ) {
        let rust_type = &class_param.rust_type;
        let on_wire_record_error = self.on_wire_record_error;
        acc.ffi_params.push(quote! { #name: *mut #rust_type });
        let binding = class_handle_binding(name, class_param, on_wire_record_error);
        acc.setup.push(binding);
        acc.call_args.push(quote! { #name });
    }

    fn lower_sync_pass_through_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        original_type: &syn::Type,
    ) {
        acc.ffi_params.push(quote! { #name: #original_type });
        acc.call_args.push(quote! { #name });
    }

    fn lower_async_pass_through_param(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        original_type: &syn::Type,
    ) {
        acc.ffi_params.push(quote! { #name: #original_type });
        acc.move_vars.push(name.clone());
        acc.call_args.push(quote! { #name });
    }
}

pub(super) struct SyncValueParamLowerer<'a> {
    decoder: ValueParamDecoder<'a>,
}

impl<'a> SyncValueParamLowerer<'a> {
    pub(super) fn new(
        custom_types: &'a CustomTypeRegistry,
        on_wire_record_error: &'a TokenStream,
    ) -> Self {
        Self {
            decoder: ValueParamDecoder::new(custom_types, on_wire_record_error),
        }
    }

    pub(super) fn lower_param_transform(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        original_type: &syn::Type,
        param_transform: ParamTransform,
    ) {
        match param_transform {
            ParamTransform::StrRef => self.decoder.lower_sync_str_ref_param(acc, name),
            ParamTransform::StringRef => self.decoder.lower_sync_string_ref_param(acc, name),
            ParamTransform::OwnedString => self.decoder.lower_sync_owned_string_param(acc, name),
            ParamTransform::SliceRef(inner_type) => {
                self.decoder
                    .lower_sync_slice_ref_param(acc, name, &inner_type)
            }
            ParamTransform::SliceMut(inner_type) => {
                self.decoder
                    .lower_sync_slice_mut_param(acc, name, &inner_type)
            }
            ParamTransform::VecPrimitive(inner_type) => self
                .decoder
                .lower_sync_vec_primitive_param(acc, name, &inner_type),
            ParamTransform::VecPassable(inner_type) => {
                self.decoder
                    .lower_sync_vec_passable_param(acc, name, &inner_type)
            }
            ParamTransform::ClassHandle(class_param) => {
                self.decoder
                    .lower_sync_class_handle_param(acc, name, &class_param)
            }
            ParamTransform::WireEncoded(wire_param) => {
                self.decoder
                    .lower_sync_wire_encoded_param(acc, name, &wire_param)
            }
            ParamTransform::Passable(ref passable_param)
                if contains_custom_types(&passable_param.rust_type, self.decoder.custom_types) =>
            {
                let wire_param = WireEncodedParam {
                    kind: WireEncodedParamKind::Required,
                    decoded_type: passable_param.rust_type.clone(),
                    passing: WireEncodedPassing::ByValue,
                };
                self.decoder
                    .lower_sync_wire_encoded_param(acc, name, &wire_param);
            }
            ParamTransform::Passable(passable_param) => {
                self.decoder
                    .lower_sync_passable_param(acc, name, &passable_param)
            }
            ParamTransform::PassThrough => {
                self.decoder
                    .lower_sync_pass_through_param(acc, name, original_type)
            }
            ParamTransform::Callback { .. }
            | ParamTransform::BoxedDynTrait(_)
            | ParamTransform::ArcDynTrait(_)
            | ParamTransform::OptionBoxedDynTrait(_)
            | ParamTransform::OptionArcDynTrait(_)
            | ParamTransform::ImplTrait(_) => {
                unreachable!("callback-shaped params must be lowered by callback lowerers")
            }
        }
    }
}

pub(super) struct AsyncValueParamLowerer<'a> {
    decoder: ValueParamDecoder<'a>,
}

impl<'a> AsyncValueParamLowerer<'a> {
    pub(super) fn new(
        custom_types: &'a CustomTypeRegistry,
        on_wire_record_error: &'a TokenStream,
    ) -> Self {
        Self {
            decoder: ValueParamDecoder::new(custom_types, on_wire_record_error),
        }
    }

    pub(super) fn lower_param_transform(
        &self,
        acc: &mut ParamLoweringState,
        name: &Ident,
        original_type: &syn::Type,
        param_transform: ParamTransform,
    ) {
        match param_transform {
            ParamTransform::StrRef => self.decoder.lower_async_str_ref_param(acc, name),
            ParamTransform::StringRef => self.decoder.lower_async_string_ref_param(acc, name),
            ParamTransform::OwnedString => self.decoder.lower_async_owned_string_param(acc, name),
            ParamTransform::SliceRef(inner_type) => {
                self.decoder
                    .lower_async_slice_ref_param(acc, name, &inner_type)
            }
            ParamTransform::VecPrimitive(inner_type) => self
                .decoder
                .lower_async_vec_primitive_param(acc, name, &inner_type),
            ParamTransform::VecPassable(inner_type) => {
                self.decoder
                    .lower_async_vec_passable_param(acc, name, &inner_type)
            }
            ParamTransform::ClassHandle(class_param) => self
                .decoder
                .lower_async_class_handle_param(acc, name, &class_param),
            ParamTransform::WireEncoded(wire_param) => {
                self.decoder
                    .lower_async_wire_encoded_param(acc, name, &wire_param)
            }
            ParamTransform::Passable(ref passable_param)
                if contains_custom_types(&passable_param.rust_type, self.decoder.custom_types) =>
            {
                let wire_param = WireEncodedParam {
                    kind: WireEncodedParamKind::Required,
                    decoded_type: passable_param.rust_type.clone(),
                    passing: WireEncodedPassing::ByValue,
                };
                self.decoder
                    .lower_async_wire_encoded_param(acc, name, &wire_param);
            }
            ParamTransform::Passable(passable_param) => {
                self.decoder
                    .lower_async_passable_param(acc, name, &passable_param)
            }
            ParamTransform::PassThrough => {
                self.decoder
                    .lower_async_pass_through_param(acc, name, original_type)
            }
            ParamTransform::SliceMut(_)
            | ParamTransform::Callback { .. }
            | ParamTransform::BoxedDynTrait(_)
            | ParamTransform::ArcDynTrait(_)
            | ParamTransform::OptionBoxedDynTrait(_)
            | ParamTransform::OptionArcDynTrait(_)
            | ParamTransform::ImplTrait(_) => {
                unreachable!("unsupported async params must be rejected before lowering")
            }
        }
    }
}

impl PassableParam {
    fn unpack_binding(&self, input_name: &Ident, output_name: &Ident) -> TokenStream {
        let rust_type = &self.rust_type;
        match self.passing {
            PassablePassing::MutableRef => quote! {
                let mut #output_name: #rust_type = unsafe { <#rust_type as ::boltffi::__private::Passable>::unpack(#input_name) };
            },
            PassablePassing::ByValue | PassablePassing::SharedRef => quote! {
                let #output_name: #rust_type = unsafe { <#rust_type as ::boltffi::__private::Passable>::unpack(#input_name) };
            },
        }
    }
}

fn class_handle_binding(
    name: &Ident,
    class_param: &ClassHandleParam,
    on_null: &TokenStream,
) -> TokenStream {
    let rust_type = &class_param.rust_type;
    match (class_param.kind, class_param.nullable) {
        (ClassHandleParamKind::SharedRef, false) => quote! {
            let #name: &#rust_type = if #name.is_null() {
                ::boltffi::__private::set_last_error(concat!(stringify!(#name), ": null class handle"));
                #on_null
            } else {
                unsafe { #name.as_ref().expect("checked non-null class handle") }
            };
        },
        (ClassHandleParamKind::MutableRef, false) => quote! {
            let #name: &mut #rust_type = if #name.is_null() {
                ::boltffi::__private::set_last_error(concat!(stringify!(#name), ": null class handle"));
                #on_null
            } else {
                unsafe { #name.as_mut().expect("checked non-null class handle") }
            };
        },
        (ClassHandleParamKind::SharedRef, true) => quote! {
            let #name: Option<&#rust_type> = if #name.is_null() {
                None
            } else {
                Some(unsafe { #name.as_ref().expect("checked non-null class handle") })
            };
        },
        (ClassHandleParamKind::MutableRef, true) => quote! {
            let #name: Option<&mut #rust_type> = if #name.is_null() {
                None
            } else {
                Some(unsafe { #name.as_mut().expect("checked non-null class handle") })
            };
        },
    }
}
