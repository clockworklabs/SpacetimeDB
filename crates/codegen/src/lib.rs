use spacetimedb_lib::db::raw_def::v9::TableAccess;
use spacetimedb_lib::Hash;
use spacetimedb_schema::auto_migrate::ponder_manual_migrate;
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
        // Public tables from submodules
        module
            .all_tables_with_prefix()
            .into_iter()
            .filter(|(prefix, _, table)| !prefix.is_empty() && table.table_access == TableAccess::Public)
            .map(|(_, owning_def, table)| lang.generate_submodule_table_file(owning_def, table)),
        // Views from submodules (views are currently always public)
        module
            .all_views_with_prefix()
            .into_iter()
            .filter(|(prefix, _, _)| !prefix.is_empty())
            .map(|(_, owning_def, view)| lang.generate_submodule_view_file(owning_def, view)),
        module.types().flat_map(|typ| lang.generate_type_files(module, typ)),
        util::iter_reducers(module, options.visibility).map(|reducer| lang.generate_reducer_file(module, reducer)),
        util::iter_procedures(module, options.visibility)
            .map(|procedure| lang.generate_procedure_file(module, procedure)),
        // Reducers from submodules
        module
            .all_reducers_with_prefix()
            .into_iter()
            .filter(|(prefix, _, reducer)| !prefix.is_empty() && !reducer.visibility.is_private())
            .map(|(_, owning_def, reducer)| lang.generate_submodule_reducer_file(owning_def, reducer)),
        // Procedures from submodules
        module
            .all_procedures_with_prefix()
            .into_iter()
            .filter(|(prefix, _, procedure)| !prefix.is_empty() && !procedure.visibility.is_private())
            .map(|(_, owning_def, procedure)| lang.generate_submodule_procedure_file(owning_def, procedure)),
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

    /// Generate a row-type file for a public table from a submodule.
    /// Uses `owning_def`'s typespace for type resolution.
    /// Filename goes in a subdirectory named after the *accessor* namespace path:
    /// e.g. `myLib/table_name_table.ts` for a submodule mounted as `myLib`, table `tableName`.
    fn generate_submodule_table_file(&self, owning_def: &ModuleDef, table: &TableDef) -> OutputFile {
        let schema = TableSchema::from_module_def(owning_def, table, (), 0.into())
            .validated()
            .expect("Failed to generate submodule table file");
        let mut file = self.generate_table_file_from_schema(owning_def, table, schema);
        let ns_path = owning_def.accessor_path().join_segments("/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate a row-type file for a view from a submodule.
    /// Filename goes in a subdirectory named after the accessor namespace path.
    fn generate_submodule_view_file(&self, owning_def: &ModuleDef, view: &ViewDef) -> OutputFile {
        let tbl = TableDef::from(view.clone());
        let schema = TableSchema::from_view_def_for_codegen(owning_def, view)
            .validated()
            .expect("Failed to generate submodule view file");
        let mut file = self.generate_table_file_from_schema(owning_def, &tbl, schema);
        let ns_path = owning_def.accessor_path().join_segments("/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate an arg-schema file for a reducer from a submodule.
    /// Filename goes in a subdirectory named after the accessor namespace path.
    fn generate_submodule_reducer_file(&self, owning_def: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let mut file = self.generate_reducer_file(owning_def, reducer);
        let ns_path = owning_def.accessor_path().join_segments("/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate an arg-schema file for a procedure from a submodule.
    /// Filename goes in a subdirectory named after the accessor namespace path.
    fn generate_submodule_procedure_file(&self, owning_def: &ModuleDef, procedure: &ProcedureDef) -> OutputFile {
        let mut file = self.generate_procedure_file(owning_def, procedure);
        let ns_path = owning_def.accessor_path().join_segments("/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    /// Generate the skeleton of a manual migration, to be added to the module's source.
    fn generate_migration(&self, dropped: &DroppedSchema) -> anyhow::Result<OutputFile> {
        let _ = dropped;
        anyhow::bail!("generating migrations is not supported for this language")
    }
}

/// The part of an old schema that a manual migration reads from.
pub struct DroppedSchema<'a> {
    /// The old module definition, i.e. the schema of the database being migrated.
    pub old: &'a ModuleDef,
    /// The hash of `old`, which the server uses to pick the migration to run.
    pub hash: Hash,
    /// The tables that the migration sees under the `dropped` namespace:
    /// those that the new schema removes or changes incompatibly.
    /// They're the tables of `old`, with the automatic migration steps that precede
    /// the manual migration applied, e.g. indexes the new schema removes are gone.
    /// Their row types are still described by `old`'s typespace.
    /// Their rows are deleted once the migration finishes.
    pub tables: Vec<TableDef>,
}

impl<'a> DroppedSchema<'a> {
    /// Determine what a manual migration from `old` to `new` needs to declare.
    ///
    /// Only tables in the root namespace are included;
    /// tables dropped from submodules are migrated by those submodules.
    pub fn new(old: &'a ModuleDef, new: &ModuleDef) -> anyhow::Result<Self> {
        let plan = ponder_manual_migrate(old, new)?;
        let hash = plan
            .manual
            .as_ref()
            .expect("ponder_manual_migrate always plans a manual migration")
            .hash;
        let mut tables: Vec<TableDef> = plan
            .dropped_tables()
            .into_iter()
            .filter(|((namespace, _), _)| namespace.is_empty())
            .map(|(_, table)| table)
            .collect();
        tables.sort_by(|a, b| a.accessor_name.cmp(&b.accessor_name));
        Ok(Self { old, hash, tables })
    }
}

/// Generate the skeleton of a manual migration from the schema `old` to the schema `new`.
pub fn generate_migration(old: &ModuleDef, new: &ModuleDef, lang: &dyn Lang) -> anyhow::Result<OutputFile> {
    lang.generate_migration(&DroppedSchema::new(old, new)?)
}
