fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_CFG_MADSIM");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_SIMULATION");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");

    if std::env::var_os("CARGO_CFG_MADSIM").is_some() {
        println!("cargo:rustc-cfg=simulation");
    }
}
