use std::collections::HashMap;
use std::path::Path;

#[test]
fn test_codegen_output() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/wasm32-unknown-unknown/release/rust_wasm_test.wasm"
    ));
    if !path.exists() {
        eprintln!("rust_wasm_test isn't built, skipping");
    }
    use spacetimedb_cli::codegen;
    let outfiles: HashMap<_, _> = codegen::gen_bindings(path, codegen::Language::Csharp)
        .unwrap()
        .collect();
    insta::with_settings!({ sort_maps => true }, {
        insta::assert_toml_snapshot!(outfiles);
    });
}
