#[test]
fn environment_declaration_accessors_compile_with_exact_types() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/pass/environment.rs");
    tests.compile_fail("tests/ui/environment_types.rs");
    tests.compile_fail("tests/ui/environment_enum.rs");
}
