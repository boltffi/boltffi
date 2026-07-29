use syn::{FnArg, Pat};

mod callbacks;
mod transform;
mod value;

use self::callbacks::{AsyncCallbackParamLowerer, SyncCallbackParamLowerer, TraitObjectParamKind};
use self::transform::{ClassifiedParamTransform, ParamTransform, ParamTransformClassifier};
use self::value::{AsyncValueParamLowerer, SyncValueParamLowerer};
use crate::index::callback_traits::CallbackTraitRegistry;
use crate::lowering::returns::model::ReturnLoweringContext;
use boltffi_ffi_rules::transport::ParamValueStrategy;

pub struct FfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub conversions: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
}

pub struct AsyncFfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub pre_spawn: Vec<proc_macro2::TokenStream>,
    pub thread_setup: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
    pub move_vars: Vec<syn::Ident>,
}

pub(super) struct ParamLoweringState {
    pub(super) ffi_params: Vec<proc_macro2::TokenStream>,
    pub(super) setup: Vec<proc_macro2::TokenStream>,
    pub(super) thread_setup: Vec<proc_macro2::TokenStream>,
    pub(super) call_args: Vec<proc_macro2::TokenStream>,
    pub(super) move_vars: Vec<syn::Ident>,
}

impl ParamLoweringState {
    fn new() -> Self {
        Self {
            ffi_params: Vec::new(),
            setup: Vec::new(),
            thread_setup: Vec::new(),
            call_args: Vec::new(),
            move_vars: Vec::new(),
        }
    }

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

struct SyncParamLowerer<'a> {
    callback_param_lowerer: SyncCallbackParamLowerer<'a>,
    param_transform_classifier: ParamTransformClassifier<'a>,
    value_param_lowerer: SyncValueParamLowerer<'a>,
}

impl<'a> SyncParamLowerer<'a> {
    fn new(
        return_lowering: &'a ReturnLoweringContext<'a>,
        callback_registry: &'a CallbackTraitRegistry,
        on_wire_record_error: &'a proc_macro2::TokenStream,
    ) -> Self {
        Self {
            callback_param_lowerer: SyncCallbackParamLowerer::new(
                return_lowering,
                callback_registry,
            ),
            param_transform_classifier: ParamTransformClassifier::new(
                return_lowering.class_types(),
                return_lowering.named_type_transport_classifier(),
            ),
            value_param_lowerer: SyncValueParamLowerer::new(
                return_lowering.custom_types(),
                on_wire_record_error,
            ),
        }
    }

    fn lower_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> ParamLoweringState {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .fold(ParamLoweringState::new(), |mut acc, pat_type| {
                let Some(name) = (match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                }) else {
                    return acc;
                };

                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);

                match classified_param.contract.value_strategy() {
                    ParamValueStrategy::CallbackHandle { .. } => match classified_param.transform {
                        ParamTransform::Callback {
                            params: arg_types,
                            returns,
                        } => self
                            .callback_param_lowerer
                            .lower_callback_param(&mut acc, &name, &arg_types, &returns),
                        ParamTransform::BoxedDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::Boxed,
                            )
                        }
                        ParamTransform::ArcDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::Arc,
                            )
                        }
                        ParamTransform::OptionBoxedDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::OptionBoxed,
                            )
                        }
                        ParamTransform::OptionArcDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::OptionArc,
                            )
                        }
                        ParamTransform::ImplTrait(trait_path) => self
                            .callback_param_lowerer
                            .lower_impl_trait_param(&mut acc, &name, &trait_path),
                        param_transform => {
                            unreachable!(
                                "callback param contract must lower through callback builders: {:?}",
                                param_transform_name(&param_transform)
                            )
                        }
                    },
                    _ => self.value_param_lowerer.lower_param_transform(
                        &mut acc,
                        &name,
                        &pat_type.ty,
                        classified_param.transform,
                    ),
                }

                acc
            })
    }
}

struct AsyncParamLowerer<'a> {
    callback_param_lowerer: AsyncCallbackParamLowerer<'a>,
    param_transform_classifier: ParamTransformClassifier<'a>,
    value_param_lowerer: AsyncValueParamLowerer<'a>,
}

impl<'a> AsyncParamLowerer<'a> {
    fn new(
        return_lowering: &'a ReturnLoweringContext<'a>,
        callback_registry: &'a CallbackTraitRegistry,
        on_wire_record_error: &'a proc_macro2::TokenStream,
    ) -> Self {
        Self {
            callback_param_lowerer: AsyncCallbackParamLowerer::new(
                return_lowering,
                callback_registry,
            ),
            param_transform_classifier: ParamTransformClassifier::new(
                return_lowering.class_types(),
                return_lowering.named_type_transport_classifier(),
            ),
            value_param_lowerer: AsyncValueParamLowerer::new(
                return_lowering.custom_types(),
                on_wire_record_error,
            ),
        }
    }

    fn validate_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> syn::Result<()> {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .filter_map(|pat_type| {
                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);
                Self::unsupported_param_message(&classified_param)
                    .map(|message| syn::Error::new_spanned(&pat_type.ty, message))
            })
            .reduce(|mut left, right| {
                left.combine(right);
                left
            })
            .map_or(Ok(()), Err)
    }

    fn unsupported_param_message(
        classified_param: &ClassifiedParamTransform,
    ) -> Option<&'static str> {
        match &classified_param.transform {
            ParamTransform::Callback { .. } => {
                Some("boltffi: async exports do not support closure callback parameters yet")
            }
            ParamTransform::SliceMut(_) => {
                Some("boltffi: async exports do not support mutable slice parameters (`&mut [T]`)")
            }
            ParamTransform::BoxedDynTrait(_)
            | ParamTransform::ArcDynTrait(_)
            | ParamTransform::OptionBoxedDynTrait(_)
            | ParamTransform::OptionArcDynTrait(_) => Some(
                "boltffi: async exports do not support trait object callback parameters (`Box<dyn Trait>`, `Arc<dyn Trait>`, `Option<Box<dyn Trait>>`, `Option<Arc<dyn Trait>>`) yet",
            ),
            ParamTransform::StrRef
            | ParamTransform::StringRef
            | ParamTransform::OwnedString
            | ParamTransform::SliceRef(_)
            | ParamTransform::VecPrimitive(_)
            | ParamTransform::VecPassable(_)
            | ParamTransform::ClassHandle(_)
            | ParamTransform::WireEncoded(_)
            | ParamTransform::Passable(_)
            | ParamTransform::ImplTrait(_)
            | ParamTransform::PassThrough => None,
        }
    }

    fn lower_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> ParamLoweringState {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .fold(ParamLoweringState::new(), |mut acc, pat_type| {
                let Some(name) = (match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                }) else {
                    return acc;
                };

                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);

                match classified_param.contract.value_strategy() {
                    ParamValueStrategy::CallbackHandle { .. } => match classified_param.transform {
                        ParamTransform::ImplTrait(trait_path) => self
                            .callback_param_lowerer
                            .lower_impl_trait_param(&mut acc, &name, &trait_path),
                        ParamTransform::Callback { .. }
                        | ParamTransform::BoxedDynTrait(_)
                        | ParamTransform::ArcDynTrait(_)
                        | ParamTransform::OptionBoxedDynTrait(_)
                        | ParamTransform::OptionArcDynTrait(_)
                        | ParamTransform::SliceMut(_) => {
                            unreachable!(
                                "unsupported async params must be rejected during validation"
                            );
                        }
                        param_transform => {
                            unreachable!(
                                "callback param contract must lower through callback builders: {:?}",
                                param_transform_name(&param_transform)
                            )
                        }
                    },
                    _ => self.value_param_lowerer.lower_param_transform(
                        &mut acc,
                        &name,
                        &pat_type.ty,
                        classified_param.transform,
                    ),
                }

                acc
            })
    }
}

fn param_transform_name(param_transform: &ParamTransform) -> &'static str {
    match param_transform {
        ParamTransform::PassThrough => "PassThrough",
        ParamTransform::StrRef => "StrRef",
        ParamTransform::StringRef => "StringRef",
        ParamTransform::OwnedString => "OwnedString",
        ParamTransform::Callback { .. } => "Callback",
        ParamTransform::SliceRef(_) => "SliceRef",
        ParamTransform::SliceMut(_) => "SliceMut",
        ParamTransform::BoxedDynTrait(_) => "BoxedDynTrait",
        ParamTransform::ArcDynTrait(_) => "ArcDynTrait",
        ParamTransform::OptionBoxedDynTrait(_) => "OptionBoxedDynTrait",
        ParamTransform::OptionArcDynTrait(_) => "OptionArcDynTrait",
        ParamTransform::ImplTrait(_) => "ImplTrait",
        ParamTransform::VecPrimitive(_) => "VecPrimitive",
        ParamTransform::VecPassable(_) => "VecPassable",
        ParamTransform::ClassHandle(_) => "ClassHandle",
        ParamTransform::WireEncoded(_) => "WireEncoded",
        ParamTransform::Passable(_) => "Passable",
    }
}

pub fn transform_params(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> FfiParams {
    SyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error)
        .lower_inputs(inputs)
        .into_sync()
}

pub fn transform_params_async(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> syn::Result<AsyncFfiParams> {
    let async_param_lowerer =
        AsyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error);
    async_param_lowerer.validate_inputs(inputs)?;
    Ok(async_param_lowerer.lower_inputs(inputs).into_async())
}

pub fn transform_method_params(
    inputs: impl Iterator<Item = syn::FnArg>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> FfiParams {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params(
        &function_like_inputs,
        return_lowering,
        callback_registry,
        on_wire_record_error,
    )
}

pub fn transform_method_params_async(
    inputs: impl Iterator<Item = syn::FnArg>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> syn::Result<AsyncFfiParams> {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params_async(
        &function_like_inputs,
        return_lowering,
        callback_registry,
        on_wire_record_error,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncParamLowerer, transform_method_params, transform_params, transform_params_async,
    };
    use crate::index::callback_traits::CallbackTraitRegistry;
    use crate::index::class_types::ClassTypeRegistry;
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::{DataTypeCategory, DataTypeRegistry, ScalarEnumRepr};
    use crate::lowering::returns::model::ReturnLoweringContext;
    use quote::quote;
    use syn::parse_quote;

    fn async_param_lowerer() -> AsyncParamLowerer<'static> {
        let class_types = Box::leak(Box::new(ClassTypeRegistry::default()));
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::default()));
        let return_lowering = Box::leak(Box::new(ReturnLoweringContext::new(
            custom_types,
            data_types,
            class_types,
        )));
        let callback_registry = Box::leak(Box::new(CallbackTraitRegistry::default()));
        let on_wire_record_error = Box::leak(Box::new(proc_macro2::TokenStream::new()));
        AsyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error)
    }

    fn return_lowering(data_types: DataTypeRegistry) -> &'static ReturnLoweringContext<'static> {
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(data_types));
        let class_types = Box::leak(Box::new(ClassTypeRegistry::default()));
        Box::leak(Box::new(ReturnLoweringContext::new(
            custom_types,
            data_types,
            class_types,
        )))
    }

    fn callback_registry() -> &'static CallbackTraitRegistry {
        Box::leak(Box::new(CallbackTraitRegistry::default()))
    }

    #[test]
    fn scalar_enum_param_uses_passable_input_for_short_module_path() {
        let function: syn::ItemFn = parse_quote! {
            fn demo(provider: parser::RouteProvider) -> i32 {
                0
            }
        };
        let data_types = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "core::parser::RouteProvider",
                DataTypeCategory::Scalar(ScalarEnumRepr::I32),
            )],
            &[("parser", "crate::core::parser")],
        );
        let params = transform_params(
            &function.sig.inputs,
            return_lowering(data_types),
            callback_registry(),
            &quote! { return ::core::default::Default::default(); },
        );
        let rendered_ffi_params = &params.ffi_params;
        let rendered_conversions = &params.conversions;
        let ffi_params = quote! { #(#rendered_ffi_params),* }.to_string();
        let conversions = quote! { #(#rendered_conversions)* }.to_string();

        assert!(ffi_params.contains(
            "provider : < parser :: RouteProvider as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(!ffi_params.contains("__boltffi_provider_ptr"));
        assert!(!ffi_params.contains("__boltffi_provider_len"));
        assert!(conversions.contains(
            "< parser :: RouteProvider as :: boltffi :: __private :: Passable > :: unpack (provider)"
        ));
        assert!(!conversions.contains("wire :: decode"));
    }

    #[test]
    fn borrowed_scalar_enum_param_uses_passable_storage_for_short_module_path() {
        let function: syn::ItemFn = parse_quote! {
            fn demo(provider: &parser::RouteProvider) -> i32 {
                0
            }
        };
        let data_types = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "core::parser::RouteProvider",
                DataTypeCategory::Scalar(ScalarEnumRepr::I32),
            )],
            &[("parser", "crate::core::parser")],
        );
        let params = transform_params(
            &function.sig.inputs,
            return_lowering(data_types),
            callback_registry(),
            &quote! { return ::core::default::Default::default(); },
        );
        let rendered_ffi_params = &params.ffi_params;
        let rendered_conversions = &params.conversions;
        let rendered_call_args = &params.call_args;
        let ffi_params = quote! { #(#rendered_ffi_params),* }.to_string();
        let conversions = quote! { #(#rendered_conversions)* }.to_string();
        let call_args = quote! { #(#rendered_call_args),* }.to_string();

        assert!(ffi_params.contains(
            "provider : < parser :: RouteProvider as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(!ffi_params.contains("__boltffi_provider_ptr"));
        assert!(!ffi_params.contains("__boltffi_provider_len"));
        assert!(conversions.contains("let provider_storage : parser :: RouteProvider"));
        assert!(conversions.contains(
            "< parser :: RouteProvider as :: boltffi :: __private :: Passable > :: unpack (provider)"
        ));
        assert!(conversions.contains("let provider = & provider_storage"));
        assert!(!conversions.contains("wire :: decode"));
        assert_eq!(call_args, "provider");
    }

    #[test]
    fn async_borrowed_scalar_enum_param_uses_passable_storage_for_short_module_path() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(provider: &parser::RouteProvider) -> i32 {
                0
            }
        };
        let data_types = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "core::parser::RouteProvider",
                DataTypeCategory::Scalar(ScalarEnumRepr::I32),
            )],
            &[("parser", "crate::core::parser")],
        );
        let params = transform_params_async(
            &function.sig.inputs,
            return_lowering(data_types),
            callback_registry(),
            &quote! { return ::core::default::Default::default(); },
        )
        .expect("async params should lower");
        let rendered_ffi_params = &params.ffi_params;
        let rendered_pre_spawn = &params.pre_spawn;
        let rendered_thread_setup = &params.thread_setup;
        let ffi_params = quote! { #(#rendered_ffi_params),* }.to_string();
        let pre_spawn = quote! { #(#rendered_pre_spawn)* }.to_string();
        let thread_setup = quote! { #(#rendered_thread_setup)* }.to_string();

        assert!(ffi_params.contains(
            "provider : < parser :: RouteProvider as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(!ffi_params.contains("__boltffi_provider_ptr"));
        assert!(!ffi_params.contains("__boltffi_provider_len"));
        assert!(pre_spawn.contains("let provider_storage : parser :: RouteProvider"));
        assert!(pre_spawn.contains(
            "< parser :: RouteProvider as :: boltffi :: __private :: Passable > :: unpack (provider)"
        ));
        assert!(thread_setup.contains("let provider = & provider_storage"));
        assert!(!pre_spawn.contains("wire :: decode"));
        assert!(!thread_setup.contains("wire :: decode"));
    }

    #[test]
    fn scalar_enum_method_param_and_return_use_passable_transport() {
        let method: syn::ImplItemFn = parse_quote! {
            pub fn horizontal_or(
                &self,
                fallback: parser::RouteProvider
            ) -> parser::RouteProvider {
                fallback
            }
        };
        let data_types = DataTypeRegistry::with_entries_and_use_aliases(
            &[(
                "core::parser::RouteProvider",
                DataTypeCategory::Scalar(ScalarEnumRepr::I32),
            )],
            &[("parser", "crate::core::parser")],
        );
        let return_lowering = return_lowering(data_types);
        let params = transform_method_params(
            method.sig.inputs.iter().cloned(),
            return_lowering,
            callback_registry(),
            &quote! { return ::core::default::Default::default(); },
        );
        let return_abi = return_lowering.lower_output(&method.sig.output);
        let rendered_ffi_params = &params.ffi_params;
        let rendered_conversions = &params.conversions;
        let ffi_params = quote! { #(#rendered_ffi_params),* }.to_string();
        let conversions = quote! { #(#rendered_conversions)* }.to_string();

        assert!(return_abi.is_passable_value());
        assert!(ffi_params.contains(
            "fallback : < parser :: RouteProvider as :: boltffi :: __private :: Passable > :: In"
        ));
        assert!(!ffi_params.contains("__boltffi_fallback_ptr"));
        assert!(!ffi_params.contains("__boltffi_fallback_len"));
        assert!(conversions.contains(
            "< parser :: RouteProvider as :: boltffi :: __private :: Passable > :: unpack (fallback)"
        ));
        assert!(!conversions.contains("wire :: decode"));
    }

    #[test]
    fn rejects_async_callback_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(callback: impl Fn(i32) -> i32) {}
        };

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support closure callback parameters yet")
        );
    }

    #[test]
    fn rejects_async_mutable_slice_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(values: &mut [i32]) {}
        };

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support mutable slice parameters")
        );
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

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support trait object callback parameters")
        );
    }

    #[test]
    fn accepts_supported_async_params() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(name: String, label: &String, ids: Vec<i32>, scores: &[i32], id: i64) {}
        };

        assert!(
            async_param_lowerer()
                .validate_inputs(&function.sig.inputs)
                .is_ok()
        );
    }
}
