use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
mod code_indenter;
pub mod cpp;
pub mod csharp;
pub mod rust;
pub mod typescript;
pub mod unrealcpp;
mod util;

pub use self::csharp::Csharp;
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
        // Public tables from mounted submodules
        module
            .all_tables_with_prefix()
            .into_iter()
            .filter(|(prefix, _, table)| !prefix.is_empty() && table.table_access == TableAccess::Public)
            .map(|(prefix, owning_def, table)| lang.generate_mounted_table_file(owning_def, &prefix, table)),
        // Views from mounted submodules (views are currently always public)
        module
            .all_views_with_prefix()
            .into_iter()
            .filter(|(prefix, _, _)| !prefix.is_empty())
            .map(|(prefix, owning_def, view)| lang.generate_mounted_view_file(owning_def, &prefix, view)),
        module.types().flat_map(|typ| lang.generate_type_files(module, typ)),
        util::iter_reducers(module, options.visibility).map(|reducer| lang.generate_reducer_file(module, reducer)),
        util::iter_procedures(module, options.visibility)
            .map(|procedure| lang.generate_procedure_file(module, procedure)),
        // Reducers from mounted submodules
        module
            .all_reducers_with_prefix()
            .into_iter()
            .filter(|(prefix, _, reducer)| !prefix.is_empty() && !reducer.visibility.is_private())
            .map(|(prefix, owning_def, reducer)| lang.generate_mounted_reducer_file(owning_def, &prefix, reducer)),
        // Procedures from mounted submodules
        module
            .all_procedures_with_prefix()
            .into_iter()
            .filter(|(prefix, _, procedure)| !prefix.is_empty() && !procedure.visibility.is_private())
            .map(|(prefix, owning_def, procedure)| lang.generate_mounted_procedure_file(owning_def, &prefix, procedure)),
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

    /// Generate a row-type file for a public table from a mounted submodule.
    /// Uses `owning_def`'s typespace for type resolution.
    /// Filename goes in a subdirectory named after the namespace:
    /// e.g. `alias/table_name_table.ts` for namespace `"alias."`, table `tableName`.
    fn generate_mounted_table_file(&self, owning_def: &ModuleDef, namespace: &str, table: &TableDef) -> OutputFile {
        let schema = TableSchema::from_module_def(owning_def, table, (), 0.into())
            .validated()
            .expect("Failed to generate mounted table file");
        let mut file = self.generate_table_file_from_schema(owning_def, table, schema);
        let ns_path = namespace.trim_end_matches('.').replace('.', "/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate a row-type file for a view from a mounted submodule.
    /// Filename goes in a subdirectory named after the namespace prefix.
    fn generate_mounted_view_file(&self, owning_def: &ModuleDef, namespace: &str, view: &ViewDef) -> OutputFile {
        let tbl = TableDef::from(view.clone());
        let schema = TableSchema::from_view_def_for_codegen(owning_def, view)
            .validated()
            .expect("Failed to generate mounted view file");
        let mut file = self.generate_table_file_from_schema(owning_def, &tbl, schema);
        let ns_path = namespace.trim_end_matches('.').replace('.', "/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate an arg-schema file for a reducer from a mounted submodule.
    /// Filename goes in a subdirectory named after the namespace prefix.
    fn generate_mounted_reducer_file(&self, owning_def: &ModuleDef, prefix: &str, reducer: &ReducerDef) -> OutputFile {
        let mut file = self.generate_reducer_file(owning_def, reducer);
        let ns_path = prefix.trim_end_matches('.').replace('.', "/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate an arg-schema file for a procedure from a mounted submodule.
    /// Filename goes in a subdirectory named after the namespace prefix.
    fn generate_mounted_procedure_file(&self, owning_def: &ModuleDef, prefix: &str, procedure: &ProcedureDef) -> OutputFile {
        let mut file = self.generate_procedure_file(owning_def, procedure);
        let ns_path = prefix.trim_end_matches('.').replace('.', "/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }
}
