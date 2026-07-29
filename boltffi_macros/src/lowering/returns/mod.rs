pub(crate) mod classify;
pub(crate) mod lower;
pub(crate) mod model;

#[cfg(test)]
mod tests {
    use super::classify::classify_value_return_strategy;
    use super::model::{ResolvedReturn, WasmOptionScalarEncoding};
    use crate::index::class_types::ClassTypeRegistry;
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::{DataTypeCategory, DataTypeRegistry, ScalarEnumRepr};
    use crate::lowering::returns::model::ReturnLoweringContext;
    use boltffi_ffi_rules::transport::{
        EncodedReturnStrategy, ReturnContract, ReturnInvocationContext, ReturnPlatform,
        ValueReturnMethod, ValueReturnStrategy,
    };
    use syn::parse_quote;

    fn empty_return_lowering_context<'a>(
        class_types: &'a ClassTypeRegistry,
        custom_types: &'a CustomTypeRegistry,
        data_types: &'a DataTypeRegistry,
    ) -> ReturnLoweringContext<'a> {
        ReturnLoweringContext::new(custom_types, data_types, class_types)
    }

    #[test]
    fn wasm_option_bool_uses_numeric_bool_encoding() {
        let class_types = ClassTypeRegistry::default();
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);
        let value_ident = syn::Ident::new("value", proc_macro2::Span::call_site());
        let expression =
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<bool>), &context)
                .expect("expected bool option encoding")
                .some_expression(&value_ident)
                .to_string();

        assert_eq!(expression, "if value { 1.0 } else { 0.0 }");
    }

    #[test]
    fn dependency_class_return_uses_object_handle() {
        let class_types = ClassTypeRegistry::with_paths(&[&["session_api", "Session"]]);
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);
        let rust_type = parse_quote!(session_api::Session);

        assert_eq!(
            classify_value_return_strategy(&rust_type, &context),
            ValueReturnStrategy::ObjectHandle
        );
    }

    #[test]
    fn wasm_option_i64_is_not_nan_boxed() {
        let class_types = ClassTypeRegistry::default();
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);
        assert!(
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<i64>), &context)
                .is_none()
        );
        assert!(
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<u64>), &context)
                .is_none()
        );
    }

    #[test]
    fn option_c_style_enum_return_uses_scalar_encoding() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::with_entries(&[(
            "crate::enums::Direction",
            DataTypeCategory::Scalar(ScalarEnumRepr::I32),
        )]);
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);

        assert_eq!(
            classify_value_return_strategy(&parse_quote!(Option<Direction>), &context),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
        );
    }

    #[test]
    fn option_c_style_enum_packs_discriminant_into_the_f64_slot() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::with_entries(&[(
            "crate::enums::Direction",
            DataTypeCategory::Scalar(ScalarEnumRepr::I32),
        )]);
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);
        let value_ident = syn::Ident::new("value", proc_macro2::Span::call_site());

        let expression = WasmOptionScalarEncoding::from_option_rust_type(
            &parse_quote!(Option<Direction>),
            &context,
        )
        .expect("expected scalar enum option encoding")
        .some_expression(&value_ident)
        .to_string();

        assert!(
            expression.contains("Passable :: pack (value) as f64"),
            "unexpected expression: {expression}"
        );
    }

    #[test]
    fn option_enum_with_64_bit_repr_stays_wire_encoded() {
        // Discriminants wider than f64's exact integer range must not ride the
        // scalar slot, or large variants would come back rounded.
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::with_entries(&[(
            "crate::enums::Wide",
            DataTypeCategory::Scalar(ScalarEnumRepr::I64),
        )]);
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);

        assert_eq!(
            classify_value_return_strategy(&parse_quote!(Option<Wide>), &context),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
    }

    #[test]
    fn option_blittable_record_stays_wire_encoded() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::with_entries(&[(
            "crate::records::Point",
            DataTypeCategory::Blittable,
        )]);
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);

        assert_eq!(
            classify_value_return_strategy(&parse_quote!(Option<Point>), &context),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
    }

    #[test]
    fn option_i64_and_u64_return_use_wire_encoding_to_preserve_bigint_payloads() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);

        let i64_strategy = classify_value_return_strategy(&parse_quote!(Option<i64>), &context);
        let u64_strategy = classify_value_return_strategy(&parse_quote!(Option<u64>), &context);

        assert_eq!(
            i64_strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
        assert_eq!(
            u64_strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
    }

    #[test]
    fn option_i32_return_keeps_compact_scalar_encoding() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::default();
        let context = empty_return_lowering_context(&class_types, &custom_types, &data_types);

        let strategy = classify_value_return_strategy(&parse_quote!(Option<i32>), &context);

        assert_eq!(
            strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
        );
    }

    #[test]
    fn exported_class_return_uses_object_handle_strategy() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::with_entries(&["Marker"]);
        let context = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);

        let strategy = classify_value_return_strategy(&parse_quote!(Marker), &context);

        assert_eq!(strategy, ValueReturnStrategy::ObjectHandle);
    }

    #[test]
    fn qualified_exported_class_return_uses_object_handle_strategy() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::with_entries(&["map::Marker"]);
        let context = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);

        let strategy = classify_value_return_strategy(&parse_quote!(crate::map::Marker), &context);

        assert_eq!(strategy, ValueReturnStrategy::ObjectHandle);
    }

    #[test]
    fn self_return_uses_object_handle_strategy() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::with_entries(&["Map"]);
        let self_type = parse_quote!(Map);
        let base_context = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);
        let context = base_context.with_self_type(&self_type);

        let strategy = classify_value_return_strategy(&parse_quote!(Self), &context);

        assert_eq!(strategy, ValueReturnStrategy::ObjectHandle);
    }

    #[test]
    fn optional_exported_class_return_uses_object_handle_strategy() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::with_entries(&["Marker"]);
        let context = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);

        let strategy = classify_value_return_strategy(&parse_quote!(Option<Marker>), &context);

        assert_eq!(strategy, ValueReturnStrategy::ObjectHandle);
    }

    #[test]
    fn renamed_exported_class_return_uses_object_handle_strategy() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let class_types = ClassTypeRegistry::with_entries_and_use_aliases(
            &["map::Marker"],
            &[("Pin", "crate::map::Marker")],
        );
        let context = ReturnLoweringContext::new(&custom_types, &data_types, &class_types);

        let strategy = classify_value_return_strategy(&parse_quote!(Pin), &context);

        assert_eq!(strategy, ValueReturnStrategy::ObjectHandle);
    }

    #[test]
    fn packed_encoded_return_uses_packed_default_on_wasm_failure() {
        let resolved_return = ResolvedReturn::new(
            parse_quote!(std::time::Duration),
            ReturnContract::infallible(ValueReturnStrategy::Buffer(
                EncodedReturnStrategy::WireEncoded,
            )),
        );

        let statement = resolved_return
            .invalid_arg_early_return_statement()
            .to_string();

        assert!(matches!(
            resolved_return.value_return_strategy(),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        ));
        assert!(matches!(
            resolved_return
                .value_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm,),
            ValueReturnMethod::DirectReturn
        ));
        assert!(statement.contains("FfiBuf :: default () . into_packed ()"));
        assert!(statement.contains("return :: boltffi :: __private :: FfiBuf :: default ()"));
    }

    #[test]
    fn direct_vec_return_uses_platform_aware_early_return() {
        let resolved_return = ResolvedReturn::new(
            parse_quote!(Vec<i32>),
            ReturnContract::infallible(ValueReturnStrategy::Buffer(
                EncodedReturnStrategy::DirectVec,
            )),
        );

        assert!(matches!(
            resolved_return
                .value_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm,),
            ValueReturnMethod::WriteToReturnSlot
        ));

        let combined = resolved_return
            .invalid_arg_early_return_statement()
            .to_string();
        assert!(
            combined.contains("return ;"),
            "combined: wasm branch should use void return"
        );
        assert!(
            combined.contains("return :: boltffi :: __private :: FfiBuf :: default ()"),
            "combined: native branch should return FfiBuf::default()"
        );

        assert_eq!(
            resolved_return
                .wasm_invalid_arg_early_return_statement()
                .to_string(),
            "return ;",
        );
        assert_eq!(
            resolved_return
                .native_invalid_arg_early_return_statement()
                .to_string(),
            "return :: boltffi :: __private :: FfiBuf :: default () ;",
        );
    }
}
