#[test]
fn compile_errors_match_the_recorded_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/diagnostics/*/*.rs");
    cases.pass("tests/accepted/*.rs");
}
