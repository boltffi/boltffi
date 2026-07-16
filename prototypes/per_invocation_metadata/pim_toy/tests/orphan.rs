#[test]
fn a_foreign_type_needs_a_local_tag() {
    trybuild::TestCases::new().compile_fail("tests/ui/remote_without_local_tag.rs");
}

#[test]
fn an_unannotated_field_type_is_a_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/unannotated_field.rs");
}

/// A fixed container vocabulary cannot see through an alias; only blanket impls on the containers
/// themselves would let the compiler resolve this.
#[test]
fn an_alias_to_a_container_is_a_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/alias_to_container.rs");
}

#[test]
fn an_unannotated_export_parameter_is_a_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/export_unannotated_param.rs");
}
