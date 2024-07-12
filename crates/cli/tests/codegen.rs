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

macro_rules! declare_tests {
    ($($name:ident => $lang:ident,)*) => {
        $(
            #[test]
            fn $name() {
                let module = generate::extract_descriptions(compiled_module()).unwrap();
                let outfiles: HashMap<_, _> = generate::generate(module, generate::Language::$lang, "SpacetimeDB")
                    .unwrap()
                    .into_iter()
                    .collect();
                insta::with_settings!({ sort_maps => true }, {
                    insta::assert_toml_snapshot!(outfiles);
                });
            }
        )*
    }
}

declare_tests! {
    test_codegen_csharp => Csharp,
    test_codegen_typescript => TypeScript,
    test_codegen_rust => Rust,
}
