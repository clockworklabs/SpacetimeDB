use hashbrown::HashMap;
use std::path::Path;

#[test]
fn test_codegen_output() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/wasm32-unknown-unknown/release/rust_wasm_test.wasm"
    ));
    if !path.exists() {
        eprintln!("rust_wasm_test isn't built, skipping");
        return;
    }
    use spacetimedb_cli::generate;
    println!("{}", path.to_str().unwrap());
    let outfiles: HashMap<_, _> = generate::generate(path, generate::Language::Csharp, "SpacetimeDB")
        .unwrap()
        .into_iter()
        .collect();
    insta::with_settings!({ sort_maps => true }, {
        insta::assert_toml_snapshot!(outfiles);
    });
}

#[test]
fn test_typescript_codegen_output() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/wasm32-unknown-unknown/release/rust_wasm_test.wasm"
    ));
    if !path.exists() {
        eprintln!("rust_wasm_test isn't built, skipping");
        return;
    }
    use spacetimedb_cli::generate;
    println!("{}", path.to_str().unwrap());
    let outfiles: HashMap<_, _> = generate::generate(path, generate::Language::TypeScript, "SpacetimeDB")
        .unwrap()
        .into_iter()
        .collect();
    insta::with_settings!({ sort_maps => true }, {
        insta::assert_toml_snapshot!(outfiles);
    });
}
