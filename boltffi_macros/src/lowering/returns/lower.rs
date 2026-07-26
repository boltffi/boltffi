use boltffi_ffi_rules::primitive::Primitive;
use proc_macro2::Span;
use quote::quote;
use syn::Type;

use crate::index::custom_types::{self, CustomTypeRegistry};

use super::classify::{ReturnTypeDescriptor, option_primitive_uses_scalar_encoding};
use super::model::{
    DirectBufferReturnMethod, EncodedReturnStrategy, ObjectHandleReturn, ResolvedReturn,
    ReturnInvocationContext, ReturnPlatform, ScalarReturnStrategy, ValueReturnStrategy,
    WasmOptionScalarEncoding,
};

impl ResolvedReturn {
    pub fn invalid_arg_early_return_statement(&self) -> proc_macro2::TokenStream {
        match self.value_return_strategy() {
            ValueReturnStrategy::Void => quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            },
            ValueReturnStrategy::Scalar(_) => quote! {
                return ::core::default::Default::default();
            },
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar) => {
                let _ = WasmOptionScalarEncoding::from_option_rust_type(self.rust_type())
                    .expect("OptionScalar return must have a primitive Option inner type");
                quote! {
                    return f64::NAN;
                }
            }
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec) => {
                quote! {
                    #[cfg(target_arch = "wasm32")]
                    {
                        return;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        return ::boltffi::__private::FfiBuf::default();
                    }
                }
            }
            ValueReturnStrategy::Buffer(_) => match (
                self.direct_buffer_return_method(
                    ReturnInvocationContext::SyncExport,
                    ReturnPlatform::Wasm,
                ),
                self.direct_buffer_return_method(
                    ReturnInvocationContext::SyncExport,
                    ReturnPlatform::Native,
                ),
            ) {
                (
                    Some(DirectBufferReturnMethod::Packed),
                    Some(DirectBufferReturnMethod::Descriptor),
                ) => quote! {
                    #[cfg(target_arch = "wasm32")]
                    {
                        return ::boltffi::__private::FfiBuf::default().into_packed();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        return ::boltffi::__private::FfiBuf::default();
                    }
                },
                methods => panic!(
                    "unexpected direct buffer return methods for sync export invalid-arg return: {:?}",
                    methods
                ),
            },
            ValueReturnStrategy::CompositeValue => {
                let rust_type = self.rust_type();
                quote! {
                    return unsafe {
                        ::core::mem::MaybeUninit::<<#rust_type as ::boltffi::__private::Passable>::Out>::zeroed().assume_init()
                    };
                }
            }
            ValueReturnStrategy::ObjectHandle | ValueReturnStrategy::CallbackHandle => quote! {
                return ::core::default::Default::default();
            },
        }
    }

    pub fn wasm_invalid_arg_early_return_statement(&self) -> proc_macro2::TokenStream {
        match self.value_return_strategy() {
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec) => quote! {
                return;
            },
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar) => {
                let _ = WasmOptionScalarEncoding::from_option_rust_type(self.rust_type())
                    .expect("OptionScalar return must have a primitive Option inner type");
                quote! {
                    return f64::NAN;
                }
            }
            ValueReturnStrategy::Buffer(_) => match self.direct_buffer_return_method(
                ReturnInvocationContext::SyncExport,
                ReturnPlatform::Wasm,
            ) {
                Some(DirectBufferReturnMethod::Packed) => quote! {
                    return ::boltffi::__private::FfiBuf::default().into_packed();
                },
                method => panic!(
                    "unexpected wasm direct buffer return method for invalid-arg return: {:?}",
                    method
                ),
            },
            _ => self.invalid_arg_early_return_statement(),
        }
    }

    pub fn native_invalid_arg_early_return_statement(&self) -> proc_macro2::TokenStream {
        match self.value_return_strategy() {
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::DirectVec)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::Utf8String)
            | ValueReturnStrategy::Buffer(EncodedReturnStrategy::ResultScalar) => quote! {
                return ::boltffi::__private::FfiBuf::default();
            },
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar) => quote! {
                return ::boltffi::__private::FfiBuf::default();
            },
            _ => self.invalid_arg_early_return_statement(),
        }
    }

    pub fn async_ffi_return_type(&self) -> proc_macro2::TokenStream {
        let rust_type = self.rust_type();
        match self.value_return_strategy() {
            ValueReturnStrategy::Void => quote! { () },
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue) => {
                quote! { #rust_type }
            }
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag)
            | ValueReturnStrategy::CompositeValue => {
                quote! { <#rust_type as ::boltffi::__private::Passable>::Out }
            }
            ValueReturnStrategy::Buffer(_) => quote! { ::boltffi::__private::FfiBuf },
            ValueReturnStrategy::ObjectHandle => self
                .object_handle()
                .expect("object handle return must carry handle metadata")
                .pointer_type(),
            ValueReturnStrategy::CallbackHandle => quote! { #rust_type },
        }
    }

    pub fn async_rust_return_type(&self) -> proc_macro2::TokenStream {
        let rust_type = self.rust_type();
        match self.value_return_strategy() {
            ValueReturnStrategy::Void => quote! { () },
            _ => quote! { #rust_type },
        }
    }

    pub fn async_invalid_arg_early_return_statement(&self) -> proc_macro2::TokenStream {
        let rust_return_type = self.async_rust_return_type();
        quote! {
            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
        }
    }

    pub fn async_complete_conversion(
        &self,
        return_lowering: &super::model::ReturnLoweringContext<'_>,
    ) -> proc_macro2::TokenStream {
        match self.value_return_strategy() {
            ValueReturnStrategy::Void => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                ()
            },
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::PrimitiveValue) => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                result
            },
            ValueReturnStrategy::Scalar(ScalarReturnStrategy::CStyleEnumTag)
            | ValueReturnStrategy::CompositeValue => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                ::boltffi::__private::Passable::pack(result)
            },
            ValueReturnStrategy::Buffer(strategy) => {
                let result_ident = syn::Ident::new("result", Span::call_site());
                let encode_expression = encoded_return_buffer_expression(
                    self.rust_type(),
                    strategy,
                    &result_ident,
                    Some(return_lowering.custom_types()),
                );
                quote! {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                    #encode_expression
                }
            }
            ValueReturnStrategy::ObjectHandle => {
                let object_handle = self
                    .object_handle()
                    .expect("object handle return must carry handle metadata");
                let pointer_expression = object_handle.raw_pointer_expression(quote! { result });
                quote! {
                    if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                    #pointer_expression
                }
            }
            ValueReturnStrategy::CallbackHandle => quote! {
                if !out_status.is_null() { *out_status = ::boltffi::__private::FfiStatus::OK; }
                result
            },
        }
    }

    pub fn async_default_ffi_value(&self) -> proc_macro2::TokenStream {
        match self.value_return_strategy() {
            ValueReturnStrategy::Void => quote! { () },
            ValueReturnStrategy::Scalar(_)
            | ValueReturnStrategy::CompositeValue
            | ValueReturnStrategy::ObjectHandle
            | ValueReturnStrategy::CallbackHandle => quote! { Default::default() },
            ValueReturnStrategy::Buffer(_) => quote! { ::boltffi::__private::FfiBuf::default() },
        }
    }
}

impl ObjectHandleReturn {
    pub fn pointer_type(&self) -> proc_macro2::TokenStream {
        let pointee = self.pointee();
        quote! { *mut #pointee }
    }

    pub fn ffi_return_type(&self) -> proc_macro2::TokenStream {
        let pointer_type = self.pointer_type();
        quote! { -> #pointer_type }
    }

    pub fn raw_pointer_expression(
        &self,
        value: proc_macro2::TokenStream,
    ) -> proc_macro2::TokenStream {
        if self.is_nullable() {
            quote! {
                match #value {
                    Some(value) => Box::into_raw(Box::new(value)),
                    None => ::core::ptr::null_mut(),
                }
            }
        } else if self.is_fallible() {
            quote! {
                match #value {
                    Ok(value) => Box::into_raw(Box::new(value)),
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!("{error:?}"));
                        ::core::ptr::null_mut()
                    }
                }
            }
        } else {
            quote! {
                Box::into_raw(Box::new(#value))
            }
        }
    }
}

impl WasmOptionScalarEncoding {
    pub fn from_option_rust_type(rust_type: &Type) -> Option<Self> {
        ReturnTypeDescriptor::parse(rust_type)
            .option_primitive()
            .filter(|primitive| option_primitive_uses_scalar_encoding(*primitive))
            .map(|primitive| Self { primitive })
    }

    pub fn some_expression(self, value_ident: &syn::Ident) -> proc_macro2::TokenStream {
        match self.primitive {
            Primitive::Bool => quote! {
                if #value_ident { 1.0 } else { 0.0 }
            },
            Primitive::F64 => quote! { #value_ident },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32 => quote! {
                #value_ident as f64
            },
        }
    }
}

pub fn encoded_return_body(
    rust_type: &syn::Type,
    strategy: EncodedReturnStrategy,
    result_ident: &syn::Ident,
    evaluate_result_expression: proc_macro2::TokenStream,
    conversions: &[proc_macro2::TokenStream],
    custom_type_registry: &CustomTypeRegistry,
) -> proc_macro2::TokenStream {
    let encode_expression = encoded_return_buffer_expression(
        rust_type,
        strategy,
        result_ident,
        Some(custom_type_registry),
    );

    quote! {
        #(#conversions)*
        let #result_ident: #rust_type = #evaluate_result_expression;
        #encode_expression
    }
}

pub fn encoded_return_buffer_expression(
    rust_type: &syn::Type,
    strategy: EncodedReturnStrategy,
    result_ident: &syn::Ident,
    custom_type_registry: Option<&CustomTypeRegistry>,
) -> proc_macro2::TokenStream {
    match strategy {
        EncodedReturnStrategy::DirectVec => quote! {
            <_ as ::boltffi::__private::VecTransport>::pack_vec(#result_ident)
        },
        EncodedReturnStrategy::Utf8String => quote! {
            #[cfg(target_arch = "wasm32")]
            {
                ::boltffi::__private::FfiBuf::from_vec(#result_ident.into_bytes())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
            }
        },
        EncodedReturnStrategy::OptionScalar | EncodedReturnStrategy::ResultScalar => quote! {
            ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
        },
        EncodedReturnStrategy::WireEncoded => {
            wire_encode_expression(rust_type, result_ident, custom_type_registry)
        }
    }
}

fn wire_encode_expression(
    rust_type: &syn::Type,
    result_ident: &syn::Ident,
    custom_type_registry: Option<&CustomTypeRegistry>,
) -> proc_macro2::TokenStream {
    match custom_type_registry {
        Some(registry) if custom_types::contains_custom_types(rust_type, registry) => {
            let wire_ty = custom_types::wire_type_for(rust_type, registry);
            let wire_value_ident = syn::Ident::new("__boltffi_wire_value", result_ident.span());
            let to_wire = custom_types::to_wire_expr_owned(rust_type, registry, result_ident);
            quote! {
                let #wire_value_ident: #wire_ty = { #to_wire };
                ::boltffi::__private::FfiBuf::wire_encode(&#wire_value_ident)
            }
        }
        _ => quote! {
            ::boltffi::__private::FfiBuf::wire_encode(&#result_ident)
        },
    }
}
