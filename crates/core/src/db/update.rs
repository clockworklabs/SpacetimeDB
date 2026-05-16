use super::relational_db::RelationalDB;
use crate::database_logger::SystemLogger;
use crate::sql::parser::RowLevelExpr;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::{ColSet, TableId};
use spacetimedb_schema::auto_migrate::{AutoMigratePlan, ManualMigratePlan, MigratePlan};
use spacetimedb_schema::def::{TableDef, ViewDef};
use spacetimedb_schema::schema::{column_schemas_from_defs, IndexSchema, Schema, SequenceSchema, TableSchema};

/// The logger used for by [`update_database`] and friends.
pub trait UpdateLogger {
    fn info(&self, msg: &str);
}

impl UpdateLogger for SystemLogger {
    fn info(&self, msg: &str) {
        self.info(msg);
    }
}

/// The result of a database update.
/// Indicates whether clients should be disconnected when the update is complete.
#[must_use]
pub enum UpdateResult {
    Success,
    RequiresClientDisconnect,
    EvaluateSubscribedViews,
}

/// Update the database according to the migration plan.
///
/// The update is performed within the transactional context `tx`.
// NOTE: Manual migration support is predicated on the transactionality of
// dropping database objects (tables, indexes, etc.).
// Currently, none of the drop_* methods are transactional.
// This is safe because the __update__ reducer is no longer supported,
// and the auto plan guarantees that the migration can't fail.
// But when implementing manual migrations, we need to make sure that
// drop_* become transactional.
pub fn update_database(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    auth_ctx: AuthCtx,
    plan: MigratePlan,
    logger: &dyn UpdateLogger,
) -> anyhow::Result<UpdateResult> {
    let existing_tables = stdb.get_all_tables_mut(tx)?;

    // TODO: consider using `ErrorStream` here.
    let old_module_def = plan.old_def();
    for table in existing_tables
        .iter()
        .filter(|table| table.table_type != StTableType::System && !table.is_view())
    {
        let old_def = old_module_def
            .table(&table.table_name[..])
            .ok_or_else(|| anyhow::anyhow!("table {} not found in old_module_def", table.table_name))?;

        table.check_compatible(old_module_def, old_def)?;
    }

    match plan {
        MigratePlan::Manual(plan) => manual_migrate_database(stdb, tx, plan, logger),
        MigratePlan::Auto(plan) => auto_migrate_database(stdb, tx, auth_ctx, plan, logger),
    }
}

/// Manually migrate a database.
fn manual_migrate_database(
    _stdb: &RelationalDB,
    _tx: &mut MutTxId,
    _plan: ManualMigratePlan,
    _logger: &dyn UpdateLogger,
) -> anyhow::Result<UpdateResult> {
    unimplemented!("Manual database migrations are not yet implemented")
}

/// Logs with `info` level to `$logger` as well as via the `log` crate.
macro_rules! log {
    ($logger:expr, $($tokens:tt)*) => {
        $logger.info(&format!($($tokens)*));
        log::info!($($tokens)*);
    };
}

/// Automatically migrate a database.
fn auto_migrate_database(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    auth_ctx: AuthCtx,
    plan: AutoMigratePlan,
    logger: &dyn UpdateLogger,
) -> anyhow::Result<UpdateResult> {
    log::info!("Running database update prechecks: {}", stdb.database_identity());
    // We used to memoize all table schemas upfront, which cause issue #3441.
    // Schema should be queries only when needed to ensure that any schema changes made during earlier migration steps are visible
    // to later steps.

    for precheck in plan.prechecks {
        match precheck {
            spacetimedb_schema::auto_migrate::AutoMigratePrecheck::CheckAddSequenceRangeValid(sequence_name) => {
                let table_def = plan.new.stored_in_table_def(sequence_name).unwrap();
                let sequence_def = &table_def.sequences[sequence_name];
                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();

                let ty = table_def
                    .get_column(sequence_def.column)
                    .ok_or_else(|| {
                        anyhow::anyhow!("Precheck failed: added sequence {sequence_name} refers to unknown column")
                    })?
                    .ty
                    .clone();

                // Convert `SequenceDef` min/max to `AlgebraicValue`s of the correct type.
                let min = AlgebraicValue::from_i128(&ty, sequence_def.min_value.unwrap_or(1)).ok_or_else(|| {
                    anyhow::anyhow!("Precheck failed: added sequence {sequence_name} has invalid min value")
                })?;

                let max =
                    AlgebraicValue::from_i128(&ty, sequence_def.max_value.unwrap_or(i128::MAX)).ok_or_else(|| {
                        anyhow::anyhow!("Precheck failed: added sequence {sequence_name} has invalid max value")
                    })?;

                let range = min..max;
                if stdb
                    .iter_by_col_range_mut(tx, table_id, sequence_def.column, range)?
                    .next()
                    .is_some()
                {
                    anyhow::bail!("Precheck failed: added sequence {sequence_name} already has values in range",);
                }
            }
        }
    }

    log::info!("Running database update steps: {}", stdb.database_identity());
    let mut res = UpdateResult::Success;

    for step in plan.steps {
        match step {
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveTable(table_name) => {
                let table_id = stdb.table_id_from_name_mut(tx, table_name)?.unwrap();

                if stdb.table_row_count_mut(tx, table_id).unwrap_or(0) > 0 {
                    anyhow::bail!(
                        "Cannot remove table `{table_name}`: table contains data. \
                         Clear the table's rows (e.g. via a reducer) before removing it from your schema."
                    );
                }

                log!(logger, "Dropping table `{table_name}`");
                stdb.drop_table(tx, table_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddTable(table_name) => {
                let table_def: &TableDef = plan.new.expect_lookup(table_name);

                // Recursively sets IDs to 0.
                // They will be initialized by the database when the table is created.
                let table_schema = TableSchema::from_module_def(plan.new, table_def, (), TableId::SENTINEL);

                log!(logger, "Creating table `{table_name}`");

                stdb.create_table(tx, table_schema)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddView(view_name) => {
                let view_def: &ViewDef = plan.new.expect_lookup(view_name);
                stdb.create_view(tx, plan.new, view_def)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveView(view_name) => {
                let view_id = stdb.view_id_from_name_mut(tx, view_name)?.unwrap();
                stdb.drop_view(tx, view_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::UpdateView(_) => {
                // if we already have to disconnect clients, no need to set
                // `EvaluateSubscribedViews` as clients will be disconnected anyway
                if !matches!(res, UpdateResult::RequiresClientDisconnect) {
                    res = UpdateResult::EvaluateSubscribedViews;
                }
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddIndex(index_name) => {
                let table_def = plan.new.stored_in_table_def(index_name).unwrap();
                let index_def = table_def.indexes.get(index_name).unwrap();
                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();

                let index_cols = ColSet::from(index_def.algorithm.columns());

                let is_unique = table_def
                    .constraints
                    .iter()
                    .filter_map(|(_, c)| c.data.unique_columns())
                    .any(|unique_cols| unique_cols == &index_cols);

                log!(logger, "Creating index `{}` on table `{}`", index_name, table_def.name);

                let index_schema = IndexSchema::from_module_def(plan.new, index_def, table_id, 0.into());

                stdb.create_index(tx, index_schema, is_unique)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveIndex(index_name) => {
                let table_def = plan.old.stored_in_table_def(index_name).unwrap();

                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();
                let table_schema = stdb.schema_for_table_mut(tx, table_id)?;

                let index_schema = table_schema
                    .indexes
                    .iter()
                    .find(|index| index.index_name[..] == index_name[..])
                    .unwrap();

                log!(logger, "Dropping index `{}` on table `{}`", index_name, table_def.name);
                stdb.drop_index(tx, index_schema.index_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveConstraint(constraint_name) => {
                let table_def = plan.old.stored_in_table_def(constraint_name).unwrap();

                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();
                let table_schema = stdb.schema_for_table_mut(tx, table_id)?;
                let constraint_schema = table_schema
                    .constraints
                    .iter()
                    .find(|constraint| constraint.constraint_name[..] == constraint_name[..])
                    .unwrap();

                log!(
                    logger,
                    "Dropping constraint `{}` on table `{}`",
                    constraint_name,
                    table_def.name
                );
                stdb.drop_constraint(tx, constraint_schema.constraint_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddSequence(sequence_name) => {
                let table_def = plan.new.stored_in_table_def(sequence_name).unwrap();
                let sequence_def = table_def.sequences.get(sequence_name).unwrap();

                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();
                let table_schema = stdb.schema_for_table_mut(tx, table_id)?;

                log!(
                    logger,
                    "Adding sequence `{}` to table `{}`",
                    sequence_name,
                    table_def.name
                );
                let sequence_schema =
                    SequenceSchema::from_module_def(plan.new, sequence_def, table_schema.table_id, 0.into());
                stdb.create_sequence(tx, sequence_schema)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveSequence(sequence_name) => {
                let table_def = plan.old.stored_in_table_def(sequence_name).unwrap();

                let table_id = stdb.table_id_from_name_mut(tx, &table_def.name)?.unwrap();
                let table_schema = stdb.schema_for_table_mut(tx, table_id)?;
                let sequence_schema = table_schema
                    .sequences
                    .iter()
                    .find(|sequence| sequence.sequence_name[..] == sequence_name[..])
                    .unwrap();

                log!(
                    logger,
                    "Dropping sequence `{}` from table `{}`",
                    sequence_name,
                    table_def.name
                );
                stdb.drop_sequence(tx, sequence_schema.sequence_id)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::ChangeColumns(table_name) => {
                let table_def = plan.new.stored_in_table_def(&table_name.clone().into()).unwrap();
                let table_id = stdb.table_id_from_name_mut(tx, table_name).unwrap().unwrap();
                let column_schemas = column_schemas_from_defs(plan.new, &table_def.columns, table_id);

                log!(logger, "Changing columns of table `{}`", table_name);

                stdb.alter_table_row_type(tx, table_id, column_schemas)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::ChangeAccess(table_name) => {
                let table_def = plan.new.stored_in_table_def(&table_name.clone().into()).unwrap();
                stdb.alter_table_access(tx, table_name, table_def.table_access.into())?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::ChangePrimaryKey(table_name) => {
                let table_def = plan.new.stored_in_table_def(&table_name.clone().into()).unwrap();
                log!(logger, "Changing primary key for table `{table_name}`");
                stdb.alter_table_primary_key(tx, table_name, table_def.primary_key)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddSchedule(_) => {
                anyhow::bail!("Adding schedules is not yet implemented");
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveSchedule(_) => {
                anyhow::bail!("Removing schedules is not yet implemented");
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddRowLevelSecurity(sql_rls) => {
                log!(logger, "Adding row-level security `{sql_rls}`");
                let rls = plan.new.lookup_expect(sql_rls);
                let rls = RowLevelExpr::build_row_level_expr(tx, &auth_ctx, rls)?;

                stdb.create_row_level_security(tx, rls.def)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::RemoveRowLevelSecurity(sql_rls) => {
                log!(logger, "Removing-row level security `{sql_rls}`");
                stdb.drop_row_level_security(tx, sql_rls.clone())?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::AddColumns(table_name) => {
                let table_def = plan
                    .new
                    .stored_in_table_def(&table_name.clone().into())
                    .expect("table must exist");
                let table_id = stdb.table_id_from_name_mut(tx, table_name).unwrap().unwrap();
                let column_schemas = column_schemas_from_defs(plan.new, &table_def.columns, table_id);

                let default_values: Vec<AlgebraicValue> = table_def
                    .columns
                    .iter()
                    .filter_map(|col_def| col_def.default_value.clone())
                    .collect();
                stdb.add_columns_to_table_mut_tx(tx, table_id, column_schemas, default_values)?;
            }
            spacetimedb_schema::auto_migrate::AutoMigrateStep::DisconnectAllUsers => {
                log!(logger, "Disconnecting all users");
                // It does not disconnect clients right away,
                // but send response indicated that caller should drop clients
                res = UpdateResult::RequiresClientDisconnect;
            }
        }
    }

    log::info!("Database update complete");
    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        db::relational_db::tests_utils::{begin_mut_tx, insert, TestDB},
        host::module_host::create_table_from_def,
    };
    use spacetimedb_datastore::locking_tx_datastore::PendingSchemaChange;
    use spacetimedb_lib::db::raw_def::v9::{btree, RawIndexAlgorithm, RawModuleDefV9Builder, TableAccess};
    use spacetimedb_sats::{product, AlgebraicType, AlgebraicType::U64};
    use spacetimedb_schema::{auto_migrate::ponder_migrate, def::ModuleDef};

    struct TestLogger;
    impl UpdateLogger for TestLogger {
        fn info(&self, _: &str) {}
    }

    #[test]
    fn update_db_repro_2761() -> anyhow::Result<()> {
        let auth_ctx = AuthCtx::for_testing();
        let stdb = TestDB::durable()?;

        // Define the old and new modules, the latter with the index on `b`.
        let define_p = |builder: &mut RawModuleDefV9Builder| {
            builder
                .build_table_with_new_type("p", [("x", U64), ("y", U64)], true)
                .with_unique_constraint(0)
                .with_unique_constraint(1)
                .with_index(btree(0), "idx_x")
                .with_index(btree(1), "idx_y")
                .with_access(TableAccess::Public)
                .finish()
        };
        let define_t = |builder: &mut RawModuleDefV9Builder, with_index| {
            let builder = builder
                .build_table_with_new_type("t", [("a", U64), ("b", U64)], true)
                .with_access(TableAccess::Public);

            let builder = if with_index {
                builder.with_index(btree(1), "idx_b")
            } else {
                builder
            };

            builder.finish()
        };
        let module_def = |with_index| -> ModuleDef {
            let mut builder = RawModuleDefV9Builder::new();
            define_p(&mut builder);
            define_t(&mut builder, with_index);
            builder
                .finish()
                .try_into()
                .expect("builder should create a valid database definition")
        };

        let old = module_def(false);
        let new = module_def(true);

        // Create tables for `old`.
        let mut tx = begin_mut_tx(&stdb);
        for def in old.tables() {
            create_table_from_def(&stdb, &mut tx, &old, def)?;
        }

        // Write two rows to `t`
        // that would cause a unique constraint violation if `idx_b` was unique.
        let t_id = stdb
            .table_id_from_name_mut(&tx, "t")?
            .expect("there should be a table with name `t`");
        insert(&stdb, &mut tx, t_id, &product![0u64, 42u64])?;
        insert(&stdb, &mut tx, t_id, &product![1u64, 42u64])?;
        stdb.commit_tx(tx)?;

        // Try to update the db.
        let mut tx = begin_mut_tx(&stdb);
        let plan = ponder_migrate(&old, &new)?;
        let res = update_database(&stdb, &mut tx, auth_ctx, plan, &TestLogger)?;
        matches!(res, UpdateResult::Success);

        // Expect the schema change.
        let idx_b_id = stdb
            .index_id_from_name(&tx, "t_b_idx_btree")?
            .expect("there should be an index named `idx_b`");
        assert_eq!(
            tx.pending_schema_changes(),
            [PendingSchemaChange::IndexAdded(t_id, idx_b_id, None)]
        );

        Ok(())
    }

    /// Regression test for #3934: removing a primary key annotation and then
    /// re-publishing causes "Primary key mismatch" on the NEXT publish.
    #[test]
    fn update_db_remove_primary_key_issue_3934() -> anyhow::Result<()> {
        let auth_ctx = AuthCtx::for_testing();
        let stdb = TestDB::durable()?;

        // Step 1: Table with a primary key (requires unique constraint + index).
        let module_v1: ModuleDef = {
            let mut builder = RawModuleDefV9Builder::new();
            builder
                .build_table_with_new_type("person", [("name", AlgebraicType::String)], true)
                .with_primary_key(0)
                .with_unique_constraint(0)
                .with_index(btree(0), "person_name_idx")
                .with_access(TableAccess::Public)
                .finish();
            let raw: ModuleDef = builder.finish().try_into().expect("valid module def");
            raw
        };

        // Step 2: Same table, but primary key removed.
        let module_v2: ModuleDef = {
            let mut builder = RawModuleDefV9Builder::new();
            builder
                .build_table_with_new_type("person", [("name", AlgebraicType::String)], true)
                .with_access(TableAccess::Public)
                .finish();
            let raw: ModuleDef = builder.finish().try_into().expect("valid module def");
            raw
        };

        // Step 3: Trivially different module (same as v2, simulates "change anything").
        let module_v3 = {
            let mut builder = RawModuleDefV9Builder::new();
            builder
                .build_table_with_new_type("person", [("name", AlgebraicType::String)], true)
                .with_access(TableAccess::Public)
                .finish();
            builder.add_reducer("noop", spacetimedb_sats::ProductType::unit(), None);
            let raw: ModuleDef = builder.finish().try_into().expect("valid module def");
            raw
        };

        // Publish v1.
        let mut tx = begin_mut_tx(&stdb);
        for def in module_v1.tables() {
            create_table_from_def(&stdb, &mut tx, &module_v1, def)?;
        }
        stdb.commit_tx(tx)?;

        // Migrate v1 → v2 (remove primary key). Should succeed.
        let mut tx = begin_mut_tx(&stdb);
        let plan = ponder_migrate(&module_v1, &module_v2)?;
        let res = update_database(&stdb, &mut tx, auth_ctx.clone(), plan, &TestLogger)?;
        assert!(matches!(res, UpdateResult::Success), "v1 → v2 migration failed");
        stdb.commit_tx(tx)?;

        // Migrate v2 → v3 (trivial change). This is where #3934 crashes.
        let mut tx = begin_mut_tx(&stdb);
        let plan = ponder_migrate(&module_v2, &module_v3)?;
        let res = update_database(&stdb, &mut tx, auth_ctx, plan, &TestLogger)?;
        assert!(
            matches!(res, UpdateResult::Success),
            "v2 → v3 migration failed (issue #3934)"
        );
        stdb.commit_tx(tx)?;

        Ok(())
    }

    fn empty_module() -> ModuleDef {
        RawModuleDefV9Builder::new()
            .finish()
            .try_into()
            .expect("empty module should be valid")
    }

    fn single_table_module() -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        builder
            .build_table_with_new_type("droppable", [("id", U64)], true)
            .with_access(TableAccess::Public)
            .finish();
        builder
            .finish()
            .try_into()
            .expect("should be a valid module definition")
    }

    #[test]
    fn remove_empty_table_succeeds() -> anyhow::Result<()> {
        let auth_ctx = AuthCtx::for_testing();
        let stdb = TestDB::durable()?;

        let old = single_table_module();
        let new = empty_module();

        let mut tx = begin_mut_tx(&stdb);
        for def in old.tables() {
            create_table_from_def(&stdb, &mut tx, &old, def)?;
        }
        stdb.commit_tx(tx)?;

        let mut tx = begin_mut_tx(&stdb);
        let plan = ponder_migrate(&old, &new)?;
        let res = update_database(&stdb, &mut tx, auth_ctx, plan, &TestLogger)?;

        assert!(
            matches!(res, UpdateResult::RequiresClientDisconnect),
            "removing a table should disconnect clients"
        );
        assert!(stdb.table_id_from_name_mut(&tx, "droppable")?.is_none());
        assert!(
            tx.pending_schema_changes()
                .iter()
                .any(|c| matches!(c, PendingSchemaChange::TableRemoved(..))),
            "dropping a table should produce a TableRemoved pending schema change: {:?}",
            tx.pending_schema_changes()
        );
        Ok(())
    }

    #[test]
    fn remove_nonempty_table_fails() -> anyhow::Result<()> {
        let auth_ctx = AuthCtx::for_testing();
        let stdb = TestDB::durable()?;

        let old = single_table_module();
        let new = empty_module();

        let mut tx = begin_mut_tx(&stdb);
        for def in old.tables() {
            create_table_from_def(&stdb, &mut tx, &old, def)?;
        }
        let table_id = stdb
            .table_id_from_name_mut(&tx, "droppable")?
            .expect("table should exist");
        insert(&stdb, &mut tx, table_id, &product![42u64])?;
        stdb.commit_tx(tx)?;

        let mut tx = begin_mut_tx(&stdb);
        let plan = ponder_migrate(&old, &new)?;
        let result = update_database(&stdb, &mut tx, auth_ctx, plan, &TestLogger);
        let err = result.err().expect("removing a non-empty table should fail");
        assert!(
            err.to_string().contains("table contains data"),
            "error should mention that the table contains data, got: {err}"
        );
        assert!(
            tx.pending_schema_changes().is_empty(),
            "failed migration should leave no pending schema changes: {:?}",
            tx.pending_schema_changes()
        );
        Ok(())
    }

    /// Verifies that `autoinc` sequence survives a schema migration that adds a column,
    /// and is also correctly persisted across database replay.
    ///
    /// Flow:
    /// - Create v1 schema and consume a few sequence values.
    /// - Migrate to v2 (adds a column with a default).
    /// - Ensure next insert continues the sequence (no reset).
    /// - Reopen DB and verify allocation cursor is still preserved.
    #[test]
    fn auto_inc_sequence_survives_add_column_migration() -> anyhow::Result<()> {
        let auth_ctx = AuthCtx::for_testing();
        let stdb = TestDB::durable()?;

        // Define the old module that was before.
        let module_v1: ModuleDef = {
            let mut b = RawModuleDefV9Builder::new();
            b.build_table_with_new_type("seq_t", [("id", AlgebraicType::I64)], true)
                .with_auto_inc_primary_key(0)
                .with_index_no_accessor_name(RawIndexAlgorithm::BTree { columns: 0.into() })
                .with_access(TableAccess::Public)
                .finish();
            b.finish().try_into().expect("valid module v1")
        };

        // Define the module that we're migrating to.
        let module_v2: ModuleDef = {
            let mut b = RawModuleDefV9Builder::new();
            b.build_table_with_new_type(
                "seq_t",
                [("id", AlgebraicType::I64), ("payload", AlgebraicType::U64)],
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0))
            .with_access(TableAccess::Public)
            .with_default_column_value(1, product![0u64].into())
            .finish();
            b.finish().try_into().expect("valid module v2")
        };

        // helper to insert + collect sorted ids
        let insert_and_collect_ids = |stdb: &TestDB, payload: AlgebraicValue| -> anyhow::Result<Vec<i64>> {
            let mut tx = begin_mut_tx(stdb);
            let table_id = stdb.table_id_from_name_mut(&tx, "seq_t")?.expect("seq_t should exist");

            insert(stdb, &mut tx, table_id, &payload)?;

            let mut ids = stdb
                .iter_mut(&tx, table_id)?
                .map(|r| r.read_col::<i64>(0))
                .collect::<Result<Vec<_>, _>>()?;

            ids.sort();
            stdb.commit_tx(tx)?;
            Ok(ids)
        };

        // Create the old tables and insert two rows
        // that use the auto-inc sequence.
        {
            let mut tx = begin_mut_tx(&stdb);

            for def in module_v1.tables() {
                create_table_from_def(&stdb, &mut tx, &module_v1, def)?;
            }

            let table_id = stdb.table_id_from_name_mut(&tx, "seq_t")?.expect("seq_t should exist");

            insert(&stdb, &mut tx, table_id, &product![0i64])?;
            insert(&stdb, &mut tx, table_id, &product![0i64])?;

            stdb.commit_tx(tx)?;
        }

        // Successfully update the database to the new module.
        {
            let mut tx = begin_mut_tx(&stdb);

            let plan = ponder_migrate(&module_v1, &module_v2)?;
            let res = update_database(&stdb, &mut tx, auth_ctx, plan, &TestLogger)?;

            assert!(matches!(
                res,
                UpdateResult::Success | UpdateResult::RequiresClientDisconnect
            ));

            stdb.commit_tx(tx)?;
        }

        // Check that the new table has reused the sequence
        // from the old table such that the last row has the value 3.
        {
            let ids = insert_and_collect_ids(&stdb, product![0i64, 99u64].into())?;
            assert!(
                ids.iter().last().unwrap() == &3,
                "expected id 3 after migration, got {ids:?}"
            );
        }

        // Check that we can replay.
        let stdb = stdb.reopen()?;

        // After replay, the allocation cursor should be preserved.
        {
            let ids = insert_and_collect_ids(&stdb, product![0i64, 99u64].into())?;
            assert!(
                ids.iter().last().unwrap() == &4097,
                "expected id 4097 after reopen, got {ids:?}"
            );
        }

        Ok(())
    }
}
