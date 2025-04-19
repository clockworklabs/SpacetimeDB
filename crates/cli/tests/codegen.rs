use spacetimedb_cli::generate::{csharp::Csharp, generate, rust::Rust, typescript::TypeScript};
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::RawModuleDef;
use spacetimedb_testing::modules::{start_runtime, CompilationMode, CompiledModule};
use spacetimedb_testing::spacetimedb::db::datastore::traits::Program;
use spacetimedb_testing::spacetimedb::host::extract_schema;
use spacetimedb_testing::spacetimedb::messages::control_db::HostType;
use std::sync::OnceLock;

fn compiled_module() -> &'static [u8] {
    static COMPILED_MODULE: OnceLock<CompiledModule> = OnceLock::new();
    COMPILED_MODULE
        .get_or_init(|| CompiledModule::compile("module-test", CompilationMode::Debug))
        .program_bytes()
}

macro_rules! declare_tests {
    ($($name:ident => $lang:expr,)*) => ($(
        #[test]
        fn $name() {
            let program = Program::from_bytes(compiled_module());
            let module = start_runtime()
                .block_on(extract_schema(program, HostType::Wasm))
                .unwrap();
            let module = RawModuleDef::V9(module.into());
            let outfiles: HashMap<_, _> = generate(module, &$lang)
                .unwrap()
                .into_iter()
                .collect();
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
