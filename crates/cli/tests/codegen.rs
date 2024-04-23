use spacetimedb_cli::generate;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};
use std::path::Path;
use std::sync::OnceLock;

fn compiled_module() -> &'static Path {
    static COMPILED_MODULE: OnceLock<CompiledModule> = OnceLock::new();
    COMPILED_MODULE
        .get_or_init(|| CompiledModule::compile("rust-wasm-test", CompilationMode::Debug))
        .path()
}

#[test]
fn test_codegen_output() {
    let outfiles: HashMap<_, _> = generate::generate(compiled_module(), generate::Language::Csharp, "SpacetimeDB")
        .unwrap()
        .into_iter()
        .collect();
    insta::with_settings!({ sort_maps => true }, {
        insta::assert_toml_snapshot!(outfiles);
    });
}

#[test]
fn test_typescript_codegen_output() {
    let outfiles: HashMap<_, _> = generate::generate(compiled_module(), generate::Language::TypeScript, "SpacetimeDB")
        .unwrap()
        .into_iter()
        .collect();
    insta::with_settings!({ sort_maps => true }, {
        insta::assert_toml_snapshot!(outfiles);
    });
}
