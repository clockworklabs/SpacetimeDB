//! Concurrent RelationalDB API target.
//!
//! The target models concurrency at RelationalDB lock boundaries. A generated
//! round may hold one or more read transactions, or one write transaction, and
//! then probe whether another client can acquire the write lock. Once a client
//! owns a `Tx` or `MutTx`, that section is synchronous: no simulator yield or
//! async boundary is allowed until the transaction is released, committed, or
//! rolled back.

use std::{collections::BTreeMap, fmt};

use spacetimedb_core::{
    db::relational_db::{MutTx as RelMutTx, RelationalDB, Tx as RelTx},
    error::DBError,
    messages::control_db::HostType,
};
use spacetimedb_datastore::{execution_context::Workload, traits::IsolationLevel};
use spacetimedb_durability::EmptyHistory;
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::{
    def::BTreeAlgorithm,
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;
use tracing::info;

use crate::{
    client::SessionId,
    config::RunConfig,
    core::{self, StreamingProperties, TargetEngine, WorkloadSource},
    schema::SimRow,
    seed::{DstRng, DstSeed},
};

pub async fn run_generated_with_config(
    seed: DstSeed,
    config: RunConfig,
) -> anyhow::Result<RelationalDbConcurrentOutcome> {
    let source = ConcurrentWorkloadSource::new(seed, config.max_interactions_or_default(usize::MAX));
    let engine = ConcurrentRelationalDbEngine::new()?;
    let outcome = core::run_streaming(source, engine, ConcurrentProperties, config).await?;
    info!(
        rounds = outcome.rounds,
        committed = outcome.committed,
        conflicts = outcome.write_conflicts,
        "relational_db_concurrent complete"
    );
    Ok(outcome)
}

#[derive(Clone, Debug)]
struct RoundPlan {
    id: u64,
    kind: RoundKind,
    shared: SimRow,
    extra: SimRow,
}

#[derive(Clone, Copy, Debug)]
enum RoundKind {
    WriterBlocksWriter,
    ReadersBlockWriter,
    MultiReaderSnapshot,
    MixedReadWrite,
}

struct ConcurrentWorkloadSource {
    rng: DstRng,
    emitted: usize,
    target: usize,
    next_id: u64,
}

impl ConcurrentWorkloadSource {
    fn new(seed: DstSeed, target: usize) -> Self {
        Self {
            rng: seed.fork(910).rng(),
            emitted: 0,
            target,
            next_id: seed.fork(911).0.max(1),
        }
    }

    fn make_row(&mut self) -> SimRow {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        SimRow {
            values: vec![
                AlgebraicValue::U64(id),
                AlgebraicValue::U64(self.rng.next_u64() % 1_000),
            ],
        }
    }

    fn make_round(&mut self, id: u64) -> RoundPlan {
        RoundPlan {
            id,
            kind: match id % 4 {
                0 => RoundKind::WriterBlocksWriter,
                1 => RoundKind::ReadersBlockWriter,
                2 => RoundKind::MultiReaderSnapshot,
                _ => RoundKind::MixedReadWrite,
            },
            shared: self.make_row(),
            extra: self.make_row(),
        }
    }
}

impl WorkloadSource for ConcurrentWorkloadSource {
    type Interaction = RoundPlan;

    fn next_interaction(&mut self) -> Option<Self::Interaction> {
        if self.emitted >= self.target {
            return None;
        }
        let round = self.make_round(self.emitted as u64);
        self.emitted += 1;
        Some(round)
    }

    fn request_finish(&mut self) {
        self.target = self.emitted;
    }
}

struct ConcurrentRelationalDbEngine {
    db: RelationalDB,
    table_id: TableId,
    events: Vec<RoundEvent>,
}

impl ConcurrentRelationalDbEngine {
    fn new() -> anyhow::Result<Self> {
        let (db, connected_clients) = RelationalDB::open(
            Identity::ZERO,
            Identity::ZERO,
            EmptyHistory::new(),
            None,
            None,
            PagePool::new_for_test(),
        )?;
        assert_eq!(connected_clients.len(), 0);
        db.with_auto_commit(Workload::Internal, |tx| {
            db.set_initialized(tx, spacetimedb_datastore::traits::Program::empty(HostType::Wasm.into()))
        })?;

        let table_id = install_concurrent_schema(&db)?;
        Ok(Self {
            db,
            table_id,
            events: Vec::new(),
        })
    }

    fn execute_round(&mut self, round: &RoundPlan) -> Result<RoundObservation, String> {
        let mut machine = RoundMachine::new(&self.db, self.table_id, round.id, 4);
        let events = machine.run(round)?;
        self.events.extend(events.clone());
        Ok(RoundObservation {
            round: round.id,
            events,
        })
    }

    fn collect_rows(&self) -> Result<Vec<SimRow>, String> {
        let tx = self.db.begin_tx(Workload::ForTests);
        let result = collect_rows_in_tx(&self.db, self.table_id, &tx, "collect rows");
        let _ = self.db.release_tx(tx);
        result
    }
}

impl TargetEngine<RoundPlan> for ConcurrentRelationalDbEngine {
    type Observation = RoundObservation;
    type Outcome = RelationalDbConcurrentOutcome;
    type Error = String;

    fn execute_interaction<'a>(
        &'a mut self,
        interaction: &'a RoundPlan,
    ) -> impl Future<Output = Result<Self::Observation, Self::Error>> + 'a {
        async move { self.execute_round(interaction) }
    }

    fn finish(&mut self) {}

    fn collect_outcome<'a>(&'a mut self) -> impl Future<Output = anyhow::Result<Self::Outcome>> + 'a {
        async move {
            let final_rows = self.collect_rows().map_err(anyhow::Error::msg)?;
            let expected_rows = expected_rows_from_events(&self.events);
            let summary = ConcurrentSummary::from_events(&self.events);
            Ok(RelationalDbConcurrentOutcome {
                rounds: summary.rounds,
                clients: summary.clients,
                events: summary.events,
                reads: summary.reads,
                committed: summary.committed,
                write_conflicts: summary.write_conflicts,
                writer_conflicts: summary.writer_conflicts,
                reader_conflicts: summary.reader_conflicts,
                final_rows,
                expected_rows,
            })
        }
    }
}

struct RoundMachine<'a> {
    db: &'a RelationalDB,
    table_id: TableId,
    round: u64,
    clients: Vec<ClientState>,
    events: Vec<RoundEvent>,
}

impl<'a> RoundMachine<'a> {
    fn new(db: &'a RelationalDB, table_id: TableId, round: u64, clients: usize) -> Self {
        Self {
            db,
            table_id,
            round,
            clients: (0..clients).map(|_| ClientState::Idle).collect(),
            events: Vec::new(),
        }
    }

    fn run(&mut self, round: &RoundPlan) -> Result<Vec<RoundEvent>, String> {
        let result = match round.kind {
            RoundKind::WriterBlocksWriter => self.writer_blocks_writer(round),
            RoundKind::ReadersBlockWriter => self.readers_block_writer(round),
            RoundKind::MultiReaderSnapshot => self.multi_reader_snapshot(round),
            RoundKind::MixedReadWrite => self.mixed_read_write(round),
        };
        let cleanup = self.cleanup();
        result.and(cleanup)?;
        Ok(std::mem::take(&mut self.events))
    }

    fn writer_blocks_writer(&mut self, round: &RoundPlan) -> Result<(), String> {
        self.begin_write(client(0))?;
        self.insert(client(0), round.shared.clone())?;
        self.expect_write_conflict(client(1), ConflictReason::WriterHeld)?;
        self.commit(client(0))?;

        self.begin_write(client(1))?;
        self.insert(client(1), round.extra.clone())?;
        self.commit(client(1))
    }

    fn readers_block_writer(&mut self, round: &RoundPlan) -> Result<(), String> {
        self.begin_read(client(0))?;
        self.begin_read(client(1))?;
        self.full_scan(client(0))?;
        self.full_scan(client(1))?;
        self.expect_write_conflict(client(2), ConflictReason::ReadersHeld)?;
        self.release_read(client(0))?;
        self.release_read(client(1))?;

        self.begin_write(client(2))?;
        self.insert(client(2), round.shared.clone())?;
        self.commit(client(2))
    }

    fn multi_reader_snapshot(&mut self, round: &RoundPlan) -> Result<(), String> {
        self.begin_read(client(0))?;
        self.begin_read(client(1))?;
        let snapshot_0 = self.full_scan(client(0))?;
        let snapshot_1 = self.full_scan(client(1))?;
        if snapshot_0 != snapshot_1 {
            return Err(format!(
                "[ConcurrentRelationalDb] round={} readers observed different snapshots: left={snapshot_0:?} right={snapshot_1:?}",
                self.round
            ));
        }
        self.release_read(client(0))?;
        self.release_read(client(1))?;

        self.begin_write(client(2))?;
        self.insert(client(2), round.shared.clone())?;
        self.commit(client(2))?;

        self.begin_read(client(3))?;
        self.point_lookup(client(3), round.shared.id().ok_or("generated row missing id")?)?;
        self.release_read(client(3))
    }

    fn mixed_read_write(&mut self, round: &RoundPlan) -> Result<(), String> {
        self.begin_write(client(0))?;
        self.insert(client(0), round.shared.clone())?;
        self.commit(client(0))?;

        self.begin_read(client(1))?;
        self.point_lookup(client(1), round.shared.id().ok_or("generated row missing id")?)?;
        self.release_read(client(1))?;

        self.begin_write(client(2))?;
        self.delete(client(2), round.shared.clone())?;
        self.rollback(client(2));

        self.begin_write(client(3))?;
        self.insert(client(3), round.extra.clone())?;
        self.commit(client(3))
    }

    fn begin_read(&mut self, client: SessionId) -> Result<(), String> {
        if self.any_writer() {
            return Err(format!(
                "[ConcurrentRelationalDb] round={} client={} would block beginning read while writer is held",
                self.round, client
            ));
        }
        self.expect_idle(client, "begin_read")?;
        self.record_action(client, "begin_read");
        let tx = self.db.begin_tx(Workload::ForTests);
        self.replace(client, ClientState::Reading { tx });
        Ok(())
    }

    fn release_read(&mut self, client: SessionId) -> Result<(), String> {
        self.record_action(client, "release_read");
        match self.take(client)? {
            ClientState::Reading { tx } => {
                let _ = self.db.release_tx(tx);
                self.replace(client, ClientState::Idle);
                Ok(())
            }
            state => {
                self.replace(client, state);
                Err(self.invalid_state(client, "release_read"))
            }
        }
    }

    fn begin_write(&mut self, client: SessionId) -> Result<(), String> {
        if self.try_begin_write(client)? {
            Ok(())
        } else {
            Err(format!(
                "[ConcurrentRelationalDb] round={} client={} expected write lock to be available",
                self.round, client
            ))
        }
    }

    fn expect_write_conflict(&mut self, client: SessionId, reason: ConflictReason) -> Result<(), String> {
        if self.try_begin_write(client)? {
            self.rollback(client);
            return Err(format!(
                "[ConcurrentRelationalDb] round={} client={} unexpectedly acquired write lock",
                self.round, client
            ));
        }
        match self.events.last() {
            Some(RoundEvent::WriteConflict { reason: observed, .. }) if *observed == reason => Ok(()),
            Some(event) => Err(format!(
                "[ConcurrentRelationalDb] round={} expected conflict reason {reason:?}, observed {event}",
                self.round
            )),
            None => Err(format!(
                "[ConcurrentRelationalDb] round={} expected write conflict event",
                self.round
            )),
        }
    }

    fn try_begin_write(&mut self, client: SessionId) -> Result<bool, String> {
        self.expect_idle(client, "try_begin_write")?;
        self.record_action(client, "try_begin_write");
        match self
            .db
            .try_begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
        {
            Some(tx) => {
                self.replace(
                    client,
                    ClientState::Writing {
                        tx,
                        pending: Vec::new(),
                    },
                );
                self.events.push(RoundEvent::WriteLockAcquired {
                    round: self.round,
                    client,
                });
                Ok(true)
            }
            None => {
                self.events.push(RoundEvent::WriteConflict {
                    round: self.round,
                    client,
                    reason: self.conflict_reason(),
                });
                Ok(false)
            }
        }
    }

    fn insert(&mut self, client: SessionId, row: SimRow) -> Result<(), String> {
        self.record_action(client, "insert");
        let table_id = self.table_id;
        let db = self.db;
        self.with_writer(client, |tx, pending| {
            let bsatn = row.to_bsatn().map_err(|err| err.to_string())?;
            match db.insert(tx, table_id, &bsatn) {
                Ok((_, row_ref, _)) => {
                    pending.push(ConcurrentMutation::Inserted(SimRow::from_product_value(
                        row_ref.to_product_value(),
                    )));
                    Ok(())
                }
                Err(err) if is_unique_constraint_violation(&err) => Ok(()),
                Err(err) => Err(format!("insert failed: {err}")),
            }
        })
    }

    fn delete(&mut self, client: SessionId, row: SimRow) -> Result<(), String> {
        self.record_action(client, "delete");
        let table_id = self.table_id;
        let db = self.db;
        self.with_writer(client, |tx, pending| {
            match db.delete_by_rel(tx, table_id, [row.to_product_value()]) {
                0 => Ok(()),
                1 => {
                    pending.push(ConcurrentMutation::Deleted(row));
                    Ok(())
                }
                deleted => Err(format!("delete affected {deleted} rows")),
            }
        })
    }

    fn commit(&mut self, client: SessionId) -> Result<(), String> {
        self.record_action(client, "commit");
        match self.take(client)? {
            ClientState::Writing { tx, mut pending } => {
                let committed = self
                    .db
                    .commit_tx(tx)
                    .map_err(|err| format!("commit failed: {err}"))?
                    .ok_or_else(|| "commit returned no tx data".to_string())?;
                self.events.push(RoundEvent::Committed {
                    round: self.round,
                    client,
                    tx_offset: committed.0,
                    mutations: std::mem::take(&mut pending),
                });
                self.replace(client, ClientState::Idle);
                Ok(())
            }
            state => {
                self.replace(client, state);
                Err(self.invalid_state(client, "commit"))
            }
        }
    }

    fn rollback(&mut self, client: SessionId) {
        self.record_action(client, "rollback");
        match self.take(client) {
            Ok(ClientState::Writing { tx, .. }) => {
                let _ = self.db.rollback_mut_tx(tx);
                self.events.push(RoundEvent::RolledBack {
                    round: self.round,
                    client,
                });
                self.replace(client, ClientState::Idle);
            }
            Ok(state) => self.replace(client, state),
            Err(_) => {}
        }
    }

    fn full_scan(&mut self, client: SessionId) -> Result<ReadSummary, String> {
        self.record_action(client, "full_scan");
        let summary = self.with_reader(client, |tx| scan_summary_in_tx(self.db, self.table_id, tx, "full scan"))?;
        self.events.push(RoundEvent::Read {
            round: self.round,
            client,
            kind: ReadKind::FullScan,
            summary,
        });
        Ok(summary)
    }

    fn point_lookup(&mut self, client: SessionId, id: u64) -> Result<ReadSummary, String> {
        self.record_action(client, "point_lookup");
        let summary = self.with_reader(client, |tx| point_lookup_summary_in_tx(self.db, self.table_id, tx, id))?;
        self.events.push(RoundEvent::Read {
            round: self.round,
            client,
            kind: ReadKind::PointLookup { id },
            summary,
        });
        Ok(summary)
    }

    fn with_writer<T>(
        &mut self,
        client: SessionId,
        f: impl FnOnce(&mut RelMutTx, &mut Vec<ConcurrentMutation>) -> Result<T, String>,
    ) -> Result<T, String> {
        match self.state_mut(client)? {
            ClientState::Writing { tx, pending } => f(tx, pending),
            _ => Err(self.invalid_state(client, "write operation")),
        }
    }

    fn with_reader<T>(&self, client: SessionId, f: impl FnOnce(&RelTx) -> Result<T, String>) -> Result<T, String> {
        match self.state(client)? {
            ClientState::Reading { tx } => f(tx),
            _ => Err(self.invalid_state(client, "read operation")),
        }
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let mut leaked = None;
        for index in 0..self.clients.len() {
            let client = SessionId::from_index(index);
            match self.take(client)? {
                ClientState::Idle => self.replace(client, ClientState::Idle),
                ClientState::Reading { tx } => {
                    let _ = self.db.release_tx(tx);
                    self.replace(client, ClientState::Idle);
                    leaked.get_or_insert_with(|| {
                        format!(
                            "[ConcurrentRelationalDb] round={} client={} leaked read transaction",
                            self.round, client
                        )
                    });
                }
                ClientState::Writing { tx, .. } => {
                    let _ = self.db.rollback_mut_tx(tx);
                    self.replace(client, ClientState::Idle);
                    leaked.get_or_insert_with(|| {
                        format!(
                            "[ConcurrentRelationalDb] round={} client={} leaked write transaction",
                            self.round, client
                        )
                    });
                }
            }
        }
        match leaked {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn conflict_reason(&self) -> ConflictReason {
        if self.any_writer() {
            ConflictReason::WriterHeld
        } else if self.any_reader() {
            ConflictReason::ReadersHeld
        } else {
            ConflictReason::Unknown
        }
    }

    fn any_reader(&self) -> bool {
        self.clients.iter().any(ClientState::is_reading)
    }

    fn any_writer(&self) -> bool {
        self.clients.iter().any(ClientState::is_writing)
    }

    fn expect_idle(&self, client: SessionId, action: &'static str) -> Result<(), String> {
        if self.state(client)?.is_idle() {
            Ok(())
        } else {
            Err(self.invalid_state(client, action))
        }
    }

    fn record_action(&mut self, client: SessionId, name: &'static str) {
        self.events.push(RoundEvent::Action {
            round: self.round,
            client,
            name,
        });
    }

    fn state(&self, client: SessionId) -> Result<&ClientState, String> {
        self.clients
            .get(client.as_index())
            .ok_or_else(|| format!("[ConcurrentRelationalDb] unknown client {client}"))
    }

    fn state_mut(&mut self, client: SessionId) -> Result<&mut ClientState, String> {
        self.clients
            .get_mut(client.as_index())
            .ok_or_else(|| format!("[ConcurrentRelationalDb] unknown client {client}"))
    }

    fn take(&mut self, client: SessionId) -> Result<ClientState, String> {
        let state = self.state_mut(client)?;
        Ok(std::mem::replace(state, ClientState::Idle))
    }

    fn replace(&mut self, client: SessionId, state: ClientState) {
        self.clients[client.as_index()] = state;
    }

    fn invalid_state(&self, client: SessionId, action: &str) -> String {
        format!(
            "[ConcurrentRelationalDb] round={} client={} cannot {action} from {}",
            self.round,
            client,
            self.state(client).map(ClientState::name).unwrap_or("unknown")
        )
    }
}

enum ClientState {
    Idle,
    Reading {
        tx: RelTx,
    },
    Writing {
        tx: RelMutTx,
        pending: Vec<ConcurrentMutation>,
    },
}

impl ClientState {
    fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Reading { .. } => "reading",
            Self::Writing { .. } => "writing",
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn is_reading(&self) -> bool {
        matches!(self, Self::Reading { .. })
    }

    fn is_writing(&self) -> bool {
        matches!(self, Self::Writing { .. })
    }
}

#[derive(Clone, Debug)]
struct RoundObservation {
    round: u64,
    events: Vec<RoundEvent>,
}

#[derive(Clone, Debug)]
pub struct RelationalDbConcurrentOutcome {
    pub rounds: usize,
    pub clients: usize,
    pub events: usize,
    pub reads: usize,
    pub committed: usize,
    pub write_conflicts: usize,
    pub writer_conflicts: usize,
    pub reader_conflicts: usize,
    pub final_rows: Vec<SimRow>,
    pub expected_rows: Vec<SimRow>,
}

#[derive(Clone, Debug)]
enum RoundEvent {
    Action {
        round: u64,
        client: SessionId,
        name: &'static str,
    },
    WriteLockAcquired {
        round: u64,
        client: SessionId,
    },
    WriteConflict {
        round: u64,
        client: SessionId,
        reason: ConflictReason,
    },
    Committed {
        round: u64,
        client: SessionId,
        tx_offset: u64,
        mutations: Vec<ConcurrentMutation>,
    },
    RolledBack {
        round: u64,
        client: SessionId,
    },
    Read {
        round: u64,
        client: SessionId,
        kind: ReadKind,
        summary: ReadSummary,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConflictReason {
    WriterHeld,
    ReadersHeld,
    Unknown,
}

#[derive(Clone, Debug)]
enum ReadKind {
    FullScan,
    PointLookup { id: u64 },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReadSummary {
    row_count: usize,
    checksum: u64,
}

impl ReadSummary {
    fn add_row(&mut self, row: &SimRow, label: &'static str) -> Result<(), String> {
        self.row_count += 1;
        self.checksum = self.checksum.wrapping_add(concurrent_row_checksum(row, label)?);
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum ConcurrentMutation {
    Inserted(SimRow),
    Deleted(SimRow),
}

#[derive(Default)]
struct ConcurrentSummary {
    rounds: usize,
    clients: usize,
    events: usize,
    reads: usize,
    committed: usize,
    write_conflicts: usize,
    writer_conflicts: usize,
    reader_conflicts: usize,
}

impl ConcurrentSummary {
    fn from_events(events: &[RoundEvent]) -> Self {
        let mut summary = Self::default();
        let mut max_round = None;
        let mut max_client = None;

        for event in events {
            summary.events += 1;
            let (round, client) = event.position();
            max_round = Some(max_round.unwrap_or(round).max(round));
            max_client = Some(max_client.unwrap_or(client.as_index()).max(client.as_index()));

            match event {
                RoundEvent::WriteConflict { reason, .. } => {
                    summary.write_conflicts += 1;
                    match reason {
                        ConflictReason::WriterHeld => summary.writer_conflicts += 1,
                        ConflictReason::ReadersHeld => summary.reader_conflicts += 1,
                        ConflictReason::Unknown => {}
                    }
                }
                RoundEvent::Committed { .. } => summary.committed += 1,
                RoundEvent::Read { .. } => summary.reads += 1,
                RoundEvent::Action { .. } | RoundEvent::WriteLockAcquired { .. } | RoundEvent::RolledBack { .. } => {}
            }
        }

        summary.rounds = max_round.map(|round| round as usize + 1).unwrap_or_default();
        summary.clients = max_client.map(|client| client + 1).unwrap_or_default();
        summary
    }
}

impl RoundEvent {
    fn position(&self) -> (u64, SessionId) {
        match self {
            Self::Action { round, client, .. }
            | Self::WriteLockAcquired { round, client }
            | Self::WriteConflict { round, client, .. }
            | Self::Committed { round, client, .. }
            | Self::RolledBack { round, client }
            | Self::Read { round, client, .. } => (*round, *client),
        }
    }
}

struct ConcurrentProperties;

impl StreamingProperties<RoundPlan, RoundObservation, ConcurrentRelationalDbEngine> for ConcurrentProperties {
    fn observe(
        &mut self,
        _engine: &ConcurrentRelationalDbEngine,
        _interaction: &RoundPlan,
        observation: &RoundObservation,
    ) -> Result<(), String> {
        if observation.events.is_empty() {
            return Err(format!(
                "[ConcurrentRelationalDb] round={} produced no events",
                observation.round
            ));
        }

        for event in &observation.events {
            if let RoundEvent::Read {
                kind: ReadKind::PointLookup { id },
                summary,
                ..
            } = event
            {
                if summary.row_count > 1 {
                    return Err(format!(
                        "[ConcurrentRelationalDb] round={} invalid point lookup id={id}: {summary:?}",
                        observation.round
                    ));
                }
            }
        }
        Ok(())
    }

    fn finish(
        &mut self,
        _engine: &ConcurrentRelationalDbEngine,
        outcome: &RelationalDbConcurrentOutcome,
    ) -> Result<(), String> {
        if outcome.final_rows != outcome.expected_rows {
            return Err(format!(
                "[ConcurrentRelationalDb] final rows differ from commit-offset oracle: expected={:?} actual={:?}",
                outcome.expected_rows, outcome.final_rows
            ));
        }
        if outcome.writer_conflicts == 0 {
            return Err("[ConcurrentRelationalDb] no writer-held lock contention was observed".to_string());
        }
        if outcome.reader_conflicts == 0 {
            return Err("[ConcurrentRelationalDb] no reader-held lock contention was observed".to_string());
        }
        if outcome.reads == 0 {
            return Err("[ConcurrentRelationalDb] no read sections were observed".to_string());
        }
        Ok(())
    }
}

fn collect_rows_in_tx(
    db: &RelationalDB,
    table_id: TableId,
    tx: &RelTx,
    label: &'static str,
) -> Result<Vec<SimRow>, String> {
    let mut rows = db
        .iter(tx, table_id)
        .map_err(|err| format!("{label} failed: {err}"))?
        .map(|row_ref| SimRow::from_product_value(row_ref.to_product_value()))
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id().unwrap_or_default());
    Ok(rows)
}

fn scan_summary_in_tx(
    db: &RelationalDB,
    table_id: TableId,
    tx: &RelTx,
    label: &'static str,
) -> Result<ReadSummary, String> {
    let mut summary = ReadSummary::default();
    for row_ref in db.iter(tx, table_id).map_err(|err| format!("{label} failed: {err}"))? {
        let row = SimRow::from_product_value(row_ref.to_product_value());
        summary.add_row(&row, label)?;
    }
    Ok(summary)
}

fn point_lookup_summary_in_tx(
    db: &RelationalDB,
    table_id: TableId,
    tx: &RelTx,
    id: u64,
) -> Result<ReadSummary, String> {
    let value = AlgebraicValue::U64(id);
    let mut summary = ReadSummary::default();
    for row_ref in db
        .iter_by_col_eq(tx, table_id, 0u16, &value)
        .map_err(|err| format!("point lookup failed: {err}"))?
    {
        let row = SimRow::from_product_value(row_ref.to_product_value());
        if row.id() != Some(id) {
            return Err(format!(
                "[ConcurrentRelationalDb] point lookup id={id} returned different row: {row:?}"
            ));
        }
        summary.add_row(&row, "point lookup")?;
    }
    Ok(summary)
}

fn concurrent_row_checksum(row: &SimRow, label: &'static str) -> Result<u64, String> {
    let id = row
        .id()
        .ok_or_else(|| format!("[ConcurrentRelationalDb] {label} row missing u64 id: {row:?}"))?;
    let value = match row.values.get(1) {
        Some(AlgebraicValue::U64(value)) => *value,
        other => {
            return Err(format!(
                "[ConcurrentRelationalDb] {label} row has invalid value column: {other:?} in {row:?}"
            ));
        }
    };

    Ok(mix64(id)
        .wrapping_add(mix64(value ^ 0xa076_1d64_78bd_642f))
        .wrapping_add(mix64(row.values.len() as u64)))
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn expected_rows_from_events(events: &[RoundEvent]) -> Vec<SimRow> {
    let mut commits = events
        .iter()
        .filter_map(|event| match event {
            RoundEvent::Committed {
                tx_offset, mutations, ..
            } => Some((*tx_offset, mutations)),
            _ => None,
        })
        .collect::<Vec<_>>();
    commits.sort_by_key(|(tx_offset, _)| *tx_offset);

    let mut rows = BTreeMap::<u64, SimRow>::new();
    for (_tx_offset, mutations) in commits {
        for mutation in mutations {
            match mutation {
                ConcurrentMutation::Inserted(row) => {
                    if let Some(id) = row.id() {
                        rows.insert(id, row.clone());
                    }
                }
                ConcurrentMutation::Deleted(row) => {
                    if let Some(id) = row.id() {
                        rows.remove(&id);
                    }
                }
            }
        }
    }
    rows.into_values().collect()
}

fn install_concurrent_schema(db: &RelationalDB) -> anyhow::Result<TableId> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
    let table_id = db.create_table(
        &mut tx,
        TableSchema::new(
            TableId::SENTINEL,
            TableName::for_test("concurrent_rows"),
            None,
            vec![
                ColumnSchema::for_test(0, "id", spacetimedb_sats::AlgebraicType::U64),
                ColumnSchema::for_test(1, "value", spacetimedb_sats::AlgebraicType::U64),
            ],
            vec![IndexSchema::for_test("concurrent_rows_id_idx", BTreeAlgorithm::from(0))],
            vec![ConstraintSchema::unique_for_test("concurrent_rows_id_unique", 0)],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            Some(0.into()),
            false,
            None,
        ),
    )?;
    let _ = db.commit_tx(tx)?;
    Ok(table_id)
}

fn client(index: usize) -> SessionId {
    SessionId::from_index(index)
}

fn is_unique_constraint_violation(err: &DBError) -> bool {
    err.to_string().contains("Unique") || err.to_string().contains("unique")
}

impl fmt::Display for RoundEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Action { name, .. } => write!(f, "action({name})"),
            event => write!(f, "{event:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim;

    #[test]
    fn seed_12_exercises_lock_state_machine() {
        let seed = DstSeed(12);
        let config = RunConfig::with_max_interactions(100);
        let mut runtime = sim::Runtime::new(seed).unwrap();

        let outcome = runtime.block_on(run_generated_with_config(seed, config)).unwrap();

        assert_eq!(outcome.rounds, 100);
        assert!(outcome.committed > 0);
        assert!(outcome.writer_conflicts > 0);
        assert!(outcome.reader_conflicts > 0);
        assert!(outcome.reads > 0);
        assert_eq!(outcome.final_rows, outcome.expected_rows);
    }

    #[test]
    fn first_four_rounds_cover_core_lock_cases() {
        let seed = DstSeed(12);
        let config = RunConfig::with_max_interactions(4);
        let mut runtime = sim::Runtime::new(seed).unwrap();

        let outcome = runtime.block_on(run_generated_with_config(seed, config)).unwrap();

        assert_eq!(outcome.rounds, 4);
        assert_eq!(outcome.writer_conflicts, 1);
        assert_eq!(outcome.reader_conflicts, 1);
        assert!(outcome.reads >= 4);
        assert_eq!(outcome.final_rows, outcome.expected_rows);
    }
}
