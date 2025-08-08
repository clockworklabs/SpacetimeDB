#![allow(clippy::disallowed_macros)]
fn main() {
    let target = std::env::var("TARGET").unwrap();
    println!("cargo::rustc-env=BUILD_TARGET={target}");
}
