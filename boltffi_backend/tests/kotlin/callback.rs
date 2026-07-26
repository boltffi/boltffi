use boltffi_backend::{Error, target::kotlin::KotlinHost};

use super::rendered_fixture;

#[test]
fn kotlin_target_renders_callback_handle_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn kotlin_target_renders_callback_enum_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_enum_parameter"));
}

#[test]
fn kotlin_target_renders_callback_encoded_payloads() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_return"));
}

#[test]
fn kotlin_target_renders_callback_encoded_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_byte_slice_parameter"));
}

#[test]
fn kotlin_target_renders_callback_record_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_parameter"));
}

#[test]
fn kotlin_target_renders_callback_vectors() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_direct_vector_parameter"
    ));
}

#[test]
fn kotlin_target_renders_callback_optional_scalar_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_optional_scalar_return"));
}

#[test]
fn kotlin_target_renders_callback_result_returns() {
    let rendered = rendered_fixture("callback/callback_status_result");

    assert!(rendered.contains("catch (__boltffi_mapStatus_error: Throwable)"));
    assert!(!rendered.contains("catch (__boltffi_mapStatus_error: String)"));
    assert!(rendered.contains("val __boltffi_mapStatus_result = impl.mapStatus(status)"));
    assert_eq!(rendered.matches("impl.mapStatus(status)").count(), 1);

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_callback_encoded_result_returns() {
    let rendered = rendered_fixture("callback/callback_encoded_status_result");

    assert!(rendered.contains("catch (__boltffi_mapMessage_error: Throwable)"));
    assert!(!rendered.contains("catch (__boltffi_mapMessage_error: String)"));
    assert!(rendered.contains("val __boltffi_mapMessage_result = impl.mapMessage(key)"));
    assert_eq!(rendered.matches("impl.mapMessage(key)").count(), 1);

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_callback_handle_returns() {
    let rendered = rendered_fixture("callback/callback_handle_return");

    assert!(rendered.contains("if (__boltffi_closed.compareAndSet(false, true)) {"));
    assert!(rendered.contains("check(!__boltffi_closed.get()) { \"callback handle is closed\" }"));
    assert!(!rendered.contains("private var closed"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_rejects_callback_methods_shadowing_handle_members() {
    let error = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target")
        .render(&super::bindings(
            r#"
            #[export]
            pub trait Listener {
                fn raw_handle(&self) -> u32;
            }

            #[export]
            pub fn make_listener() -> Box<dyn Listener> {
                loop {}
            }
            "#,
        ))
        .expect_err("a callback method shadowing the generated handle members must not render");
    assert!(
        matches!(
            &error,
            Error::KotlinNameCollision { scope, name }
                if scope == "ListenerHandle" && name == "rawHandle()"
        ),
        "{error:?}"
    );
}

#[test]
fn kotlin_target_renders_nullable_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/nullable_callback_handle_return"));
}

#[test]
fn kotlin_target_renders_async_callback_return_shapes() {
    let rendered = rendered_fixture("callback/async_callback_return_shapes");

    assert!(rendered.contains("\ninterface Listener {"));
    assert!(!rendered.contains("fun interface"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_async_callback_unit_results() {
    let rendered = rendered_fixture("callback/async_callback_unit_result");

    assert!(
        rendered
            .contains("impl.remove(key.toUInt()); Native.boltffi_async_callback_complete_Bytes(")
    );

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_single_method_callbacks_as_fun_interfaces() {
    let rendered = rendered_fixture("callback/async_callback_string_return");

    assert!(rendered.contains("fun interface Listener {"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_async_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_returning_callback_handle"
    ));
}
