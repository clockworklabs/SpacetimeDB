use spacetimedb_schema::def::{ModuleDef, ReducerDef, TableDef, TypeDef};

mod code_indenter;
pub mod csharp;
pub mod rust;
pub mod typescript;
pub mod unrealcpp;
mod util;

pub use self::csharp::Csharp;
pub use self::rust::Rust;
pub use self::typescript::TypeScript;
pub use self::unrealcpp::UnrealCpp;
pub use util::AUTO_GENERATED_PREFIX;

pub fn generate(module: &ModuleDef, lang: &dyn Lang) -> Vec<OutputFile> {
    itertools::chain!(
        module.tables().map(|tbl| lang.generate_table_file(module, tbl)),
        module.types().flat_map(|typ| lang.generate_type_files(module, typ)),
        util::iter_reducers(module).map(|reducer| lang.generate_reducer_file(module, reducer)),
        std::iter::once(lang.generate_globals_file(module)),
    )
    .collect()
}

pub struct OutputFile {
    pub filename: String,
    pub code: String,
}

pub trait Lang {
    fn generate_table_file(&self, module: &ModuleDef, tbl: &TableDef) -> OutputFile;
    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile>;
    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile;
    fn generate_globals_file(&self, module: &ModuleDef) -> OutputFile;
}
