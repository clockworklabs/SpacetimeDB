#[allow(clippy::disallowed_macros)]
fn main() {
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET").unwrap_or_default());
}
