#[test]
fn compile_errors_match_the_recorded_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/diagnostics/*/*.rs");
}
