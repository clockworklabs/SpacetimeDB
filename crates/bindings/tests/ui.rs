#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    t.pass("tests/pass/*.rs");
}

#[test]
fn local_outbox_typecheck() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/local_outbox_ui/*.rs");
}
