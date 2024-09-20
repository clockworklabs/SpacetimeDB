use super::datastore::locking_tx_datastore::MutTxId;
use super::relational_db::RelationalDB;
use crate::database_logger::SystemLogger;
use crate::execution_context::ExecutionContext;
use spacetimedb_data_structures::map::HashMap;
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::ColSet;
use spacetimedb_schema::auto_migrate::{AutoMigratePlan, ManualMigratePlan, MigratePlan};
use spacetimedb_schema::def::TableDef;
use spacetimedb_schema::schema::{IndexSchema, Schema, SequenceSchema, TableSchema};
use std::sync::Arc;

/// Update the database according to the migration plan.
pub fn update_database(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    plan: MigratePlan,
    system_logger: &SystemLogger,
) -> anyhow::Result<()> {
    let existing_tables = stdb.get_all_tables_mut(tx)?;

    // TODO: consider using `ErrorStream` here.
    let old_def = plan.old_def();
    for table in existing_tables
        .iter()
        .filter(|table| table.table_type != StTableType::System)
    {
        let old_def = old_def
            .table(&table.table_name[..])
            .ok_or_else(|| anyhow::anyhow!("table {} not found in old_def", table.table_name))?;

        table.check_compatible(old_def)?;
    }

    match plan {
        MigratePlan::Manual(plan) => manual_migrate_database(stdb, tx, plan, system_logger, existing_tables),
        MigratePlan::Auto(plan) => auto_migrate_database(stdb, tx, plan, system_logger, existing_tables),
    }
}

/// Manually migrate a database.
fn manual_migrate_database(
    _stdb: &RelationalDB,
    _tx: &mut MutTxId,
    _plan: ManualMigratePlan,
    _system_logger: &SystemLogger,
    _existing_tables: Vec<Arc<TableSchema>>,
) -> anyhow::Result<()> {
    unimplemented!("Manual database migrations are not yet implemented")
}

/// Automatically migrate a database.
fn auto_migrate_database(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    plan: AutoMigratePlan,
    system_logger: &SystemLogger,
    existing_tables: Vec<Arc<TableSchema>>,
) -> anyhow::Result<()> {
    // We have already checked in `migrate_database` that `existing_tables` are compatible with the `old` definition in `plan`.
    // So we can look up tables in there using unwrap.

    let table_schemas_by_name = existing_tables
        .into_iter()
        .map(|table| (table.table_name.clone(), table))
        .collect::<HashMap<_, _>>();

    let ctx = &ExecutionContext::internal(stdb.address());

    for precheck in plan.prechecks {
        match precheck {
            spacetimedb_schema::auto_migrate::AutoMigratePrecheck::CheckAddSequenceRangeValid(sequence_name) => {
                let table_def = plan.new.stored_in_table_def(sequence_name).unwrap();
                let sequence_def = &table_def.sequences[sequence_name];

                let table_schema = &table_schemas_by_name[&table_def.name[..]];

                let min: AlgebraicValue = sequence_def.min_value.unwrap_or(1).into();
                let max: AlgebraicValue = sequence_def.max_value.unwrap_or(i128::MAX).into();

                let range = min..max;

                if stdb
                    .iter_by_col_range_mut(ctx, tx, table_schema.table_id, sequence_def.column, range)?
                    .next()
                    .is_some()
                {
                    anyhow::bail!(
                        "Precheck failed: added sequence {} already has values in range",
                        sequence_name,
                    );
                }
            }
        }
    }

    for step in plan.steps {
        match step {
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddTable(table_name) => {
                let table_def: &TableDef = plan.new.expect_lookup(table_name);

                // Recursively sets IDs to 0.
                // They will be initialized by the database when the table is created.
                let table_schema = TableSchema::from_module_def(plan.new, table_def, (), 0.into());

                system_logger.info(&format!("Creating table `{}`", table_name));
                stdb.create_table(tx, table_schema)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddIndex(index_name) => {
                let table_def = plan.new.stored_in_table_def(index_name).unwrap();
                let index_def = table_def.indexes.get(index_name).unwrap();
                let table_schema = &table_schemas_by_name[&table_def.name[..]];

                let index_cols = ColSet::from(index_def.algorithm.columns());

                let is_unique = plan
                    .new
                    .constraints()
                    .filter_map(|c| c.data.unique_columns())
                    .any(|unique_cols| unique_cols == &index_cols);

                system_logger.info(&format!(
                    "Creating index `{}` on table `{}`",
                    index_name, table_def.name
                ));

                let index_schema = IndexSchema::from_module_def(plan.new, index_def, table_schema.table_id, 0.into());

                stdb.create_index(tx, index_schema, is_unique)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveIndex(index_name) => {
                let table_def = plan.old.stored_in_table_def(index_name).unwrap();

                let table_schema = &table_schemas_by_name[&table_def.name[..]];
                let index_schema = table_schema
                    .indexes
                    .iter()
                    .find(|index| index.index_name[..] == index_name[..])
                    .unwrap();

                system_logger.info(&format!(
                    "Dropping index `{}` on table `{}`",
                    index_name, table_def.name
                ));
                stdb.drop_index(tx, index_schema.index_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveConstraint(constraint_name) => {
                let table_def = plan.old.stored_in_table_def(constraint_name).unwrap();
                let table_schema = &table_schemas_by_name[&table_def.name[..]];
                let constraint_schema = table_schema
                    .constraints
                    .iter()
                    .find(|constraint| constraint.constraint_name[..] == constraint_name[..])
                    .unwrap();

                system_logger.info(&format!(
                    "Dropping constraint `{}` on table `{}`",
                    constraint_name, table_def.name
                ));
                stdb.drop_constraint(tx, constraint_schema.constraint_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddSequence(sequence_name) => {
                let table_def = plan.new.stored_in_table_def(sequence_name).unwrap();
                let sequence_def = table_def.sequences.get(sequence_name).unwrap();
                let table_schema = &table_schemas_by_name[&table_def.name[..]];

                system_logger.info(&format!(
                    "Adding sequence `{}` to table `{}`",
                    sequence_name, table_def.name
                ));
                let sequence_schema =
                    SequenceSchema::from_module_def(plan.new, sequence_def, table_schema.table_id, 0.into());
                stdb.create_sequence(tx, sequence_schema)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveSequence(sequence_name) => {
                let table_def = plan.old.stored_in_table_def(sequence_name).unwrap();
                let table_schema = &table_schemas_by_name[&table_def.name[..]];
                let sequence_schema = table_schema
                    .sequences
                    .iter()
                    .find(|sequence| sequence.sequence_name[..] == sequence_name[..])
                    .unwrap();

                system_logger.info(&format!(
                    "Dropping sequence `{}` from table `{}`",
                    sequence_name, table_def.name
                ));
                stdb.drop_sequence(tx, sequence_schema.sequence_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::ChangeAccess(table_name) => {
                let table_def = plan.new.stored_in_table_def(table_name).unwrap();
                stdb.alter_table_access(tx, table_name[..].into(), table_def.table_access.into())?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddSchedule(_) => {
                anyhow::bail!("Adding schedules is not yet implemented");
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveSchedule(_) => {
                anyhow::bail!("Removing schedules is not yet implemented");
            }
        }
    }
    Ok(())
}
