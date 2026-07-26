use std::path::Path;

use boltffi_binding::DeclarationRef;

use super::{
    bindings, bridge, bridge_fixture, bridge_with_owner, rendered_fixture,
    rendered_fixture_with_class_support, rendered_fixture_with_support, rendered_source,
    source::SourceFixture,
};

#[test]
fn jni_bridge_layers_primitive_functions_on_c_bridge() {
    insta::assert_snapshot!(rendered_fixture("exports/primitive_functions"));
}

#[test]
fn jni_bridge_renders_shared_support_fragments() {
    insta::assert_snapshot!(rendered_fixture_with_support(
        "exports/closure_result_return"
    ));
}

#[test]
fn jni_bridge_contract_records_class_and_source_path() {
    let output = bridge_fixture("exports/single_function");
    let contract = output.contract();

    assert_eq!(contract.class().as_java_path(), "com.boltffi.demo.Native");
    assert_eq!(
        contract.source_path().as_path(),
        Path::new("jni/jni_glue.c")
    );
    assert_eq!(contract.c_header().as_str(), "demo.h");
    assert_eq!(contract.methods().len(), 1);
    assert_eq!(
        contract.methods()[0].symbol().to_string(),
        "Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add"
    );
}

#[test]
fn jni_bridge_uses_utf8_diagnostics_for_supplementary_class_names() {
    let source = SourceFixture::one("callback/foreign_callback_parameter").read();
    let output = bridge_with_owner(&source, "com.boltffi.𐐀runtime", "Native");
    let source = output
        .output()
        .files()
        .iter()
        .find(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|ext| ext == "c")
        })
        .expect("generated JNI source")
        .contents();

    assert!(source.contains(
        "boltffi_jni_lookup_global_class_with_diagnostic(env, \"com/boltffi/\\355\\240\\201\\355\\260\\200runtime/Native\", \"com/boltffi/\\360\\220\\220\\200runtime/Native\", &boltffi_jni_native_class)"
    ));
    assert!(source.contains(
        "boltffi_jni_lookup_static_method_with_diagnostic(env, g____ListenerVTable_class, \"com/boltffi/\\360\\220\\220\\200runtime/ListenerCallbacks\", \"on_value\", \"on_value\", \"(JI)I\", \"(JI)I\", &g____ListenerVTable_on_value_method)"
    ));
    assert!(source.contains(
        "could not resolve static method %s.%s%s\\n\", diagnostic_class_name, diagnostic_method_name, diagnostic_signature"
    ));
}

#[test]
fn jni_bridge_indexes_native_methods_by_source_symbol() {
    let source = SourceFixture::one("exports/single_function").read();
    let bindings = bindings(&source);
    let function = bindings
        .decls()
        .iter()
        .find_map(|decl| match DeclarationRef::from(decl) {
            DeclarationRef::Function(function) => Some(function),
            _ => None,
        })
        .expect("function fixture declaration");
    let output = bridge(&source);
    let method = output
        .contract()
        .source_method(function.symbol().id())
        .expect("JNI method for source symbol");

    assert_eq!(method.source_symbol(), function.symbol().id());
    assert_eq!(
        method.c_function().name(),
        function.symbol().name().as_str()
    );
}

#[test]
fn jni_bridge_renders_direct_records_and_c_style_enums() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_records_and_c_style_enums"));
}

#[test]
fn jni_bridge_renders_encoded_functions_as_byte_arrays() {
    insta::assert_snapshot!(rendered_source(SourceFixture::many([
        "records/person",
        "enums/shape",
        "enums/message",
        "exports/encoded_functions",
    ])));
}

#[test]
fn jni_bridge_renders_fallible_returns_as_encoded_error_checked_values() {
    insta::assert_snapshot!(rendered_fixture("exports/fallible_returns"));
}

#[test]
fn jni_bridge_renders_string_functions_as_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("exports/string_functions"));
}

#[test]
fn jni_bridge_renders_custom_type_functions_as_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_type_functions"));
}

#[test]
fn jni_bridge_renders_class_handles_and_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/class_handles_and_methods"));
}

#[test]
fn jni_bridge_preserves_rust_pascal_type_spelling() {
    insta::assert_snapshot!(rendered_fixture("exports/acronym_class"));
}

#[test]
fn jni_bridge_renders_async_class_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/async_class_methods"));
}

#[test]
fn jni_bridge_reports_custom_continuation_owner_class() {
    let rendered = rendered_fixture_with_class_support("exports/async_class_methods", "Bindings");

    assert!(rendered.contains(
        "boltffi_jni_lookup_static_method_with_diagnostic(env, boltffi_jni_native_class, \"com/boltffi/demo/Bindings\", \"boltffiFutureContinuationCallback\", \"boltffiFutureContinuationCallback\", \"(JB)V\", \"(JB)V\", &boltffi_jni_continuation_method)"
    ));
    assert!(!rendered.contains(
        "boltffi_jni_lookup_static_method_with_diagnostic(env, boltffi_jni_native_class, \"Native\", \"boltffiFutureContinuationCallback\""
    ));
}

#[test]
fn jni_bridge_casts_async_handles_and_callbacks_to_c_abi_types() {
    insta::assert_snapshot!(rendered_fixture("exports/async_handles_and_callbacks"));
}

#[test]
fn jni_bridge_renders_async_complete_return_shapes() {
    insta::assert_snapshot!(rendered_fixture("exports/async_complete_return_shapes"));
}

#[test]
fn jni_bridge_renders_closure_parameters_from_contract_group() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_parameter"));
}

#[test]
fn jni_bridge_preserves_multi_argument_closure_signature_names() {
    insta::assert_snapshot!(rendered_fixture("exports/multi_argument_closure_parameter"));
}

#[test]
fn jni_bridge_renders_encoded_closure_parameters_from_contract_group() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_closure_parameter"));
}

#[test]
fn jni_bridge_renders_encoded_closure_return_shapes_as_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_closure_return_shapes"));
}

#[test]
fn jni_bridge_renders_c_style_enum_closure_returns_as_scalars() {
    insta::assert_snapshot!(rendered_fixture("exports/c_style_enum_closure_return"));
}

#[test]
fn jni_bridge_renders_direct_vector_closure_parameters_from_contract_group() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_vector_closure_parameter"));
}

#[test]
fn jni_bridge_renders_closure_result_returns_from_contract_group() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_result_return"));
}

#[test]
fn jni_bridge_renders_nested_closure_parameters_from_contract_group() {
    insta::assert_snapshot!(rendered_fixture("exports/nested_closure_parameter"));
}

#[test]
fn jni_bridge_renders_nested_closure_parameters_for_callback_owned_closures() {
    insta::assert_snapshot!(rendered_fixture(
        "exports/nested_callback_owned_closure_parameter"
    ));
}

#[test]
fn jni_bridge_renders_closure_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_callback_handle_return"));
}

#[test]
fn jni_bridge_renders_closure_direct_record_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_direct_record_return"));
}

#[test]
fn jni_bridge_renders_closure_class_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("exports/closure_class_handle_return"));
}
