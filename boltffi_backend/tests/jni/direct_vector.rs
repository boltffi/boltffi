use super::rendered_fixture;

#[test]
fn jni_bridge_maps_primitive_direct_vectors_to_java_primitive_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_parameter"));
}

#[test]
fn jni_bridge_writes_mutable_primitive_slices_back_to_java_arrays() {
    insta::assert_snapshot!(rendered_fixture(
        "direct_vector/mutable_primitive_slice_parameter"
    ));
}

#[test]
fn jni_bridge_maps_direct_record_vectors_to_java_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_parameter"));
}

#[test]
fn jni_bridge_maps_callback_direct_vectors_to_java_primitive_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/callback_vector_parameter"));
}
