use crate::{
    def::{ManualMigrationFunctionDef, ModuleDef, ModuleDefLookup, TableDef},
    migrate::auto_migrate::AutoMigrateStep,
};

type TableIdent<'def> = <TableDef as ModuleDefLookup>::Key<'def>;

/// A plan for a manual migration.
#[derive(Debug)]
pub struct ManualMigratePlan<'def> {
    pub old: &'def ModuleDef,
    pub new: &'def ModuleDef,
    pub auto_migrate_steps_before: Vec<AutoMigrateStep<'def>>,
    pub tables_to_rename_before: Vec<(TableIdent<'def>, TableIdent<'def>)>,
    pub manual_migration_function: <ManualMigrationFunctionDef as ModuleDefLookup>::Key<'def>,
    pub tables_to_delete_after: Vec<TableIdent<'def>>,
}
