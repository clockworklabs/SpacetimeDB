use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
mod code_indenter;
pub mod cpp;
pub mod csharp;
pub mod go;
pub mod rust;
pub mod typescript;
pub mod unrealcpp;
mod util;

pub use self::csharp::Csharp;
pub use self::go::Go;
pub use self::rust::Rust;
pub use self::typescript::TypeScript;
pub use self::unrealcpp::UnrealCpp;
pub use util::private_table_names;
pub use util::CodegenVisibility;
pub use util::AUTO_GENERATED_PREFIX;

#[derive(Clone, Copy, Debug)]
pub struct CodegenOptions {
    pub visibility: CodegenVisibility,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            visibility: CodegenVisibility::OnlyPublic,
        }
    }
}

pub fn generate(module: &ModuleDef, lang: &dyn Lang, options: &CodegenOptions) -> Vec<OutputFile> {
    itertools::chain!(
        util::iter_tables(module, options.visibility).map(|tbl| lang.generate_table_file(module, tbl)),
        module.views().map(|view| lang.generate_view_file(module, view)),
        module.types().flat_map(|typ| lang.generate_type_files(module, typ)),
        util::iter_reducers(module, options.visibility).map(|reducer| lang.generate_reducer_file(module, reducer)),
        util::iter_procedures(module, options.visibility)
            .map(|procedure| lang.generate_procedure_file(module, procedure)),
        lang.generate_global_files(module, options),
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
    fn generate_procedure_file(&self, module: &ModuleDef, procedure: &ProcedureDef) -> OutputFile;
    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile>;

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
