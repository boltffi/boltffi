use super::rendered_fixture;

#[test]
fn kotlin_target_passes_signed_primitive_vectors_as_jni_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_parameter"));
}

#[test]
fn kotlin_target_passes_mutable_primitive_slices_as_jni_arrays() {
    insta::assert_snapshot!(rendered_fixture(
        "direct_vector/mutable_primitive_slice_parameter"
    ));
}

#[test]
fn kotlin_target_preserves_unsigned_primitive_vector_arrays() {
    let rendered = rendered_fixture("direct_vector/unsigned_primitive_vector_parameter");

    assert!(rendered.contains("fun echoU16(values: UShortArray): UShortArray"));
    assert!(rendered.contains("fun echoU32(values: UIntArray): UIntArray"));
    assert!(rendered.contains("fun echoU64(values: ULongArray): ULongArray"));
    assert!(
        rendered.contains(
            "external fun boltffi_function_demo_echo_u16(values: ShortArray): ByteArray?"
        )
    );
    assert!(
        rendered
            .contains("external fun boltffi_function_demo_echo_u32(values: IntArray): ByteArray?")
    );
    assert!(
        rendered
            .contains("external fun boltffi_function_demo_echo_u64(values: LongArray): ByteArray?")
    );
    assert!(rendered.contains("Native.boltffi_function_demo_echo_u16(values.asShortArray())"));
    assert!(rendered.contains("Native.boltffi_function_demo_echo_u32(values.asIntArray())"));
    assert!(rendered.contains("Native.boltffi_function_demo_echo_u64(values.asLongArray())"));
    assert!(rendered.contains("DirectVectorCodec.readUShortArray"));
    assert!(rendered.contains("DirectVectorCodec.readUIntArray"));
    assert!(rendered.contains("DirectVectorCodec.readULongArray"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_passes_direct_record_vectors_as_packed_bytes() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_parameter"));
}

#[test]
fn kotlin_target_returns_primitive_vectors_from_native_buffers() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_return"));
}

#[test]
fn kotlin_target_returns_direct_record_vectors_from_native_buffers() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_return"));
}
