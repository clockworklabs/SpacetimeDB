use std::{sync::mpsc, thread};

use spacetimedb_datastore::{
    execution_context::Workload,
    locking_tx_datastore::{
        datastore::Locking,
        lock_trace::{install_lock_event_hook, LockEvent, LockEventKind},
        MutTxId,
    },
    traits::{IsolationLevel, MutTx, MutTxDatastore, Tx},
};
use spacetimedb_execution::Datastore as _;
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    schema::{ColumnSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;

use crate::{
    seed::{DstRng, DstSeed},
    subsystem::{assert_invariants, DeterminismLevel, DstSubsystem, Invariant, RunRecord},
    trace::Trace,
};

pub fn bootstrap_datastore() -> spacetimedb_datastore::Result<Locking> {
    Locking::bootstrap(Identity::ZERO, PagePool::new_for_test())
}

pub fn basic_table_schema(name: &str) -> TableSchema {
    TableSchema::new(
        TableId::SENTINEL,
        TableName::for_test(name),
        None,
        vec![
            ColumnSchema::for_test(0, "id", AlgebraicType::U64),
            ColumnSchema::for_test(1, "name", AlgebraicType::String),
        ],
        vec![],
        vec![],
        vec![],
        StTableType::User,
        StAccess::Public,
        None,
        None,
        false,
        None,
    )
}

pub fn create_table(datastore: &Locking, schema: TableSchema) -> spacetimedb_datastore::Result<TableId> {
    let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
    let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
    datastore.commit_mut_tx(tx)?;
    Ok(table_id)
}

pub fn insert_row(datastore: &Locking, table_id: TableId, id: u64, name: &str) -> spacetimedb_datastore::Result<()> {
    let row = ProductValue::from_iter([AlgebraicValue::U64(id), AlgebraicValue::String(name.into())]);
    let bytes = spacetimedb_sats::bsatn::to_vec(&row).map_err(anyhow::Error::from)?;
    let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
    datastore.insert_mut_tx(&mut tx, table_id, &bytes)?;
    datastore.commit_mut_tx(tx)?;
    Ok(())
}

pub fn observe_lock_events<F, R>(hook: F, body: impl FnOnce() -> R) -> R
where
    F: Fn(LockEvent) + Send + Sync + 'static,
{
    let _guard = install_lock_event_hook(hook);
    body()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatastoreCase {
    pub seed: DstSeed,
    pub baseline: BaselinePlan,
    pub hold_reader_during_writer_start: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatastoreOutcome {
    pub baseline_row_count: u64,
    pub final_row_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselinePlan {
    pub schema: SchemaPlan,
    pub setup: Vec<SetupTxn>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaPlan {
    pub table_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupTxn {
    pub ops: Vec<SetupOp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupOp {
    Insert { id: u64, name: String },
    DeleteExisting { id: u64, name: String },
}

#[derive(Clone, Debug, Default)]
struct BaselineModel {
    existing_rows: Vec<(u64, String)>,
    next_id: u64,
}

pub struct DatastoreSubsystem;

impl DstSubsystem for DatastoreSubsystem {
    type Case = DatastoreCase;
    type Event = LockEvent;
    type Outcome = DatastoreOutcome;

    fn name() -> &'static str {
        "datastore"
    }


    fn generate_case(seed: DstSeed) -> Self::Case {
        let mut rng = seed.fork(1).rng();
        DatastoreCase {
            seed,
            baseline: generate_baseline_plan(&mut rng),
            hold_reader_during_writer_start: true,
        }
    }

    fn run_case(case: &Self::Case) -> anyhow::Result<RunRecord<Self::Case, Self::Event, Self::Outcome>> {
        let datastore = bootstrap_datastore()?;
        let table_id = apply_baseline_plan(&datastore, &case.baseline)?;

        let (tx, rx) = mpsc::channel::<LockEvent>();

        let trace_events = observe_lock_events(
            move |event| {
                tx.send(event).expect("send lock event");
            },
            || -> anyhow::Result<Vec<LockEvent>> {
                let read_tx = case
                    .hold_reader_during_writer_start
                    .then(|| datastore.begin_tx(Workload::ForTests));
                let datastore_for_writer = datastore.clone();

                let writer = thread::spawn(move || {
                    let write_tx = datastore_for_writer.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
                    let _ = datastore_for_writer.rollback_mut_tx(write_tx);
                });

                let mut events = Vec::new();
                while !events
                    .iter()
                    .any(|event: &LockEvent| event.kind == LockEventKind::BeginWriteRequested)
                {
                    events.push(rx.recv()?);
                }

                if let Some(read_tx) = read_tx {
                    drop(read_tx);
                    while !events
                        .iter()
                        .any(|event: &LockEvent| event.kind == LockEventKind::BeginWriteAcquired)
                    {
                        events.push(rx.recv()?);
                    }
                }

                writer.join().expect("writer join");
                Ok(events)
            },
        )?;

        let baseline_row_count = datastore.begin_tx(Workload::ForTests).row_count(table_id);
        let final_row_count = datastore.begin_tx(Workload::ForTests).row_count(table_id);

        let artifact = RunRecord {
            subsystem: Self::name(),
            determinism_level: Self::determinism_level(),
            seed: case.seed,
            case: case.clone(),
            trace: Some(Trace::from_events(trace_events)),
            outcome: DatastoreOutcome {
                baseline_row_count,
                final_row_count,
            },
        };

        assert_invariants(
            &artifact,
            &[
                &BaselineMatchesPlan,
                &WriterWaitsForReader,
                &RollbackPreservesBaseline,
                &ReplayableOutcome,
            ],
        )?;

        Ok(artifact)
    }
}

fn generate_baseline_plan(rng: &mut DstRng) -> BaselinePlan {
    let mut model = BaselineModel {
        existing_rows: Vec::new(),
        next_id: 1,
    };
    let tx_count = rng.index(5) + 1;
    let mut txns = Vec::with_capacity(tx_count);

    for _ in 0..tx_count {
        let op_count = rng.index(3) + 1;
        let mut ops = Vec::with_capacity(op_count);

        for _ in 0..op_count {
            let op = choose_setup_op(rng, &mut model);
            apply_op_to_model(&mut model, &op);
            ops.push(op);
        }

        txns.push(SetupTxn { ops });
    }

    BaselinePlan {
        schema: SchemaPlan {
            table_name: format!("dst_case_{}", rng.next_u64() % 10_000),
        },
        setup: txns,
    }
}

fn choose_setup_op(rng: &mut DstRng, model: &mut BaselineModel) -> SetupOp {
    let can_delete = !model.existing_rows.is_empty();
    let choose_insert = !can_delete || rng.index(100) < 70;

    if choose_insert {
        let id = model.next_id;
        SetupOp::Insert {
            id,
            name: format!("row_{}", rng.next_u64() % 1000),
        }
    } else {
        let idx = rng.index(model.existing_rows.len());
        let (id, name) = &model.existing_rows[idx];
        SetupOp::DeleteExisting {
            id: *id,
            name: name.clone(),
        }
    }
}

fn apply_op_to_model(model: &mut BaselineModel, op: &SetupOp) {
    match op {
        SetupOp::Insert { id, name } => {
            model.existing_rows.push((*id, name.clone()));
            model.next_id = model.next_id.max(id + 1);
        }
        SetupOp::DeleteExisting { id, .. } => {
            if let Some(pos) = model
                .existing_rows
                .iter()
                .position(|(existing_id, _)| existing_id == id)
            {
                model.existing_rows.remove(pos);
            }
        }
    }
}

fn apply_baseline_plan(datastore: &Locking, plan: &BaselinePlan) -> anyhow::Result<TableId> {
    let table_id = create_table(datastore, basic_table_schema(&plan.schema.table_name))?;

    for txn in &plan.setup {
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        for op in &txn.ops {
            apply_setup_op(datastore, &mut tx, table_id, op)?;
        }
        datastore.commit_mut_tx(tx)?;
    }

    Ok(table_id)
}

fn apply_setup_op(datastore: &Locking, tx: &mut MutTxId, table_id: TableId, op: &SetupOp) -> anyhow::Result<()> {
    match op {
        SetupOp::Insert { id, name } => {
            let row = ProductValue::from_iter([AlgebraicValue::U64(*id), AlgebraicValue::String(name.clone().into())]);
            let bytes = spacetimedb_sats::bsatn::to_vec(&row)?;
            datastore.insert_mut_tx(tx, table_id, &bytes)?;
        }
        SetupOp::DeleteExisting { id, name } => {
            let row = ProductValue::from_iter([AlgebraicValue::U64(*id), AlgebraicValue::String(name.clone().into())]);
            let _ = datastore.delete_by_rel_mut_tx(tx, table_id, [row]);
        }
    }
    Ok(())
}

struct WriterWaitsForReader;

impl Invariant<RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>> for WriterWaitsForReader {
    fn name(&self) -> &'static str {
        "writer-waits-for-reader"
    }

    fn check(&self, run: &RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>) -> anyhow::Result<()> {
        if !run.case.hold_reader_during_writer_start {
            return Ok(());
        }

        let trace = run
            .trace
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing diagnostic trace"))?;
        let write_requested = trace
            .as_slice()
            .iter()
            .position(|event| event.event.kind == LockEventKind::BeginWriteRequested)
            .ok_or_else(|| anyhow::anyhow!("missing write request event"))?;
        let write_acquired = trace
            .as_slice()
            .iter()
            .position(|event| event.event.kind == LockEventKind::BeginWriteAcquired)
            .ok_or_else(|| anyhow::anyhow!("missing write acquired event"))?;

        if write_acquired <= write_requested {
            anyhow::bail!("writer acquired before request ordering was established");
        }
        Ok(())
    }
}

struct RollbackPreservesBaseline;

impl Invariant<RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>> for RollbackPreservesBaseline {
    fn name(&self) -> &'static str {
        "rollback-preserves-baseline"
    }

    fn check(&self, run: &RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>) -> anyhow::Result<()> {
        if run.outcome.baseline_row_count != run.outcome.final_row_count {
            anyhow::bail!(
                "rollback changed row count: baseline={} final={}",
                run.outcome.baseline_row_count,
                run.outcome.final_row_count
            );
        }
        Ok(())
    }
}

struct ReplayableOutcome;

impl Invariant<RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>> for ReplayableOutcome {
    fn name(&self) -> &'static str {
        "trace-has-events"
    }

    fn check(&self, run: &RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>) -> anyhow::Result<()> {
        if run.trace.as_ref().is_none_or(|trace| trace.as_slice().is_empty()) {
            anyhow::bail!("trace is empty");
        }
        Ok(())
    }
}

struct BaselineMatchesPlan;

impl Invariant<RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>> for BaselineMatchesPlan {
    fn name(&self) -> &'static str {
        "baseline-matches-plan"
    }

    fn check(&self, run: &RunRecord<DatastoreCase, LockEvent, DatastoreOutcome>) -> anyhow::Result<()> {
        let expected = expected_baseline_rows(&run.case.baseline).len() as u64;
        if run.outcome.baseline_row_count != expected {
            anyhow::bail!(
                "baseline row count mismatch: expected={} actual={}",
                expected,
                run.outcome.baseline_row_count
            );
        }
        Ok(())
    }
}

fn expected_baseline_rows(plan: &BaselinePlan) -> Vec<(u64, String)> {
    let mut model = BaselineModel::default();
    for txn in &plan.setup {
        for op in &txn.ops {
            apply_op_to_model(&mut model, op);
        }
    }
    model.existing_rows
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use crate::{
        runner::{rerun_case, run_generated, verify_repeatable_execution},
        seed::DstSeed,
    };

    use super::DatastoreSubsystem;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn datastore_writer_waits_for_reader() {
        let _guard = test_lock().lock().expect("lock datastore dst tests");
        let artifact = run_generated::<DatastoreSubsystem>(DstSeed(1)).expect("run datastore dst case");
        assert_eq!(artifact.outcome.baseline_row_count, artifact.outcome.final_row_count);
    }

    #[test]
    fn rerun_reproduces_case_trace_and_outcome() {
        let _guard = test_lock().lock().expect("lock datastore dst tests");
        let artifact = run_generated::<DatastoreSubsystem>(DstSeed(9)).expect("run datastore dst case");
        let replayed = rerun_case::<DatastoreSubsystem>(&artifact).expect("rerun datastore dst case");
        assert_eq!(artifact.case, replayed.case);
        assert_eq!(artifact.trace, replayed.trace);
        assert_eq!(artifact.outcome, replayed.outcome);
    }

    #[test]
    fn observed_trace_verifies_repeatable_execution() {
        let _guard = test_lock().lock().expect("lock datastore dst tests");
        let artifact = run_generated::<DatastoreSubsystem>(DstSeed(11)).expect("run datastore dst case");
        let replayed =
            verify_repeatable_execution::<DatastoreSubsystem>(&artifact).expect("verify repeatable execution");
        assert_eq!(artifact.trace, replayed.trace);
        assert_eq!(artifact.outcome, replayed.outcome);
    }

    proptest! {
        #[test]
        fn datastore_property_holds_across_generated_seeds(seed in any::<u64>()) {
            let _guard = test_lock().lock().expect("lock datastore dst tests");
            run_generated::<DatastoreSubsystem>(DstSeed(seed))
                .unwrap_or_else(|err| panic!("seed {seed} failed: {err}"));
        }
    }
}
