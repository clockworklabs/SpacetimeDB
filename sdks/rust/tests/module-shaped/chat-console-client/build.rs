#![allow(clippy::disallowed_macros)]
//! Fail the build if `src/main.rs` differs from the quickstart template's client, so the spike
//! demonstrably compiles the template's code unchanged.
fn main() {
    let ours = "src/main.rs";
    let template = "../../../../../templates/chat-console-rs/src/main.rs";
    println!("cargo:rerun-if-changed={ours}");
    println!("cargo:rerun-if-changed={template}");
    let (a, b) = (std::fs::read(ours).unwrap(), std::fs::read(template).unwrap());
    assert!(a == b, "{ours} must be identical to {template}");
}
