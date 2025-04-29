use spacetimedb_codegen::{extract_descriptions, generate, Csharp, Rust, TypeScript};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule};
use std::path::Path;
use std::sync::OnceLock;

fn compiled_module() -> &'static Path {
    static COMPILED_MODULE: OnceLock<CompiledModule> = OnceLock::new();
    COMPILED_MODULE
        .get_or_init(|| CompiledModule::compile("module-test", CompilationMode::Debug))
        .path()
}

macro_rules! declare_tests {
    ($($name:ident => $lang:expr,)*) => ($(
        #[test]
        fn $name() {
            let module = extract_descriptions(compiled_module()).unwrap().try_into().unwrap();
            let outfiles = HashMap::<_, _>::from_iter(generate(&module, &$lang));
            insta::with_settings!({ sort_maps => true }, {
                insta::assert_toml_snapshot!(outfiles);
            });
        }
    )*);
}

declare_tests! {
    test_codegen_csharp => Csharp { namespace: "SpacetimeDB" },
    test_codegen_typescript => TypeScript,
    test_codegen_rust => Rust,
}
