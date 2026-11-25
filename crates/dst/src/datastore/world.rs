use std::{io, sync::Arc};

use spacetimedb_commitlog::SizeOnDisk;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::TxData;
use spacetimedb_engine::error::DBError;
use spacetimedb_engine::persistence::{DiskSizeFn, Durability as EngineDurability, Persistence};
use spacetimedb_engine::relational_db::{MutTx, RelationalDB};
use spacetimedb_lib::{Identity, RawModuleDef};
use spacetimedb_primitives::TableId;
use spacetimedb_runtime::Handle;
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_table::tiered::TieredStorageConfig;

use super::model::{Model, TxOverlay};
use super::{normalize_rows, rules, CommitDelta, CountState, TableDelta, TableRowCount};
use crate::harness::{assert_always, Choices, Rule, RunContext, World};
use crate::schema::{default_schema, to_raw_def, SchemaPlan};
use crate::sim::commitlog::{InMemoryCommitlog, InMemoryCommitlogHandle};

/// A `RelationalDB` over an in-memory commitlog, plus the model it is checked against.
pub struct Datastore {
    // Declared first so it drops, and releases the datastore lock, before `db`.
    pub(super) tx: Option<OpenTx>,
    /// `None` only inside [`Datastore::reopen_and_check`].
    db: Option<RelationalDB>,
    commitlog: InMemoryCommitlog,
    runtime: Handle,
    pub(super) schema: SchemaPlan,
    pub(super) table_ids: Vec<TableId>,
    pub(super) model: Model,
}

/// The open mutable transaction and the model's view of it.
pub(super) struct OpenTx {
    pub(super) tx: MutTx,
    pub(super) overlay: TxOverlay,
}

impl World for Datastore {
    const NAME: &'static str = "datastore";
    const RULES: &'static [Rule<Self>] = rules::RULES;

    async fn open(context: &RunContext, choices: &mut Choices) -> anyhow::Result<Self> {
        let schema = default_schema(choices);
        let commitlog = InMemoryCommitlog::new();
        let db = open_db(&commitlog, context.handle.clone())?;
        install_schema(&db, &schema)?;
        let table_ids = load_table_ids(&db, &schema)?;
        let model = Model::new(schema.clone());

        Ok(Self {
            tx: None,
            db: Some(db),
            commitlog,
            runtime: context.handle.clone(),
            schema,
            table_ids,
            model,
        })
    }

    /// Production never holds the mutable transaction across an await.
    fn can_yield(&self) -> bool {
        self.tx.is_none()
    }
}

impl Datastore {
    pub(super) fn db(&self) -> &RelationalDB {
        self.db
            .as_ref()
            .expect("`db` is only `None` inside `reopen_and_check`, which restores it or fails the run")
    }

    /// Borrow the datastore, the open transaction and the model at once.
    pub(super) fn tx_parts(&mut self) -> (&RelationalDB, &mut OpenTx, &Model) {
        let open = self
            .tx
            .as_mut()
            .expect("rules that use the open transaction are guarded by `in_tx`");
        let db = self.db.as_ref().expect("`db` is open between steps");
        (db, open, &self.model)
    }

    /// Roll back any open transaction, reopen from the commitlog, and check
    /// that replay restored exactly the committed row counts.
    pub(super) fn reopen_and_check(&mut self) -> anyhow::Result<()> {
        self.tx = None;
        let db = self.db.take().expect("`db` is open between steps");
        drop(db);
        self.db = Some(open_db(&self.commitlog, self.runtime.clone())?);

        let replayed = self.count_state()?;
        let expected = self.model.committed_counts();
        assert_always(replayed == expected, "replay restores committed row counts", || {
            format!("replayed {replayed:?}, model {expected:?}")
        })
    }

    fn count_state(&self) -> anyhow::Result<CountState> {
        let db = self.db();
        let tx = db.begin_tx(Workload::Internal);
        let mut row_counts = Vec::with_capacity(self.table_ids.len());
        for (table, table_id) in self.table_ids.iter().enumerate() {
            let count = match db.iter(&tx, *table_id) {
                Ok(rows) => rows.count() as u64,
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

    /// The datastore's committed delta, in the model's table numbering.
    pub(super) fn commit_delta(&self, tx_data: &TxData) -> CommitDelta {
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
}

fn open_db(commitlog: &InMemoryCommitlog, runtime: Handle) -> anyhow::Result<RelationalDB> {
    let history = commitlog.open_handle()?;
    let persistence = persistence(history.clone(), runtime);
    let (db, connected_clients) = RelationalDB::open(Identity::ZERO, Identity::ZERO, history, persistence, None)?;
    anyhow::ensure!(connected_clients.is_empty(), "replay produced connected clients");
    Ok(db)
}

fn persistence(handle: InMemoryCommitlogHandle, runtime: Handle) -> Persistence {
    let durability: Arc<EngineDurability> = Arc::new(handle);
    let disk_size: DiskSizeFn = Arc::new(|| {
        io::Result::Ok(SizeOnDisk {
            total_bytes: 0,
            total_blocks: 0,
        })
    });
    Persistence::persistent(
        durability,
        disk_size,
        None,
        TieredStorageConfig::new_for_test(),
        runtime,
    )
}

fn install_schema(db: &RelationalDB, schema: &SchemaPlan) -> anyhow::Result<()> {
    let raw_module_def = RawModuleDef::V10(to_raw_def(schema));
    let module_def =
        ModuleDef::try_from(raw_module_def).map_err(|e| anyhow::anyhow!("schema validation failed: {e}"))?;

    db.with_auto_commit(Workload::Internal, |tx| -> Result<(), DBError> {
        for table_def in module_def.tables() {
            let table_schema = TableSchema::from_module_def(&module_def, table_def, (), TableId::SENTINEL);
            db.create_table(tx, table_schema)?;
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
    assert!(table_ids.len() == schema.tables.len());
    Ok(table_ids)
}
