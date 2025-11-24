use super::module_host::ModuleEvent;
use super::module_host::ModuleFunctionCall;
use super::module_host::{CallReducerParams, WeakModuleHost};
use super::module_host::{DatabaseUpdate, EventStatus};
use super::{FunctionArgs, ModuleHost};
use crate::db::relational_db::RelationalDB;
use crate::host::module_host::{CallProcedureParams, ModuleInfo};
use crate::host::NoSuchModule;
use crate::host::{InvalidProcedureArguments, InvalidReducerArguments};
use anyhow::anyhow;
use core::time::Duration;
use futures::StreamExt;
use itertools::Either;
use rustc_hash::FxHashMap;
use spacetimedb_client_api_messages::energy::EnergyQuanta;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::system_tables::{StFields, StScheduledFields, ST_SCHEDULED_ID};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::scheduler::ScheduleAt;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::{bsatn::ToBsatn as _, AlgebraicValue};
use spacetimedb_table::table::RowRef;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinError;
use tokio::time::Instant;
use tokio_util::time::delay_queue::{self, DelayQueue, Expired};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ScheduledFunctionId {
    /// The ID of the table whose rows hold the scheduled reducers or procedures.
    /// This table should have an entry in `ST_SCHEDULED`.
    table_id: TableId,
    /// The particular schedule row in the scheduling table referred to by `self.table_id`.
    schedule_id: u64,
    // These may seem redundant, but they're actually free - they fit in the struct padding.
    // `scheduled_id: u64, table_id: u32, id_column: u16, at_column: u16` == 16 bytes, same as
    // (`scheduled_id: u64, table_id: u32` == 12 bytes).pad_to_align() == 16 bytes
    /// The column that the primary key (`scheduled_id`) is in.
    id_column: ColId,
    /// The column that the `ScheduleAt` value is in.
    at_column: ColId,
}

spacetimedb_table::static_assert_size!(ScheduledFunctionId, 16);

enum MsgOrExit<T> {
    Msg(T),
    Exit,
}

enum SchedulerMessage {
    Schedule {
        id: ScheduledFunctionId,
        /// The timestamp we'll tell the reducer it is.
        effective_at: Timestamp,
        /// The actual instant we're scheduling for.
        real_at: Instant,
    },
    ScheduleImmediate {
        function_name: String,
        args: FunctionArgs,
    },
}

pub struct ScheduledFunction {
    function: Box<str>,
    bsatn_args: Vec<u8>,
}

#[derive(Clone)]
pub struct Scheduler {
    tx: mpsc::UnboundedSender<MsgOrExit<SchedulerMessage>>,
}

pub struct SchedulerStarter {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    db: Arc<RelationalDB>,
}

impl Scheduler {
    pub fn open(db: Arc<RelationalDB>) -> (Self, SchedulerStarter) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Scheduler { tx }, SchedulerStarter { rx, db })
    }
}

impl SchedulerStarter {
    // TODO(cloutiertyler): This whole start dance is scuffed, but I don't have
    // time to make it better right now.
    pub fn start(mut self, module_host: &ModuleHost) -> anyhow::Result<()> {
        let mut queue: DelayQueue<QueueItem> = DelayQueue::new();
        let mut key_map = FxHashMap::default();

        let tx = self.db.begin_tx(Workload::Internal);

        // Draining rx before processing schedules from the DB to ensure there are no in-flight messages,
        // as this can result in duplication.
        //
        // Explanation: By this time, if the `Scheduler::schedule` method has been called (the `init` reducer can do that),
        // there will be an in-flight message in tx that has already been inserted into the DB.
        // We are building the `queue` below with the DB and then spawning `SchedulerActor`, which will processes
        // the in-flight message, resulting in a duplicate entry in the queue.
        while self.rx.try_recv().is_ok() {}

        // Find all Scheduled tables
        for st_scheduled_row in self.db.iter(&tx, ST_SCHEDULED_ID)? {
            let table_id = st_scheduled_row.read_col(StScheduledFields::TableId)?;
            let (id_column, at_column) = self
                .db
                .table_scheduled_id_and_at(&tx, table_id)?
                .ok_or_else(|| anyhow!("scheduled table {table_id} doesn't have valid columns"))?;

            let now_ts = Timestamp::now();
            let now_instant = Instant::now();

            // Insert each entry (row) in the scheduled table into `queue`.
            for scheduled_row in self.db.iter(&tx, table_id)? {
                let (schedule_id, schedule_at) = get_schedule_from_row(&scheduled_row, id_column, at_column)?;
                // calculate duration left to call the scheduled reducer
                let duration = schedule_at.to_duration_from(now_ts);
                let at = schedule_at.to_timestamp_from(now_ts);
                let id = ScheduledFunctionId {
                    table_id,
                    schedule_id,
                    id_column,
                    at_column,
                };
                let key = queue.insert_at(QueueItem::Id { id, at }, now_instant + duration);

                // This should never happen as duplicate entries should be gated by unique
                // constraint violation in scheduled tables.
                if key_map.insert(id, key).is_some() {
                    return Err(anyhow!(
                        "Duplicate key found in scheduler queue: table_id {}, schedule_id {}",
                        id.table_id,
                        id.schedule_id
                    ));
                }
            }
        }

        tokio::spawn(
            SchedulerActor {
                rx: self.rx,
                queue,
                key_map,
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
const MAX_SCHEDULE_DELAY: Duration = Duration::from_millis(
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
    /// Schedule a reducer/procedure to run from a scheduled table.
    ///
    /// `fn_start` is the timestamp of the start of the current reducer/procedure.
    pub(super) fn schedule(
        &self,
        table_id: TableId,
        schedule_id: u64,
        schedule_at: ScheduleAt,
        id_column: ColId,
        at_column: ColId,
        fn_start: Timestamp,
    ) -> Result<(), ScheduleError> {
        // if `Timestamp::now()` is properly monotonic, use it; otherwise, use
        // the start of the reducer run as "now" for purposes of scheduling
        // TODO(procedure-tx): when we do `with_tx` in a procedure,
        // it inherits the timestamp of the procedure,
        // which could become a problem here for long running procedures.
        let now = fn_start.max(Timestamp::now());

        // Check that `at` is within `tokio_utils::time::DelayQueue`'s
        // accepted time-range.
        //
        // `DelayQueue` uses a sliding window, and there may be some non-zero
        // delay between this check and the actual call to `DelayQueue::insert_at`.
        //
        // Assuming a monotonic clock, this means we may reject some otherwise
        // acceptable schedule calls.
        let delay = schedule_at.to_duration_from(now);
        if delay >= MAX_SCHEDULE_DELAY {
            return Err(ScheduleError::DelayTooLong(delay));
        }
        let effective_at = schedule_at.to_timestamp_from(now);
        let real_at = Instant::now() + delay;

        // if the actor has exited, it's fine to ignore; it means that the host actor calling
        // schedule will exit soon as well, and it'll be scheduled to run when the module host restarts
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Schedule {
            id: ScheduledFunctionId {
                table_id,
                schedule_id,
                id_column,
                at_column,
            },
            effective_at,
            real_at,
        }));

        Ok(())
    }

    pub fn volatile_nonatomic_schedule_immediate(&self, function_name: String, args: FunctionArgs) {
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::ScheduleImmediate {
            function_name,
            args,
        }));
    }

    pub fn close(&self) {
        let _ = self.tx.send(MsgOrExit::Exit);
    }

    pub async fn closed(&self) {
        self.tx.closed().await
    }
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    queue: DelayQueue<QueueItem>,
    key_map: FxHashMap<ScheduledFunctionId, delay_queue::Key>,
    module_host: WeakModuleHost,
}

pub(crate) enum QueueItem {
    Id { id: ScheduledFunctionId, at: Timestamp },
    VolatileNonatomicImmediate { function_name: String, args: FunctionArgs },
}

#[cfg(target_pointer_width = "64")]
spacetimedb_table::static_assert_size!(QueueItem, 64);

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
            SchedulerMessage::Schedule {
                id,
                effective_at,
                real_at,
            } => {
                // Incase of row update, remove the existing entry from queue first
                if let Some(key) = self.key_map.get(&id) {
                    self.queue.remove(key);
                }
                let key = self.queue.insert_at(QueueItem::Id { id, at: effective_at }, real_at);
                self.key_map.insert(id, key);
            }
            SchedulerMessage::ScheduleImmediate { function_name, args } => {
                self.queue.insert(
                    QueueItem::VolatileNonatomicImmediate { function_name, args },
                    Duration::ZERO,
                );
            }
        }
    }

    async fn handle_queued(&mut self, id: Expired<QueueItem>) {
        let item = id.into_inner();
        let id: Option<ScheduledFunctionId> = match item {
            QueueItem::Id { id, .. } => Some(id),
            QueueItem::VolatileNonatomicImmediate { .. } => None,
        };
        if let Some(id) = id {
            self.key_map.remove(&id);
        }

        let Some(module_host) = self.module_host.upgrade() else {
            return;
        };

        // We need a mutable transaction
        // both to determine the function name and the scheduled row.
        let db = module_host.replica_ctx().relational_db.clone();
        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);

        // Determine the call params.
        // This also lets us know whether to call a reducer or procedure.
        let params = call_params_for_queued_item(&module_host.info, &db, &tx, item);
        let (timestamp, params) = match params {
            // If the function was already deleted,
            // leave the `ScheduledFunction` in the database for when the module restarts.
            Ok(None) => return,
            Ok(Some(params)) => params,
            Err(err) => {
                // All we can do here is log an error.
                log::error!("could not determine scheduled function or its parameters: {err:#}");

                if let Some(id) = id {
                    let _ = self
                        .delete_scheduled_function_row(&db, Some(tx), id, &module_host, Timestamp::now())
                        .await;
                }
                return;
            }
        };

        // We've determined whether we have a reducer or procedure.
        // The logic between them will now split,
        // as for scheduled procedures, it's incorrect to retry them if execution aborts midway,
        // so we must remove the schedule row before executing.
        match params {
            Either::Left(params) => {
                // Execute the reducer.
                let module_host_clone = module_host.clone();
                let res = tokio::spawn(async move {
                    module_host_clone
                        .call_reducer_with_params("scheduled_reducer", Some(tx), params)
                        .await
                })
                .await;

                log_scheduled_exec_error(&res);

                // If we didn't actually call the reducer, because the module exited,
                // leave the scheduled row in the database for when the module restarts.
                match res {
                    Ok(Err(NoSuchModule)) => {}
                    Ok(Ok(_)) | Err(_) => {
                        if let Some(id) = id {
                            let _ = self
                                .delete_scheduled_function_row(&db, None, id, &module_host, Timestamp::now())
                                .await;
                        }
                    }
                }
            }
            Either::Right(params) => {
                // Delete scheduled row.
                if let Some(id) = id {
                    let _ = self
                        .delete_scheduled_function_row(&db, Some(tx), id, &module_host, timestamp)
                        .await;
                }

                // Execute the procedure.
                let res = tokio::spawn(async move {
                    module_host
                        .call_procedure_with_params("scheduled_procedure", params)
                        .await
                })
                .await;

                log_scheduled_exec_error(&res);
            }
        }
    }

    async fn delete_scheduled_function_row(
        &mut self,
        db: &RelationalDB,
        tx: Option<MutTxId>,
        id: ScheduledFunctionId,
        module_host: &ModuleHost,
        ts: Timestamp,
    ) {
        let db = db.clone();
        let schedule_at = match tx {
            Some(tx) => Ok(delete_scheduled_function_row_with_tx(module_host, &db, tx, id)),
            None => {
                let host_clone = module_host.clone();
                module_host
                    .on_module_thread("delete_scheduled_function_row", move || {
                        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
                        delete_scheduled_function_row_with_tx(&host_clone, &db, tx, id)
                    })
                    .await
            }
        };

        // Log errors if needed and bail on error.
        let schedule_at = match schedule_at {
            Ok(sa) => sa,
            Err(err) => {
                let tid = id.table_id;
                let sid = id.schedule_id;
                log::error!("couldn't delete scheduled row; table id `{tid}`, schedule id `{sid}`: {err:#}");
                return;
            }
        };

        // If this was repeated, we need to add it back to the queue.
        if let Some(ScheduleAt::Interval(dur)) = schedule_at {
            let key = self.queue.insert(
                QueueItem::Id { id, at: ts + dur },
                dur.to_duration().unwrap_or(Duration::ZERO),
            );
            self.key_map.insert(id, key);
        }
    }
}

fn delete_scheduled_function_row_with_tx(
    module_host: &ModuleHost,
    db: &RelationalDB,
    mut tx: MutTxId,
    id: ScheduledFunctionId,
) -> Option<ScheduleAt> {
    if let Ok(Some(schedule_row)) = get_schedule_row_mut(&tx, db, id) {
        if let Ok(schedule_at) = read_schedule_at(&schedule_row, id.at_column) {
            // If the schedule is an interval, we handle it as a repeated schedule
            if let ScheduleAt::Interval(_) = schedule_at {
                return Some(schedule_at);
            }
            let row_ptr = schedule_row.pointer();
            db.delete(&mut tx, id.table_id, [row_ptr]);

            commit_and_broadcast_deletion_event(tx, module_host);
        } else {
            log::debug!(
                "Failed to read 'scheduled_at' from row: table_id {}, schedule_id {}",
                id.table_id,
                id.schedule_id
            );
        }
    }
    None
}

fn commit_and_broadcast_deletion_event(tx: MutTxId, module_host: &ModuleHost) {
    let info = module_host.info();

    let event = ModuleEvent {
        timestamp: Timestamp::now(),
        caller_identity: info.database_identity,
        caller_connection_id: None,
        function_call: ModuleFunctionCall::default(),
        status: EventStatus::Committed(DatabaseUpdate::default()),
        //Keeping them 0 as it is internal transaction, not by reducer
        energy_quanta_used: EnergyQuanta { quanta: 0 },
        host_execution_duration: Duration::from_millis(0),
        request_id: None,
        timer: None,
    };

    if let Err(e) = info.subscriptions.commit_and_broadcast_event(None, event, tx) {
        log::error!("Failed to broadcast deletion event: {e:#}");
    }
}

fn log_scheduled_exec_error<T>(res: &Result<Result<T, NoSuchModule>, JoinError>) {
    match res {
        Ok(Ok(_)) => {}
        Ok(Err(NoSuchModule)) => log::warn!("module exited while executing scheduled function"),
        Err(e) => log::error!("invoking scheduled function failed: {e:#}"),
    }
}

pub(crate) fn call_params_for_queued_item(
    module: &ModuleInfo,
    db: &RelationalDB,
    tx: &MutTxId,
    item: QueueItem,
) -> anyhow::Result<Option<(Timestamp, CallParams)>> {
    Ok(Some(match item {
        QueueItem::Id { id, at } => {
            let Some(schedule_row) = get_schedule_row_mut(tx, db, id)? else {
                // If the row is not found, it means the schedule is cancelled by the user.
                return Ok(None);
            };
            let ScheduledFunction { function, bsatn_args } = process_schedule(tx, db, id.table_id, &schedule_row)?;

            let fun_args = FunctionArgs::Bsatn(bsatn_args.into());
            function_to_call_params(module, &function, fun_args, Some(at))?
        }
        QueueItem::VolatileNonatomicImmediate { function_name, args } => {
            function_to_call_params(module, &function_name, args, None)?
        }
    }))
}

type CallParams = Either<CallReducerParams, CallProcedureParams>;

/// Finds the function for `name`
/// and returns the appropriate call parameters
/// to call the function with `args`.
fn function_to_call_params(
    module: &ModuleInfo,
    name: &str,
    args: FunctionArgs,
    at: Option<Timestamp>,
) -> anyhow::Result<(Timestamp, CallParams)> {
    let identity = module.database_identity;

    // Find the function and deserialize the arguments.
    let module = &module.module_def;
    let (id, args) = if let Some((id, def)) = module.reducer_full(name) {
        let args = args.into_tuple_for_def(module, def).map_err(InvalidReducerArguments)?;
        (Either::Left(id), args)
    } else if let Some((id, def)) = module.procedure_full(name) {
        let args = args
            .into_tuple_for_def(module, def)
            .map_err(InvalidProcedureArguments)?;
        (Either::Right(id), args)
    } else {
        // This should be impossible, but let's still return an error to log.
        return Err(anyhow!("Reducer or procedure `{name}` not found"));
    };

    // The timestamp we tell the function it's running at will be
    // at least the timestamp it was scheduled to run at.
    let now = Timestamp::now();
    let ts = at.unwrap_or(now).max(now);

    use Either::*;
    let params = match id {
        Left(id) => Left(CallReducerParams::from_system(ts, identity, id, args)),
        Right(id) => Right(CallProcedureParams::from_system(ts, identity, id, args)),
    };

    Ok((ts, params))
}

/// Generate [`ScheduledFunction`] for given [`ScheduledFunctionId`].
fn process_schedule(
    tx: &MutTxId,
    db: &RelationalDB,
    table_id: TableId,
    schedule_row: &RowRef<'_>,
) -> Result<ScheduledFunction, anyhow::Error> {
    // Get reducer name from `ST_SCHEDULED` table.
    let table_id_col = StScheduledFields::TableId.col_id();
    let function_name_col = StScheduledFields::ReducerName.col_id();
    let st_scheduled_row = db
        .iter_by_col_eq_mut(tx, ST_SCHEDULED_ID, table_id_col, &table_id.into())?
        .next()
        .ok_or_else(|| anyhow!("Scheduled table with id {table_id} entry does not exist in `st_scheduled`"))?;
    let function = st_scheduled_row.read_col::<Box<str>>(function_name_col)?;

    Ok(ScheduledFunction {
        function,
        bsatn_args: schedule_row.to_bsatn_vec()?,
    })
}

/// Returns the `schedule_row` for `id`.
fn get_schedule_row_mut<'a>(
    tx: &'a MutTxId,
    db: &'a RelationalDB,
    id: ScheduledFunctionId,
) -> anyhow::Result<Option<RowRef<'a>>> {
    Ok(db
        .iter_by_col_eq_mut(tx, id.table_id, id.id_column, &id.schedule_id.into())?
        .next())
}

/// Helper to get `schedule_id` and `schedule_at`
/// from `schedule_row` product value.
pub fn get_schedule_from_row(
    row: &RowRef<'_>,
    id_column: ColId,
    at_column: ColId,
) -> anyhow::Result<(u64, ScheduleAt)> {
    let schedule_id: u64 = row.read_col(id_column)?;
    let schedule_at = read_schedule_at(row, at_column)?;

    Ok((schedule_id, schedule_at))
}

fn read_schedule_at(row: &RowRef<'_>, at_column: ColId) -> anyhow::Result<ScheduleAt> {
    let schedule_at_av: AlgebraicValue = row.read_col(at_column)?;
    ScheduleAt::try_from(schedule_at_av).map_err(|e| anyhow!("Failed to convert 'scheduled_at' to ScheduleAt: {e:?}"))
}
