use super::{rendered_fixture, run_kotlin_assertions};

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
fn kotlin_target_compares_array_fields_by_content() {
    let rendered = rendered_fixture("records/array_fields");

    assert!(rendered.contains("        if (other !is Blob) return false"));
    assert!(rendered.contains("this.payload.contentEquals(other.payload)"));
    assert!(rendered.contains("this.checksum.contentEquals(other.checksum)"));
    assert!(rendered.contains("this.ratio.equals(other.ratio)"));
    assert!(rendered.contains("result = 31 * result + this.samples.contentHashCode()"));
    assert!(rendered.contains("        if (other !is BlobError) return false"));
    assert!(rendered.contains("            if (other !is Data) return false"));
    assert!(
        rendered.contains(
            "        override fun hashCode(): kotlin.Int = this.payload.contentHashCode()"
        )
    );
    assert!(rendered.contains(
        "this.pages[index].indices.all { index1 -> this.pages[index][index1].contentEquals(other.pages[index][index1]) }"
    ));
    assert!(rendered.contains(
        "(this.maybeChunks?.let { left -> other.maybeChunks?.let { right -> left.size == right.size"
    ));
    assert!(rendered.contains("?: false } ?: (other.maybeChunks == null))"));
    assert!(rendered.contains(
        "this.named.entries.fold(0) { hash, entry -> hash + entry.key.hashCode().xor(entry.value.contentHashCode()) }"
    ));
    assert!(!rendered.contains("if (other !is Label) return false"));
    assert!(!rendered.contains("if (other !is Named) return false"));

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_array_fields_compare_by_content_at_runtime() {
    run_kotlin_assertions(
        "records/array_fields",
        include_str!("../fixtures/kotlin/array_fields.kt"),
    );
}
