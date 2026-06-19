use std::{io, sync::Arc};

use spacetimedb_commitlog::SizeOnDisk;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_engine::error::DBError;
use spacetimedb_engine::persistence::{DiskSizeFn, Durability as EngineDurability, Persistence};
use spacetimedb_engine::relational_db::{MutTx, RelationalDB};
use spacetimedb_lib::{Identity, RawModuleDef};
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::sim::{Rng, Runtime as SimRuntime};
use spacetimedb_runtime::Handle;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::page_pool::PagePool;

mod properties;
mod workload;

use self::workload::{row_to_bytes, summarize_rows, Interaction, Observation, TableSummary};

use crate::engine::properties::EngineProperties;
use crate::engine::workload::{Model, WorkloadGen};
use crate::schema::{default_schema, lower_schema, SchemaPlan};
use crate::sim::commitlog::{InMemoryCommitlog, InMemoryCommitlogHandle};
use crate::traits::{TargetDriver, TestSuite};

pub struct EngineTarget {
    db: Option<RelationalDB>,
    schema: SchemaPlan,
    table_ids: Vec<TableId>,
    active_mut_tx: Option<MutTx>,
    commitlog: InMemoryCommitlog,
    runtime_handle: Handle,
    runtime: SimRuntime,
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
            schema,
            table_ids,
            active_mut_tx: None,
            commitlog,
            runtime_handle,
            runtime,
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
        let raw = lower_schema(schema);
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

    fn replay(&mut self) -> anyhow::Result<()> {
        self.active_mut_tx.take();
        let db = self
            .db
            .take()
            .ok_or_else(|| anyhow::anyhow!("replay without open database"))?;

        drop(db);

        self.db = Some(Self::open_db(&self.commitlog, self.runtime_handle.clone())?);
        Ok(())
    }

    fn table_summaries(&self) -> anyhow::Result<Vec<TableSummary>> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
        let tx = db.begin_tx(Workload::Internal);
        let mut summaries = Vec::with_capacity(self.table_ids.len());

        for table_id in &self.table_ids {
            let rows = match db.iter(&tx, *table_id) {
                Ok(iter) => iter.map(|row| row.to_product_value()).collect::<Vec<_>>(),
                Err(err) => {
                    let _ = db.release_tx(tx);
                    return Err(err.into());
                }
            };
            summaries.push(summarize_rows(&rows));
        }

        let _ = db.release_tx(tx);
        Ok(summaries)
    }

    pub fn execute(&mut self, interaction: &Interaction) -> anyhow::Result<Observation> {
        match interaction {
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
                    Ok(_) => {}
                    Err(_) => {}
                }
                let count_after = db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Inserted { count_after })
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
                let count_after = db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Deleted { count_after })
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
                db.finish_tx(tx, Ok::<(), anyhow::Error>(()))?;
                Ok(Observation::Committed {
                    summaries: self.table_summaries()?,
                })
            }
            Interaction::Count { table } => {
                let table_id = self.table_ids[*table];
                let db = self
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("database is not open"))?;
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("count without active mutable transaction"))?;
                let count = db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Counted { count })
            }
            Interaction::Replay => {
                self.replay()?;
                Ok(Observation::Replayed {
                    summaries: self.table_summaries()?,
                })
            }
        }
    }

    pub fn db(&self) -> &RelationalDB {
        self.db.as_ref().expect("database is open")
    }

    pub fn schema(&self) -> &SchemaPlan {
        &self.schema
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
