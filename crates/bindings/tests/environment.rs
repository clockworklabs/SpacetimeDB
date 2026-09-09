#[test]
fn environment_declaration_accessors_compile_with_exact_types() {
    trybuild::TestCases::new().pass("tests/pass/environment.rs");
}
