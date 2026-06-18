use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_durability::EmptyHistory;
use spacetimedb_engine::error::DBError;
use spacetimedb_engine::relational_db::{MutTx, RelationalDB};
use spacetimedb_lib::RawModuleDef;
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::sim::Rng;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::page_pool::PagePool;

mod properties;
mod workload;

use self::workload::{row_to_bytes, Interaction, Observation};

use crate::engine::properties::EngineProperties;
use crate::engine::workload::{Model, WorkloadGen};
use crate::schema::{default_schema, lower_schema, SchemaPlan};
use crate::traits::{TargetDriver, TestSuite};
pub struct EngineTarget {
    db: RelationalDB,
    schema: SchemaPlan,
    table_ids: Vec<TableId>,
    active_mut_tx: Option<MutTx>,
}

impl EngineTarget {
    pub fn init(schema: SchemaPlan) -> Result<Self, DBError> {
        let history = EmptyHistory::new();
        let (db, _) = RelationalDB::open(
            spacetimedb_lib::Identity::ZERO,
            spacetimedb_lib::Identity::ZERO,
            history,
            None,
            None,
            PagePool::new_for_test(),
        )?;

        let raw = lower_schema(&schema);
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

        Ok(Self {
            db,
            schema,
            table_ids,
            active_mut_tx: None,
        })
    }

    pub fn execute(&mut self, interaction: &Interaction) -> anyhow::Result<Observation> {
        match interaction {
            Interaction::BeginMutTx => {
                anyhow::ensure!(
                    self.active_mut_tx.is_none(),
                    "begin mutable transaction while one is already active"
                );
                self.active_mut_tx = Some(self.db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal));
                Ok(Observation::BeganMutTx)
            }
            Interaction::Insert { table, row } => {
                let table_id = self.table_ids[*table];
                let bytes = row_to_bytes(row);
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("insert without active mutable transaction"))?;
                match self.db.insert(tx, table_id, &bytes) {
                    Ok(_) => {}
                    Err(_) => {}
                }
                let count_after = self.db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Inserted { count_after })
            }
            Interaction::Delete { table, row } => {
                let table_id = self.table_ids[*table];
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("delete without active mutable transaction"))?;
                self.db.delete_by_rel(tx, table_id, [row.clone()]);
                let count_after = self.db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Deleted { count_after })
            }
            Interaction::CommitTx => {
                let tx = self
                    .active_mut_tx
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("commit without active mutable transaction"))?;
                self.db.finish_tx(tx, Ok::<(), anyhow::Error>(()))?;
                Ok(Observation::Committed)
            }
            Interaction::Count { table } => {
                let table_id = self.table_ids[*table];
                let tx = self
                    .active_mut_tx
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("count without active mutable transaction"))?;
                let count = self.db.iter_mut(tx, table_id)?.count() as u64;
                Ok(Observation::Counted { count })
            }
        }
    }

    pub fn db(&self) -> &RelationalDB {
        &self.db
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
        let target = EngineTarget::init(schema.clone())?;
        let properties = EngineProperties {};

        let model = Model::new(schema);
        let interactions = WorkloadGen::new(rng, model);

        Ok((interactions, target, properties))
    }
}
