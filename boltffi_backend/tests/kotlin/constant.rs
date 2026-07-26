use super::rendered_fixture;

#[test]
fn kotlin_target_renders_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/literals_and_accessors"));
}

#[test]
fn kotlin_target_uses_data_enum_variant_names_for_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/data_enum"));
}

#[test]
fn kotlin_target_renders_associated_constants_in_the_companion_object() {
    let source = rendered_fixture("constant/associated");

    assert!(source.contains("val BLACK: Color"));
    assert!(source.contains("val CHANNEL_COUNT: UByte\n"));
    assert!(source.contains("get() = 4.toUByte()"));
    assert!(source.contains("val DEFAULT: Mode\n"));
    assert!(source.contains("get() = Mode.FAST"));
    assert!(source.contains("val FALLBACK: Mode\n"));
    assert!(source.contains("val VARIANT_COUNT: UByte\n"));
    assert!(source.contains("val INITIAL: State\n"));
    assert!(source.contains("get() = State.Idle"));
    assert!(source.contains("val MAX_COLORS: UByte\n"));
    assert!(source.contains("get() = 16.toUByte()"));
    assert!(source.contains("external fun boltffi_const_"));
    assert!(source.contains("color_black"));
    assert!(!source.contains("\nval BLACK: Color"));
    assert!(!source.contains("UNEXPORTED_ASSOCIATED"));
}

#[test]
fn kotlin_target_omits_redundant_enum_aliases_after_name_normalization() {
    let source = rendered_fixture("constant/enum_alias_collision");

    assert!(source.contains("DEFAULT("));
    assert!(!source.contains("val DEFAULT: Mode"));
}
