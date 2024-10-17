use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use futures::StreamExt;
use rustc_hash::FxHashMap;
use spacetimedb_client_api_messages::energy::EnergyQuanta;
use spacetimedb_lib::scheduler::ScheduleAt;
use spacetimedb_lib::Address;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{bsatn::ToBsatn as _, AlgebraicValue};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_table::table::RowRef;
use tokio::sync::mpsc;
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue, DelayQueue};

use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::system_tables::{StFields, StScheduledFields, ST_SCHEDULED_ID};
use crate::db::datastore::traits::IsolationLevel;
use crate::db::relational_db::RelationalDB;
use crate::execution_context::ExecutionContext;

use super::module_host::ModuleEvent;
use super::module_host::ModuleFunctionCall;
use super::module_host::{CallReducerParams, WeakModuleHost};
use super::module_host::{DatabaseUpdate, EventStatus};
use super::{ModuleHost, ReducerArgs, ReducerCallError};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ScheduledReducerId {
    /// The ID of the table whose rows hold the scheduled reducers.
    /// This table should have a entry in `ST_SCHEDULED`.
    table_id: TableId,
    /// The particular schedule row in the reducer scheduling table referred to by `self.table_id`.
    schedule_id: u64,
}

enum MsgOrExit<T> {
    Msg(T),
    Exit,
}

enum SchedulerMessage {
    Schedule { id: ScheduledReducerId, at: ScheduleAt },
    ScheduleImmediate { reducer_name: String, args: ReducerArgs },
}

pub struct ScheduledReducer {
    reducer: Box<str>,
    bsatn_args: Vec<u8>,
}

#[derive(Clone)]
pub struct Scheduler {
    tx: mpsc::UnboundedSender<MsgOrExit<SchedulerMessage>>,
    db: Arc<RelationalDB>,
}

pub struct SchedulerStarter {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    db: Arc<RelationalDB>,
}

impl Scheduler {
    pub fn open(db: Arc<RelationalDB>) -> (Self, SchedulerStarter) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Scheduler { tx, db: db.clone() }, SchedulerStarter { rx, db })
    }

    pub fn new_with_same_db(&self) -> (Self, SchedulerStarter) {
        Self::open(self.db.clone())
    }
}

const SCHEDULED_AT_FIELD: [&str; 2] = ["scheduled_at", "ScheduledAt"];
const SCHEDULED_ID_FIELD: [&str; 2] = ["scheduled_id", "ScheduledId"];

impl SchedulerStarter {
    // TODO(cloutiertyler): This whole start dance is scuffed, but I don't have
    // time to make it better right now.
    pub fn start(mut self, module_host: &ModuleHost) -> anyhow::Result<()> {
        let mut queue: DelayQueue<QueueItem> = DelayQueue::new();
        let ctx = &ExecutionContext::internal(self.db.address());
        let tx = self.db.begin_tx();

        // Draining rx before processing schedules from the DB to ensure there are no in-flight messages,
        // as this can result in duplication.
        //
        // Explanation: By this time, if the `Scheduler::schedule` method has been called (the `init` reducer can do that),
        // there will be an in-flight message in tx that has already been inserted into the DB.
        // We are building the `queue` below with the DB and then spawning `SchedulerActor`, which will processes
        // the in-flight message, resulting in a duplicate entry in the queue.
        while self.rx.try_recv().is_ok() {}

        // Find all Scheduled tables
        for st_scheduled_row in self.db.iter(ctx, &tx, ST_SCHEDULED_ID)? {
            let table_id = st_scheduled_row.read_col(StScheduledFields::TableId)?;

            // Insert each entry (row) in the scheduled table into `queue`.
            for scheduled_row in self.db.iter(ctx, &tx, table_id)? {
                let schedule_id = get_schedule_id(&tx, &self.db, table_id, &scheduled_row)?;
                let schedule_at = get_schedule_at(&tx, &self.db, table_id, &scheduled_row)?;
                // calculate duration left to call the scheduled reducer
                let duration = schedule_at.to_duration_from_now();
                queue.insert(QueueItem::Id(ScheduledReducerId { table_id, schedule_id }), duration);
            }
        }

        tokio::spawn(
            SchedulerActor {
                rx: self.rx,
                queue,
                key_map: FxHashMap::default(),
                module_host: module_host.downgrade(),
            }
            .run(),
        );

        Ok(())
    }
}

/// The maximum `Duration` into the future that we can schedule a reducer.
///
/// `tokio_utils::time::DelayQueue`, as of version 0.7.8,
/// limits its scheduling to at most approx. 2 years into the future.
/// More specifically, they define:
/// ```ignore
/// const NUM_LEVELS: usize = 6;
/// const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;
/// ```
/// These specific incantations have to do with the internal representation
/// of `DelayQueue`.
///
/// Unfortunately, rather than returning an `Err`
/// if the requested duration is longer than `MAX_DURATION`,
/// `DelayQueue` will panic.
/// We can't allow users to crash SpacetimeDB
/// by scheduling a reducer in the distant future,
/// so we have to re-derive their maximum delay
/// and check against it ourselves.
///
/// The exact range of delays supported by `DelayQueue` may change in the future,
/// but (hopefully) it won't ever shrink, as that would be a user-visible regression.
/// If `DelayQueue` extends to support a larger range,
/// we may reject some long-delayed schedule calls which could succeed,
/// but we will never permit a schedule attempt which will panic.
const MAX_SCHEDULE_DELAY: std::time::Duration = std::time::Duration::from_millis(
    // Equal to 64^6 - 1 milliseconds, which is 2.177589 years.
    (1 << (6 * 6)) - 1,
);

#[derive(thiserror::Error, Debug)]
pub enum ScheduleError {
    #[error("Unable to schedule with long delay at {0:?}")]
    DelayTooLong(Duration),

    #[error("Unable to read scheduled row: {0:?}")]
    DecodingError(anyhow::Error),
}

impl Scheduler {
    pub fn schedule(&self, table_id: TableId, schedule_id: u64, schedule_at: ScheduleAt) -> Result<(), ScheduleError> {
        // Check that `at` is within `tokio_utils::time::DelayQueue`'s accepted time-range.
        //
        // `DelayQueue` uses a sliding window,
        // and there may be some non-zero delay between this check
        // and the actual call to `DelayQueue::insert`.
        //
        // Assuming a monotonic clock,
        // this means we may reject some otherwise acceptable schedule calls.
        //
        // If `Timestamp::now()`, i.e. `std::time::SystemTime::now()`,
        // is not monotonic,
        // `DelayQueue::insert` may panic.
        // This will happen if a module attempts to schedule a reducer
        // with a delay just before the two-year limit,
        // and the system clock is adjusted backwards
        // after the check but before scheduling so that after the adjustment,
        // the delay is beyond the two-year limit.
        //
        // We could avoid this edge case by scheduling in terms of the monotonic `Instant`,
        // rather than `SystemTime`,
        // but we don't currently have a meaningful way
        // to convert a `Timestamp` into an `Instant`.
        let delay = schedule_at.to_duration_from_now();
        if delay >= MAX_SCHEDULE_DELAY {
            return Err(ScheduleError::DelayTooLong(delay));
        }

        // if the actor has exited, it's fine to ignore; it means that the host actor calling
        // schedule will exit soon as well, and it'll be scheduled to run when the module host restarts
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Schedule {
            id: ScheduledReducerId { table_id, schedule_id },
            at: schedule_at,
        }));

        Ok(())
    }

    pub fn volatile_nonatomic_schedule_immediate(&self, reducer_name: String, args: ReducerArgs) {
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::ScheduleImmediate {
            reducer_name,
            args,
        }));
    }

    pub fn close(&self) {
        let _ = self.tx.send(MsgOrExit::Exit);
    }
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    queue: DelayQueue<QueueItem>,
    key_map: FxHashMap<ScheduledReducerId, delay_queue::Key>,
    module_host: WeakModuleHost,
}

enum QueueItem {
    Id(ScheduledReducerId),
    VolatileNonatomicImmediate { reducer_name: String, args: ReducerArgs },
}

impl SchedulerActor {
    async fn run(mut self) {
        loop {
            tokio::select! {
                msg = self.rx.recv() => match msg {
                    Some(MsgOrExit::Msg(msg)) => self.handle_message(msg),
                    // it's fine to just drop any messages in the queue because they've
                    // already been stored in the database
                    Some(MsgOrExit::Exit) | None => break,
                },
                Some(scheduled) = self.queue.next() => {
                    self.handle_queued(scheduled).await;
                }
            }
        }
    }

    fn handle_message(&mut self, msg: SchedulerMessage) {
        match msg {
            SchedulerMessage::Schedule { id, at } => {
                // Incase of row update, remove the existing entry from queue first
                if let Some(key) = self.key_map.get(&id) {
                    self.queue.remove(key);
                }
                let key = self.queue.insert(QueueItem::Id(id), at.to_duration_from_now());
                self.key_map.insert(id, key);
            }
            SchedulerMessage::ScheduleImmediate { reducer_name, args } => {
                self.queue.insert(
                    QueueItem::VolatileNonatomicImmediate { reducer_name, args },
                    Duration::ZERO,
                );
            }
        }
    }

    async fn handle_queued(&mut self, id: Expired<QueueItem>) {
        let item = id.into_inner();
        let id = match item {
            QueueItem::Id(id) => Some(id),
            QueueItem::VolatileNonatomicImmediate { .. } => None,
        };
        if let Some(id) = id {
            self.key_map.remove(&id);
        }

        let Some(module_host) = self.module_host.upgrade() else {
            return;
        };
        let db = module_host.replica_ctx().relational_db.clone();
        let ctx = ExecutionContext::internal(db.address());
        let caller_identity = module_host.info().identity;
        let module_info = module_host.info.clone();

        let call_reducer_params = move |tx: &MutTxId| -> Result<Option<CallReducerParams>, anyhow::Error> {
            let id = match item {
                QueueItem::Id(id) => id,
                QueueItem::VolatileNonatomicImmediate { reducer_name, args } => {
                    let (reducer_seed, reducer_id) = module_info
                        .reducer_seed_and_id(&reducer_name[..])
                        .ok_or_else(|| anyhow!("Reducer not found: {}", reducer_name))?;
                    let reducer_args = args.into_tuple(reducer_seed)?;

                    return Ok(Some(CallReducerParams {
                        timestamp: Timestamp::now(),
                        caller_identity,
                        caller_address: Address::default(),
                        client: None,
                        request_id: None,
                        timer: None,
                        reducer_id,
                        args: reducer_args,
                    }));
                }
            };

            let Ok(schedule_row) = get_schedule_row_mut(&ctx, tx, &db, id) else {
                // if the row is not found, it means the schedule is cancelled by the user
                log::debug!(
                    "table row corresponding to yeild scheduler id not found: tableid {}, schedulerId {}",
                    id.table_id,
                    id.schedule_id
                );
                return Ok(None);
            };

            let ScheduledReducer { reducer, bsatn_args } =
                proccess_schedule(&ctx, tx, &db, id.table_id, &schedule_row)?;

            let (reducer_seed, reducer_id) = module_info
                .reducer_seed_and_id(&reducer[..])
                .ok_or_else(|| anyhow!("Reducer not found: {}", reducer))?;

            let reducer_args = ReducerArgs::Bsatn(bsatn_args.into()).into_tuple(reducer_seed)?;

            Ok(Some(CallReducerParams {
                timestamp: Timestamp::now(),
                caller_identity,
                caller_address: Address::default(),
                client: None,
                request_id: None,
                timer: None,
                reducer_id,
                args: reducer_args,
            }))
        };

        let db = module_host.replica_ctx().relational_db.clone();
        let module_host_clone = module_host.clone();
        let ctx = ExecutionContext::internal(db.address());

        let res = tokio::spawn(async move { module_host.call_scheduled_reducer(call_reducer_params).await }).await;

        match res {
            // if we didn't actually call the reducer because the module exited or it was already deleted, leave
            // the ScheduledReducer in the database for when the module restarts
            Ok(Err(ReducerCallError::NoSuchModule(_)) | Err(ReducerCallError::ScheduleReducerNotFound)) => {}

            // delete the scheduled reducer row if its not repeated reducer
            Ok(_) | Err(_) => {
                if let Some(id) = id {
                    self.delete_scheduled_reducer_row(&ctx, &db, id, module_host_clone)
                        .await;
                }
            }
        }

        if let Err(e) = res {
            log::error!("invoking scheduled reducer failed: {e:#}");
        };
    }

    /// Handle repeated schedule by adding it back to queue
    /// return true if it is repeated schedule
    fn handle_repeated_schedule(
        &mut self,
        tx: &MutTxId,
        db: &RelationalDB,
        id: ScheduledReducerId,
        schedule_row: &RowRef<'_>,
    ) -> Result<bool, anyhow::Error> {
        let schedule_at = get_schedule_at_mut(tx, db, id.table_id, schedule_row)?;

        if let ScheduleAt::Interval(dur) = schedule_at {
            let key = self
                .queue
                .insert(QueueItem::Id(id), dur.to_duration().unwrap_or(Duration::ZERO));
            self.key_map.insert(id, key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn delete_scheduled_reducer_row(
        &mut self,
        ctx: &ExecutionContext,
        db: &RelationalDB,
        id: ScheduledReducerId,
        module_host: ModuleHost,
    ) {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

        match get_schedule_row_mut(ctx, &tx, db, id) {
            Ok(schedule_row) => {
                if let Ok(is_repeated) = self.handle_repeated_schedule(&tx, db, id, &schedule_row) {
                    if is_repeated {
                        return; // Do not delete entry for repeated reducer
                    }

                    let row_ptr = schedule_row.pointer();
                    db.delete(&mut tx, id.table_id, [row_ptr]);

                    commit_and_broadcast_deletion_event(ctx, tx, module_host);
                }
            }
            Err(_) => {
                log::debug!(
                    "Table row corresponding to yield scheduler ID not found: table_id {}, scheduler_id {}",
                    id.table_id,
                    id.schedule_id
                );
            }
        }
    }
}

fn commit_and_broadcast_deletion_event(ctx: &ExecutionContext, tx: MutTxId, module_host: ModuleHost) {
    let caller_identity = module_host.info().identity;

    let event = ModuleEvent {
        timestamp: Timestamp::now(),
        caller_identity,
        caller_address: None,
        function_call: ModuleFunctionCall::default(),
        status: EventStatus::Committed(DatabaseUpdate::default()),
        //Keeping them 0 as it is internal transaction, not by reducer
        energy_quanta_used: EnergyQuanta { quanta: 0 },
        host_execution_duration: Duration::from_millis(0),
        request_id: None,
        timer: None,
    };

    if let Err(e) = module_host
        .info()
        .subscriptions
        .commit_and_broadcast_event(None, event, ctx, tx)
    {
        log::error!("Failed to broadcast deletion event: {e:#}");
    }
}

/// Generate `ScheduledReducer` for given `ScheduledReducerId`
fn proccess_schedule(
    ctx: &ExecutionContext,
    tx: &MutTxId,
    db: &RelationalDB,
    table_id: TableId,
    schedule_row: &RowRef<'_>,
) -> Result<ScheduledReducer, anyhow::Error> {
    // get reducer name from `ST_SCHEDULED` table
    let table_id_col = StScheduledFields::TableId.col_id();
    let reducer_name_col = StScheduledFields::ReducerName.col_id();
    let st_scheduled_row = db
        .iter_by_col_eq_mut(ctx, tx, ST_SCHEDULED_ID, table_id_col, &table_id.into())?
        .next()
        .ok_or_else(|| {
            anyhow!(
                "Scheduled table with id {} entry does not exist in `st_scheduled`",
                table_id
            )
        })?;
    let reducer = st_scheduled_row.read_col::<Box<str>>(reducer_name_col)?;

    Ok(ScheduledReducer {
        reducer,
        bsatn_args: schedule_row.to_bsatn_vec()?,
    })
}

/// Helper to get schedule_id from schedule_row with `TxId`
fn get_schedule_id(tx: &TxId, db: &RelationalDB, table_id: TableId, schedule_row: &RowRef<'_>) -> anyhow::Result<u64> {
    let schema = db.schema_for_table(tx, table_id)?;
    let schedule_id_pos = schema
        .get_column_id_by_name(SCHEDULED_ID_FIELD[0])
        .or_else(|| schema.get_column_id_by_name(SCHEDULED_ID_FIELD[1]))
        .ok_or_else(|| anyhow!("Column '{}' not found in table {}", SCHEDULED_ID_FIELD[0], table_id))?;

    schedule_row.read_col::<u64>(schedule_id_pos).map_err(|e| {
        anyhow!(
            "Error reading column '{}' from schedule table id:{}, row: {} ",
            SCHEDULED_ID_FIELD[0],
            table_id,
            e
        )
    })
}

/// Helper to get schedule_row with `MutTxId`
fn get_schedule_row_mut<'a>(
    ctx: &'a ExecutionContext,
    tx: &'a MutTxId,
    db: &'a RelationalDB,
    id: ScheduledReducerId,
) -> anyhow::Result<RowRef<'a>> {
    let ScheduledReducerId { schedule_id, table_id } = id;
    let schema = db.schema_for_table_mut(tx, table_id)?;
    let scheduled_id_pos = schema
        .get_column_id_by_name(SCHEDULED_ID_FIELD[0])
        .or_else(|| schema.get_column_id_by_name(SCHEDULED_ID_FIELD[1]))
        .ok_or_else(|| anyhow!("Column '{}' not found in table {}", SCHEDULED_ID_FIELD[0], table_id))?;

    db.iter_by_col_eq_mut(ctx, tx, table_id, scheduled_id_pos, &schedule_id.into())?
        .next()
        .ok_or_else(|| anyhow!("Schedule with ID {} not found in table {}", schedule_id, table_id))
}

/// Helper to get schedule_id and schedule_at from schedule_row product value
pub fn get_schedule_from_row(
    tx: &MutTxId,
    db: &RelationalDB,
    table_id: TableId,
    row: &RowRef<'_>,
) -> anyhow::Result<(u64, ScheduleAt)> {
    let row_ty = db.row_schema_for_table(tx, table_id)?;

    let col_pos = |field_name: &str| -> anyhow::Result<usize> {
        row_ty
            .index_of_field_name(field_name)
            .ok_or_else(|| anyhow!("Column '{}' not found in row schema for table {}", field_name, table_id))
    };

    let schedule_id_col_pos = col_pos(SCHEDULED_ID_FIELD[0]).or_else(|_| col_pos(SCHEDULED_ID_FIELD[1]))?;
    let schedule_at_col_pos = col_pos(SCHEDULED_AT_FIELD[0]).or_else(|_| col_pos(SCHEDULED_AT_FIELD[1]))?;

    let schedule_id: u64 = row.read_col(schedule_id_col_pos)?;
    let schedule_at_av: AlgebraicValue = row.read_col(schedule_at_col_pos)?;
    let schedule_at = ScheduleAt::try_from(schedule_at_av.clone()).map_err(|e| {
        anyhow!(
            "Failed to convert field '{}' to ScheduleAt: {:?}",
            SCHEDULED_AT_FIELD[0],
            e
        )
    })?;

    Ok((schedule_id, schedule_at))
}

/// Helper to get schedule_at from schedule_row with `TxId`
fn get_schedule_at(
    tx: &TxId,
    db: &RelationalDB,
    table_id: TableId,
    schedule_row: &RowRef<'_>,
) -> anyhow::Result<ScheduleAt> {
    let schema = db.schema_for_table(tx, table_id)?;
    get_schedule_at_from_schema(table_id, schema, schedule_row)
}

/// Helper to get schedule_at from schedule_row with `MutTxId`
fn get_schedule_at_mut(
    tx: &MutTxId,
    db: &RelationalDB,
    table_id: TableId,
    schedule_row: &RowRef<'_>,
) -> anyhow::Result<ScheduleAt> {
    let schema = db.schema_for_table_mut(tx, table_id)?;
    get_schedule_at_from_schema(table_id, schema, schedule_row)
}

/// Helper to get schedule_at from schedule_row
fn get_schedule_at_from_schema(
    table_id: TableId,
    table_schema: Arc<TableSchema>,
    schedule_row: &RowRef<'_>,
) -> anyhow::Result<ScheduleAt> {
    let schedule_at_pos = table_schema
        .get_column_id_by_name(SCHEDULED_AT_FIELD[0])
        .or_else(|| table_schema.get_column_id_by_name(SCHEDULED_AT_FIELD[1]))
        .ok_or_else(|| anyhow!("Column '{}' not found in table {}", SCHEDULED_AT_FIELD[0], table_id))?;

    schedule_row
        .read_col::<AlgebraicValue>(schedule_at_pos)?
        .try_into()
        .map_err(|_| anyhow!("Failed to convert column '{}' to ScheduleAt", SCHEDULED_AT_FIELD[0],))
}
