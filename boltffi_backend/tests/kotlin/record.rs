use super::rendered_fixture;

#[test]
fn kotlin_target_steps_over_direct_record_padding_in_wire_codecs() {
    let rendered = rendered_fixture("records/padded_direct_record");

    assert!(rendered.contains("internal const val STRUCT_SIZE: Int = 12"));
    assert!(rendered.contains("internal fun wireSize(): Int {\n        return 12"));
    assert!(rendered.contains("reader.skip(2).readI32()"));
    assert!(rendered.contains("writer.pad(2).writeI32(value)"));
    assert!(rendered.contains(".also { reader.skip(3) }"));
    assert!(rendered.contains("writer.writeU8(flag)\n        writer.pad(3)"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_instance_methods_on_empty_records() {
    let rendered = rendered_fixture("records/empty_record_methods");

    assert!(rendered.contains("object Marker {"));
    assert!(rendered.contains("    fun describe(): String {"));
    assert!(rendered.contains("    fun intoCode(): UInt {"));
    assert!(rendered.contains("    fun touch(): Marker {"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_async_instance_methods_on_records() {
    let rendered = rendered_fixture("records/async_record_methods");

    assert_eq!(rendered.matches("return boltffiCallAsync(").count(), 3);

    insta::assert_snapshot!(rendered);
}
