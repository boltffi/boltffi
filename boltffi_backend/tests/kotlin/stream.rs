use super::{rendered_fixture, rendered_fixture_with_runtime};

#[test]
fn kotlin_target_renders_stream_protocols() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}

#[test]
fn kotlin_target_renders_stream_runtime() {
    insta::assert_snapshot!(rendered_fixture_with_runtime("stream/protocol_functions"));
}
