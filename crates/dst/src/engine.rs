use std::{io, sync::Arc};

use spacetimedb_commitlog::SizeOnDisk;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::{IsolationLevel, TxData};
use spacetimedb_engine::error::{DBError, DatastoreError, IndexError};
use spacetimedb_engine::persistence::{DiskSizeFn, Durability as EngineDurability, Persistence};
use spacetimedb_engine::relational_db::{MutTx, RelationalDB};
use spacetimedb_engine::update::{update_database, UpdateLogger};
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{Identity, RawModuleDef};
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::sim::{Rng, Runtime as SimRuntime};
use spacetimedb_runtime::Handle;
use spacetimedb_schema::auto_migrate::{ponder_migrate, AutoMigrateStep, MigratePlan};
use spacetimedb_schema::def::{IndexAlgorithm as SchemaIndexAlgorithm, ModuleDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::page_pool::PagePool;

mod generation;
mod migrations;
mod model;
mod properties;
mod workload;

use self::migrations::{ExpectedStep, Migration};
use self::workload::{
    normalize_rows, row_to_bytes, ColumnState, CommitDelta, CountState, IndexAlgorithmState, IndexState, InsertOutcome,
    Interaction, Observation, SchemaState, SequenceState, TableDelta, TableRowCount, TableRows, TableSchemaState,
    UniqueConstraintState,
};

use crate::engine::model::Model;
use crate::engine::properties::EngineProperties;
use crate::engine::workload::WorkloadGen;
use crate::schema::{default_schema, to_raw_def, SchemaPlan};
use crate::sim::commitlog::{InMemoryCommitlog, InMemoryCommitlogHandle};
use crate::traits::{TargetDriver, TestSuite};

pub struct EngineTarget {
    db: Option<RelationalDB>,
    table_ids: Vec<TableId>,
    active_mut_tx: Option<MutTx>,
    commitlog: InMemoryCommitlog,
    runtime_handle: Handle,
    schema: SchemaPlan,
}

impl EngineTarget {
    pub fn init(schema: SchemaPlan, runtime_seed: u64) -> anyhow::Result<Self> {
        let runtime = SimRuntime::new(runtime_seed);
        let runtime_handle = Handle::simulation(runtime.handle());
        let commitlog = InMemoryCommitlog::new();
        let db = Self::open_db(&commitlog, runtime_handle.clone())?;

        Self::install_schema(&db, &schema)?;
        let table_ids = Self::load_table_ids(&db, &schema)?;

        Ok(Self {
            db: Some(db),
            table_ids,
            active_mut_tx: None,
            commitlog,
            runtime_handle,
            schema,
        })
    }

    fn open_db(commitlog: &InMemoryCommitlog, runtime_handle: Handle) -> anyhow::Result<RelationalDB> {
        let history = commitlog.open_handle()?;
        let persistence = Self::persistence(history.clone(), runtime_handle);
        let (db, connected_clients) = RelationalDB::open(
            Identity::ZERO,
            Identity::ZERO,
            history,
            Some(persistence),
            None,
            PagePool::new_for_test(),
        )?;
        anyhow::ensure!(connected_clients.is_empty(), "replay produced connected clients");
        Ok(db)
    }

    fn persistence(handle: InMemoryCommitlogHandle, runtime_handle: Handle) -> Persistence {
        let durability: Arc<EngineDurability> = Arc::new(handle);
        let disk_size: DiskSizeFn = Arc::new(|| {
            io::Result::Ok(SizeOnDisk {
                total_bytes: 0,
                total_blocks: 0,
            })
        });
        Persistence {
            durability,
            disk_size,
            snapshots: None,
            runtime: runtime_handle,
        }
    }

    fn module_def(schema: &SchemaPlan) -> anyhow::Result<ModuleDef> {
        let raw = to_raw_def(schema);
        let raw_module_def = RawModuleDef::V10(raw);
        ModuleDef::try_from(raw_module_def).map_err(|e| anyhow::anyhow!("schema validation failed: {e}"))
    }

    fn install_schema(db: &RelationalDB, schema: &SchemaPlan) -> anyhow::Result<()> {
        let module_def = Self::module_def(schema)?;

        db.with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
            for table_def in module_def.tables() {
                let tbl_schema = TableSchema::from_module_def(&module_def, table_def, (), TableId::SENTINEL);
                db.create_table(tx, tbl_schema)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    fn load_table_ids(db: &RelationalDB, schema: &SchemaPlan) -> anyhow::Result<Vec<TableId>> {
        let mut table_ids = Vec::with_capacity(schema.tables.len());
        db.with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
            for table_plan in &schema.tables {
                let id = db
                    .table_id_from_name_mut(tx, &table_plan.name)?
                    .ok_or_else(|| anyhow::anyhow!("table '{}' not found after creation", table_plan.name))?;
                table_ids.push(id);
            }
            Ok(())
        })?;
        Ok(table_ids)
    }

    fn reopen_from_commitlog(&mut self) -> anyhow::Result<()> {
        let db = self
            .db
            .take()
            .ok_or_else(|| anyhow::anyhow!("replay without open database"))?;

        drop(db);

        self.db = Some(Self::open_db(&self.commitlog, self.runtime_handle.clone())?);
        Ok(())
    }

    fn count_state(&self) -> anyhow::Result<CountState> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
        let tx = db.begin_tx(Workload::Internal);
        let mut row_counts = Vec::with_capacity(self.table_ids.len());
        let mut table_rows = Vec::with_capacity(self.table_ids.len());
        let mut schema_tables = Vec::with_capacity(self.table_ids.len());

        for (table, table_id) in self.table_ids.iter().enumerate() {
            let rows = match db.iter(&tx, *table_id) {
                Ok(iter) => normalize_rows(iter.map(|row| row.to_product_value()).collect()),
                Err(err) => {
                    let _ = db.release_tx(tx);
                    return Err(err.into());
                }
            };
            let count = rows.len() as u64;
            row_counts.push(TableRowCount { table, count });
            table_rows.push(TableRows { table, rows });

            let schema = match db.schema_for_table(&tx, *table_id) {
                Ok(schema) => schema,
                Err(err) => {
                    let _ = db.release_tx(tx);
                    return Err(err.into());
                }
            };
            let mut indexes = schema
                .indexes
                .iter()
                .map(|index| IndexState {
                    columns: index_columns(&index.index_algorithm),
                    algorithm: index_algorithm_state(&index.index_algorithm),
                })
                .collect::<Vec<_>>();
            indexes.sort();

            let mut unique_constraints = schema
                .constraints
                .iter()
                .filter_map(|constraint| {
                    constraint.data.unique_columns().map(|columns| UniqueConstraintState {
                        columns: columns.iter().map(|col| col.0 as usize).collect(),
                    })
                })
                .collect::<Vec<_>>();
            unique_constraints.sort();

            let mut sequences = schema
                .sequences
                .iter()
                .map(|sequence| SequenceState {
                    column: sequence.col_pos.0 as usize,
                })
                .collect::<Vec<_>>();
            sequences.sort();

            schema_tables.push(TableSchemaState {
                table,
                name: schema.table_name.to_string(),
                is_public: schema.table_access == StAccess::Public,
                is_event: schema.is_event,
                primary_key: schema.primary_key.map(|col| col.0 as usize),
                columns: schema
                    .columns
                    .iter()
                    .map(|column| ColumnState {
                        name: column.col_name.to_string(),
                        ty: column.col_type.clone(),
                    })
                    .collect(),
                indexes,
                unique_constraints,
                sequences,
            });
        }

        let _ = db.release_tx(tx);
        Ok(CountState {
            row_counts,
            table_rows,
            schema: SchemaState { tables: schema_tables },
        })
    }

    fn is_unique_constraint_violation(error: &DBError) -> bool {
        matches!(
            error,
            DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_)))
        )
    }

    fn commit_delta_from_tx_data(&self, tx_data: &TxData) -> CommitDelta {
        let mut tables = Vec::new();

        for (table_id, entry) in tx_data.iter_table_entries() {
            let Some(table) = self.table_ids.iter().position(|id| *id == table_id) else {
                continue;
            };

            let inserts = normalize_rows(entry.inserts.iter().cloned().collect());
            let deletes = normalize_rows(entry.deletes.iter().cloned().collect());
            if inserts.is_empty() && deletes.is_empty() && !entry.truncated {
                continue;
            }

            tables.push(TableDelta {
                table,
                inserts,
                deletes,
                truncated: entry.truncated,
            });
        }

        tables.sort_by_key(|delta| delta.table);
        CommitDelta { tables }
    }

    fn migrate(&mut self, migration: &Migration) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.active_mut_tx.is_none(),
            "migration while mutable transaction is active"
        );

        let new_schema = migration.apply_to(&self.schema)?;
        let old_module_def = Self::module_def(&self.schema)?;
        let new_module_def = Self::module_def(&new_schema)?;
        let plan = ponder_migrate(&old_module_def, &new_module_def)?;
        self.ensure_expected_plan(migration, &plan)?;

        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let _ = update_database(db, &mut tx, AuthCtx::for_testing(), plan, &DstUpdateLogger)?;
        let Some((_tx_offset, _tx_data, _tx_metrics, _reducer)) = db.commit_tx(tx)? else {
            anyhow::bail!("migration commit produced no transaction data");
        };

        self.schema = new_schema;
        self.table_ids = Self::load_table_ids(db, &self.schema)?;
        Ok(())
    }

    fn ensure_expected_plan(&self, migration: &Migration, plan: &MigratePlan<'_>) -> anyhow::Result<()> {
        let MigratePlan::Auto(plan) = plan else {
            anyhow::bail!("engine DST generated a manual migration plan");
        };

        let mut actual = plan
            .steps
            .iter()
            .map(expected_step_from_auto_step)
            .collect::<anyhow::Result<Vec<_>>>()?;
        actual.sort();

        let mut expected = migration.expected_steps();
        expected.sort();

        anyhow::ensure!(
            actual == expected,
            "engine DST generated unexpected migration steps: actual={actual:?}, expected={expected:?}"
        );
        Ok(())
    }

    pub fn execute(&mut self, interaction: &Interaction) -> anyhow::Result<Observation> {
        tracing::debug!(?interaction, "executing interaction");

        let observation = match interaction {
            Interaction::BeginMutTx => {
                anyhow::ensure!(
                    self.active_mut_tx.is_none(),
                    "begin mutable transaction while one is already active"
                );
                let db = self
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
                self.active_mut_tx = Some(db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal));
                Ok(Observation::BeganMutTx)
            }
            Interaction::Insert { table, row } => {
                let table_id = self.table_ids[*table];
                let bytes = row_to_bytes(row);
                let db = self
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("insert without active mutable transaction"))?;
                let outcome = match db.insert(tx, table_id, &bytes) {
                    Ok((_generated_columns, row, _flags)) => InsertOutcome::Accepted(row.to_product_value()),
                    // Generated rows can intentionally hit unique constraints; the oracle validates that rejection.
                    Err(error) if Self::is_unique_constraint_violation(&error) => {
                        InsertOutcome::UniqueConstraintViolation
                    }
                    Err(error) => return Err(error.into()),
                };
                Ok(Observation::Inserted { outcome })
            }
            Interaction::Delete { table, row } => {
                let table_id = self.table_ids[*table];
                let db = self
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("delete without active mutable transaction"))?;
                db.delete_by_rel(tx, table_id, [row.clone()]);
                Ok(Observation::Deleted)
            }
            Interaction::CommitTx => {
                let tx = self
                    .active_mut_tx
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("commit without active mutable transaction"))?;
                let db = self
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
                let Some((_tx_offset, tx_data, _tx_metrics, _reducer)) = db.commit_tx(tx)? else {
                    anyhow::bail!("commit produced no transaction data");
                };
                Ok(Observation::Committed {
                    delta: self.commit_delta_from_tx_data(&tx_data),
                })
            }
            Interaction::Migrate(migration) => {
                self.migrate(migration)?;
                Ok(Observation::Migrated)
            }
            Interaction::Replay => {
                let _ = self.active_mut_tx.take();
                self.reopen_from_commitlog()?;
                Ok(Observation::Replayed {
                    state: self.count_state()?,
                })
            }
        };

        match &observation {
            Ok(observation) => tracing::debug!(?observation, "observed interaction"),
            Err(error) => tracing::error!(?interaction, %error, "interaction failed"),
        }

        observation
    }
}

fn expected_step_from_auto_step(step: &AutoMigrateStep<'_>) -> anyhow::Result<ExpectedStep> {
    match step {
        AutoMigrateStep::AddTable(_) => Ok(ExpectedStep::AddTable),
        AutoMigrateStep::RemoveTable(_) => Ok(ExpectedStep::RemoveTable),
        AutoMigrateStep::AddColumns(_) => Ok(ExpectedStep::AddColumns),
        AutoMigrateStep::AddIndex(_) => Ok(ExpectedStep::AddIndex),
        AutoMigrateStep::RemoveIndex(_) => Ok(ExpectedStep::RemoveIndex),
        AutoMigrateStep::AddSequence(_) => Ok(ExpectedStep::AddSequence),
        AutoMigrateStep::RemoveSequence(_) => Ok(ExpectedStep::RemoveSequence),
        AutoMigrateStep::AddConstraint(_) => Ok(ExpectedStep::AddConstraint),
        AutoMigrateStep::RemoveConstraint(_) => Ok(ExpectedStep::RemoveConstraint),
        AutoMigrateStep::ChangeAccess(_) => Ok(ExpectedStep::ChangeAccess),
        AutoMigrateStep::ChangePrimaryKey(_) => Ok(ExpectedStep::ChangePrimaryKey),
        AutoMigrateStep::ChangeColumns(_) => Ok(ExpectedStep::ChangeColumns),
        AutoMigrateStep::ReschemaEventTable(_) => Ok(ExpectedStep::ReschemaEventTable),
        AutoMigrateStep::DisconnectAllUsers => Ok(ExpectedStep::DisconnectAllUsers),
        step => anyhow::bail!("engine DST generated unsupported migration step: {step:?}"),
    }
}

fn index_columns(algorithm: &SchemaIndexAlgorithm) -> Vec<usize> {
    algorithm.columns().iter().map(|col| col.0 as usize).collect()
}

fn index_algorithm_state(algorithm: &SchemaIndexAlgorithm) -> IndexAlgorithmState {
    match algorithm {
        SchemaIndexAlgorithm::BTree(_) => IndexAlgorithmState::BTree,
        SchemaIndexAlgorithm::Hash(_) => IndexAlgorithmState::Hash,
        SchemaIndexAlgorithm::Direct(_) => IndexAlgorithmState::Direct,
        _ => IndexAlgorithmState::Unknown,
    }
}

struct DstUpdateLogger;

impl UpdateLogger for DstUpdateLogger {
    fn info(&self, msg: &str) {
        tracing::debug!(%msg, "engine DST migration update");
    }
}

impl TargetDriver<Interaction> for EngineTarget {
    type Observation = Observation;

    async fn execute<'a>(&'a mut self, interaction: &'a Interaction) -> Result<Self::Observation, anyhow::Error> {
        EngineTarget::execute(self, interaction)
    }
}

pub struct EngineTest;

impl TestSuite for EngineTest {
    type Interaction = Interaction;

    type Interactions = WorkloadGen;

    type Target = EngineTarget;

    type Properties = EngineProperties;

    async fn build(&self, rng: Rng) -> Result<(Self::Interactions, Self::Target, Self::Properties), anyhow::Error> {
        let schema = default_schema(rng.clone());
        let runtime_seed = rng.next_u64();
        let target = EngineTarget::init(schema.clone(), runtime_seed)?;
        let properties = EngineProperties::new(schema.clone());

        let model = Model::new(schema);
        let interactions = WorkloadGen::new(rng, model);

        Ok((interactions, target, properties))
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::AlgebraicValue;
    use spacetimedb_runtime::sim::Runtime as SimRuntime;
    use spacetimedb_sats::product;

    use super::migrations::{Migration, MigrationOp};
    use super::*;
    use crate::schema::{ColumnPlan, IndexAlgorithm, IndexPlan, TablePlan, Type, UniqueConstraintPlan};

    fn migration_replay_schema() -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "kind".into(),
                        ty: Type::Sum { variants: 1 },
                    },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![0],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![],
                is_public: true,
                is_event: false,
            }],
        }
    }

    fn add_column_replay_schema() -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "score".into(),
                        ty: Type::U64,
                    },
                ],
                primary_key: Some(0),
                indexes: vec![IndexPlan {
                    columns: vec![0],
                    algorithm: IndexAlgorithm::BTree,
                }],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![],
                is_public: true,
                is_event: false,
            }],
        }
    }

    fn change_index_replay_schema() -> SchemaPlan {
        SchemaPlan {
            tables: vec![TablePlan {
                name: "items".into(),
                columns: vec![
                    ColumnPlan {
                        name: "id".into(),
                        ty: Type::U64,
                    },
                    ColumnPlan {
                        name: "score".into(),
                        ty: Type::U64,
                    },
                ],
                primary_key: Some(0),
                indexes: vec![
                    IndexPlan {
                        columns: vec![0],
                        algorithm: IndexAlgorithm::BTree,
                    },
                    IndexPlan {
                        columns: vec![1],
                        algorithm: IndexAlgorithm::BTree,
                    },
                ],
                unique_constraints: vec![UniqueConstraintPlan { columns: vec![0] }],
                sequences: vec![],
                is_public: true,
                is_event: false,
            }],
        }
    }

    fn insert_u64_rows(target: &mut EngineTarget) -> anyhow::Result<()> {
        target.execute(&Interaction::BeginMutTx)?;
        for id in 0..128u64 {
            target.execute(&Interaction::Insert {
                table: 0,
                row: product![id, id * 10],
            })?;
        }
        target.execute(&Interaction::CommitTx)?;
        Ok(())
    }

    #[test]
    fn engine_dst_smoke_runs_random_workload() -> anyhow::Result<()> {
        let mut runtime = SimRuntime::new(0);
        runtime.block_on(EngineTest.run(Rng::new(0), 1_000))?;
        Ok(())
    }

    #[test]
    fn add_column_migration_replays_with_existing_rows() -> anyhow::Result<()> {
        let mut target = EngineTarget::init(add_column_replay_schema(), 0)?;
        insert_u64_rows(&mut target)?;

        target.execute(&Interaction::Migrate(Migration {
            table: 0,
            ops: vec![MigrationOp::ChangeAccess, MigrationOp::AddColumn { ty: Type::U64 }],
        }))?;
        target.execute(&Interaction::Replay)?;

        Ok(())
    }

    #[test]
    fn change_index_migration_replays_with_existing_rows() -> anyhow::Result<()> {
        let mut target = EngineTarget::init(change_index_replay_schema(), 0)?;
        insert_u64_rows(&mut target)?;

        target.execute(&Interaction::Migrate(Migration {
            table: 0,
            ops: vec![MigrationOp::ChangeAccess, MigrationOp::ChangeIndex { index: 1 }],
        }))?;
        target.execute(&Interaction::Replay)?;

        Ok(())
    }

    #[test]
    fn migration_that_updates_st_table_and_st_column_replays() -> anyhow::Result<()> {
        let mut target = EngineTarget::init(migration_replay_schema(), 0)?;

        target.execute(&Interaction::BeginMutTx)?;
        for id in 0..128u64 {
            target.execute(&Interaction::Insert {
                table: 0,
                row: product![id, AlgebraicValue::sum(0, AlgebraicValue::U8(1))],
            })?;
        }
        target.execute(&Interaction::CommitTx)?;

        target.execute(&Interaction::Migrate(Migration {
            table: 0,
            ops: vec![MigrationOp::ChangeAccess, MigrationOp::ChangeColumnType { column: 1 }],
        }))?;
        target.execute(&Interaction::Replay)?;

        Ok(())
    }
}
