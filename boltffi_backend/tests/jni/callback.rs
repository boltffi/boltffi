use boltffi_binding::DeclarationRef;

use super::{
    bindings, bridge, rendered_fixture, rendered_fixture_with_support, source::SourceFixture,
};

#[test]
fn jni_bridge_indexes_callback_registrations_by_source_id() {
    let source = SourceFixture::one("callback/foreign_callback_parameter").read();
    let bindings = bindings(&source);
    let callback = bindings
        .decls()
        .iter()
        .find_map(|decl| match DeclarationRef::from(decl) {
            DeclarationRef::Callback(callback) => Some(callback),
            _ => None,
        })
        .expect("callback fixture declaration");
    let output = bridge(&source);
    let contract = output.contract();

    assert_eq!(
        contract
            .source_callback(callback.id())
            .map(|registration| registration.id()),
        Some(callback.id())
    );
}

#[test]
fn jni_bridge_renders_callback_handle_parameters() {
    let rendered = rendered_fixture("callback/foreign_callback_parameter");

    assert!(rendered.contains("static BoltFFICallbackHandle boltffi_jni_callback_parameter"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn jni_bridge_renders_constructor_callback_parameters() {
    let rendered = rendered_fixture("callback/constructor_callback_parameter");

    assert!(rendered.contains("boltffi_jni_callback_parameter((uint64_t)notifier"));
    assert!(rendered.contains("static BoltFFICallbackHandle boltffi_jni_callback_parameter"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn jni_bridge_reports_lifecycle_lookup_failures() {
    let rendered = rendered_fixture_with_support("callback/foreign_callback_parameter");

    assert!(rendered.contains(
        "boltffi_jni_lookup_global_class_with_diagnostic(env, \"com/boltffi/demo/Native\", \"com/boltffi/demo/Native\", &boltffi_jni_native_class)"
    ));
    assert!(rendered.contains(
        "boltffi_jni_lookup_global_class_with_diagnostic(env, \"com/boltffi/demo/ListenerCallbacks\", \"com/boltffi/demo/ListenerCallbacks\", &g____ListenerVTable_class)"
    ));
    assert!(rendered.contains(
        "boltffi_jni_lookup_static_method_with_diagnostic(env, g____ListenerVTable_class, \"com/boltffi/demo/ListenerCallbacks\", \"on_value\", \"on_value\", \"(JI)I\", \"(JI)I\", &g____ListenerVTable_on_value_method)"
    ));
}

#[test]
fn jni_bridge_caches_android_callback_threads_with_local_frames() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn jni_bridge_renders_callback_byte_slice_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_byte_slice_parameter"));
}

#[test]
fn jni_bridge_renders_callback_handle_method_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_method_callback_handle_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_record_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_parameter"));
}

#[test]
fn jni_bridge_renders_callback_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_closure_parameter"));
}

#[test]
fn jni_bridge_renders_callback_encoded_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_encoded_closure_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_direct_vector_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_direct_vector_closure_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_closure_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_closure_return"));
}

#[test]
fn jni_bridge_renders_callback_handle_method_closure_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/returned_callback_closure_return"
    ));
}

#[test]
fn jni_bridge_renders_callback_encoded_closure_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_closure_return"));
}

#[test]
fn jni_bridge_renders_callback_encoded_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_return"));
}

#[test]
fn jni_bridge_keeps_callback_status_param_separate_from_generated_status() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_status_result"));
}

#[test]
fn jni_bridge_renders_callback_record_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_return"));
}

#[test]
fn jni_bridge_renders_async_callback_completions() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_string_return"));
}

#[test]
fn jni_bridge_renders_async_callback_completion_shapes() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_return_shapes"));
}

#[test]
fn jni_bridge_renders_c_style_enum_async_callback_completion_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_c_style_enum_result"
    ));
}

#[test]
fn jni_bridge_renders_async_callback_handle_completion_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_returning_callback_handle"
    ));
}

#[test]
fn jni_bridge_renders_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_handle_return"));
}

#[test]
fn jni_bridge_renders_async_callback_handle_methods() {
    insta::assert_snapshot!(rendered_fixture("callback/returned_async_callback_handle"));
}

#[test]
fn jni_bridge_renders_async_callback_handle_method_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/returned_async_callback_return_shapes"
    ));
}

#[test]
fn jni_bridge_renders_callback_method_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_method_callback_handle_return"
    ));
}

#[test]
fn jni_bridge_renders_nullable_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/nullable_callback_handle_return"));
}
