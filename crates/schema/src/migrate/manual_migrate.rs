use spacetimedb_data_structures::error_stream::ErrorStream;

use crate::{
    def::{ManualMigrationFunctionDef, ModuleDef, ModuleDefLookup, TableDef},
    migrate::auto_migrate::AutoMigrateStep,
};

type Ident<'def, ModuleDefEntry> = <ModuleDefEntry as ModuleDefLookup>::Key<'def>;

/// A plan for a manual migration.
#[derive(Debug)]
pub struct ManualMigratePlan<'def> {
    pub old: &'def ModuleDef,
    pub new: &'def ModuleDef,
    pub tables_to_rename_before_and_delete_after: Vec<Ident<'def, TableDef>>,
    /// Auto-migrate steps to execute before running the manual migration function.
    ///
    /// This must never contain an [`AutoMigrateStep::RemoveTable`].
    /// Tables to be removed will instead be listed in [`Self::tables_to_rename_before_and_delete_after`].
    pub auto_migrate_steps_before: Vec<AutoMigrateStep<'def>>,
    pub manual_migration_function: Ident<'def, ManualMigrationFunctionDef>,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManualMigrateError {}

pub fn ponder_manual_migrate<'def>(
    old: &'def ModuleDef,
    old_hash: spacetimedb_lib::Hash,
    new: &'def ModuleDef,
) -> Result<ManualMigratePlan<'def>, ErrorStream<ManualMigrateError>> {
    todo!()
}
