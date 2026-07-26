use super::rendered_fixture;

#[test]
fn kotlin_target_renders_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/literals_and_accessors"));
}

#[test]
fn kotlin_target_uses_data_enum_variant_names_for_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/data_enum"));
}
