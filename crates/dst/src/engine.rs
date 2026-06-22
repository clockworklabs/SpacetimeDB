use std::{io, sync::Arc};

use spacetimedb_commitlog::SizeOnDisk;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::{IsolationLevel, TxData};
use spacetimedb_engine::error::{DBError, DatastoreError, IndexError};
use spacetimedb_engine::persistence::{DiskSizeFn, Durability as EngineDurability, Persistence};
use spacetimedb_engine::relational_db::{MutTx, RelationalDB};
use spacetimedb_lib::{Identity, RawModuleDef};
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::sim::{Rng, Runtime as SimRuntime};
use spacetimedb_runtime::Handle;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::page_pool::PagePool;

mod model;
mod properties;
mod workload;

use self::workload::{
    normalize_rows, row_to_bytes, CommitDelta, CountState, Interaction, Observation, TableDelta, TableRowCount,
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
    row_counts: Vec<u64>,
    active_mut_tx: Option<MutTx>,
    commitlog: InMemoryCommitlog,
    runtime_handle: Handle,
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
            row_counts: vec![0; table_ids.len()],
            table_ids,
            active_mut_tx: None,
            commitlog,
            runtime_handle,
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

    fn install_schema(db: &RelationalDB, schema: &SchemaPlan) -> anyhow::Result<()> {
        let raw = to_raw_def(schema);
        let raw_module_def = RawModuleDef::V10(raw);
        let module_def =
            ModuleDef::try_from(raw_module_def).map_err(|e| anyhow::anyhow!("schema validation failed: {e}"))?;

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

        for (table, table_id) in self.table_ids.iter().enumerate() {
            let count = match db.iter(&tx, *table_id) {
                Ok(iter) => iter.count() as u64,
                Err(err) => {
                    let _ = db.release_tx(tx);
                    return Err(err.into());
                }
            };
            row_counts.push(TableRowCount { table, count });
        }

        let _ = db.release_tx(tx);
        Ok(CountState { row_counts })
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
                match db.insert(tx, table_id, &bytes) {
                    Ok(_) => self.row_counts[*table] += 1,
                    // Generated rows can intentionally hit unique constraints; the model treats those inserts as no-ops.
                    Err(error) if Self::is_unique_constraint_violation(&error) => {}
                    Err(error) => return Err(error.into()),
                }
                Ok(Observation::Inserted {
                    rows_count: self.row_counts[*table],
                })
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
                let deleted = db.delete_by_rel(tx, table_id, [row.clone()]) as u64;
                self.row_counts[*table] = self.row_counts[*table]
                    .checked_sub(deleted)
                    .ok_or_else(|| anyhow::anyhow!("delete removed more rows than were tracked"))?;
                Ok(Observation::Deleted {
                    rows_count: self.row_counts[*table],
                })
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
            Interaction::Replay => {
                let _ = self.active_mut_tx.take();
                self.reopen_from_commitlog()?;
                let state = self.count_state()?;
                self.row_counts = state.row_counts.iter().map(|row_count| row_count.count).collect();
                Ok(Observation::Replayed { state })
            }
        };

        match &observation {
            Ok(observation) => tracing::debug!(?observation, "observed interaction"),
            Err(error) => tracing::error!(?interaction, %error, "interaction failed"),
        }

        observation
    }
}

pub struct Outcome;
impl TargetDriver<Interaction> for EngineTarget {
    type Observation = Observation;

    type Outcome = Outcome;

    fn execute(&mut self, interaction: &Interaction) -> Result<Self::Observation, anyhow::Error> {
        EngineTarget::execute(self, interaction)
    }
}
pub struct EngineTest;

impl TestSuite for EngineTest {
    type Interaction = Interaction;

    type Interactions = WorkloadGen;

    type Target = EngineTarget;

    type Properties = EngineProperties;

    fn build(&self, rng: Rng) -> Result<(Self::Interactions, Self::Target, Self::Properties), anyhow::Error> {
        let schema = default_schema(rng.clone());
        let runtime_seed = rng.next_u64();
        let target = EngineTarget::init(schema.clone(), runtime_seed)?;
        let properties = EngineProperties::new(schema.clone());

        let model = Model::new(schema);
        let interactions = WorkloadGen::new(rng, model);

        Ok((interactions, target, properties))
    }
}
