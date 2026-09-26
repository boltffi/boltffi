use super::{rendered_fixture, rendered_fixture_with_runtime};

#[test]
fn kotlin_target_renders_stream_protocols() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}

#[test]
fn kotlin_target_renders_stream_runtime() {
    insta::assert_snapshot!(rendered_fixture_with_runtime("stream/protocol_functions"));
}

#[test]
fn kotlin_target_renders_fallible_streams() {
    insta::assert_snapshot!(rendered_fixture("stream/fallible"));
}

#[test]
fn kotlin_target_hands_a_fallible_callback_stream_error_to_on_error() {
    let files = super::files(
        r#"
        use boltffi::EventSubscription;
        use std::sync::Arc;

        pub struct Jobs;

        #[export(single_threaded)]
        impl Jobs {
            #[ffi_stream(item = i32, error = String, mode = "callback")]
            pub fn ticks(&self) -> Arc<EventSubscription<i32, String>> {
                loop {}
            }
        }
        "#,
    );
    let (_, source) = files
        .iter()
        .find(|(path, _)| path.ends_with(".kt"))
        .expect("a Kotlin source");

    assert!(source.contains(
        "fun Jobs.ticks(onError: (Throwable) -> Unit, callback: (Int) -> Unit): TicksCancellable"
    ));
    assert!(source.contains("finish = { failure -> failure?.let(onError) },"));
    assert!(source.contains("Native.boltffi_stream_demo_jobs_ticks_take_error(handle)"));
}
