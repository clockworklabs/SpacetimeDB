use spacetimedb_schema::def::{ModuleDef, ReducerDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::schema::{Schema, TableSchema};

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
        module.views().map(|view| lang.generate_view_file(module, view)),
        module.types().flat_map(|typ| lang.generate_type_files(module, typ)),
        util::iter_reducers(module).map(|reducer| lang.generate_reducer_file(module, reducer)),
        lang.generate_global_files(module),
    )
    .collect()
}

pub struct OutputFile {
    pub filename: String,
    pub code: String,
}

pub trait Lang {
    fn generate_table_file_from_schema(&self, module: &ModuleDef, tbl: &TableDef, schema: TableSchema) -> OutputFile;
    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile>;
    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile;
    fn generate_global_files(&self, module: &ModuleDef) -> Vec<OutputFile>;

    fn generate_table_file(&self, module: &ModuleDef, tbl: &TableDef) -> OutputFile {
        let schema = TableSchema::from_module_def(module, tbl, (), 0.into())
            .validated()
            .expect("Failed to generate table due to validation errors");
        self.generate_table_file_from_schema(module, tbl, schema)
    }

    fn generate_view_file(&self, module: &ModuleDef, view: &ViewDef) -> OutputFile {
        let tbl = TableDef::from(view.clone());
        let schema = TableSchema::from_view_def_for_codegen(module, view)
            .validated()
            .expect("Failed to generate table due to validation errors");
        self.generate_table_file_from_schema(module, &tbl, schema)
    }
}
