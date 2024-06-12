use std::sync::Arc;

use anyhow::{anyhow, Error};
use futures::StreamExt;
use rustc_hash::FxHashMap;
use sled::transaction::TransactionError;
use spacetimedb_lib::bsatn::ser::BsatnError;
use spacetimedb_lib::scheduler::ScheduleAt;
use spacetimedb_lib::{bsatn, AlgebraicValue, Timestamp};
use spacetimedb_primitives::TableId;
use spacetimedb_table::table::RowRef;
use tokio::sync::mpsc;
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue, DelayQueue};

use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::system_tables::{StScheduledRow, ST_SCHEDULED_ID};
use crate::db::datastore::traits::IsolationLevel;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::messages::control_worker_api::ScheduleUpdate;

use super::module_host::WeakModuleHost;
use super::{ModuleHost, ReducerArgs};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ScheduledReducerId {
    /// Scheduled Table id, this should have a entry in `ST_SCHEDULED`
    table_id: TableId,
    /// Refer to a particular schedule (row) in scheduled table
    scheduled_id: u64,
}

enum MsgOrExit<T> {
    Msg(T),
    Exit,
}

enum SchedulerMessage {
    Schedule { id: ScheduledReducerId, at: Timestamp },
    Cancel { id: ScheduledReducerId },
}

struct ScheduledReducer {
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
    //  pub fn dummy() -> Self {
    //      let (tx, _) = mpsc::unbounded_channel();
    //      let db = TestDB::durable().unwrap();
    //      Self { tx, db: Arc::new(*db) }
    //  }

    pub fn open(db: Arc<RelationalDB>) -> (Self, SchedulerStarter) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Scheduler { tx, db: db.clone() }, SchedulerStarter { rx, db })
    }

    pub fn new_with_same_db(&self) -> (Self, SchedulerStarter) {
        Self::open(self.db.clone())
    }

    pub fn clear(&self) {
        // self.db.clear().unwrap()
    }
}

struct StModuleScheduledRow {
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

impl TryFrom<RowRef<'_>> for StModuleScheduledRow {
    type Error = Error;
    fn try_from(row: RowRef<'_>) -> Result<Self, Self::Error> {
        Ok(StModuleScheduledRow {
            scheduled_id: row.read_col(0)?,
            scheduled_at: row
                .read_col::<AlgebraicValue>(1)?
                .try_into()
                .map_err(|_| anyhow!("Error reading scheduled_at"))?,
        })
    }
}

impl SchedulerStarter {
    // TODO(cloutiertyler): This whole start dance is scuffed, but I don't have
    // time to make it better right now.
    pub fn start(self, module_host: &ModuleHost) -> anyhow::Result<()> {
        let mut queue: DelayQueue<ScheduledReducerId> = DelayQueue::new();
        let ctx = &ExecutionContext::internal(self.db.address());
        let tx = self.db.begin_tx();
        // Find all Scheduled tables
        for row in self.db.iter(&ctx, &tx, ST_SCHEDULED_ID)? {
            let scheduled_table = StScheduledRow::try_from(row).expect("Error reading scheduled table row");

            // Insert each entry (row) of scheduled tables in DelayQueue
            for row_ref in self.db.iter(&ctx, &tx, scheduled_table.table_id)? {
                let scheduler: StModuleScheduledRow = row_ref.try_into()?;

                let at_time = match scheduler.scheduled_at {
                    ScheduleAt::Time(time) => time,
                    ScheduleAt::Interval(dur) => todo!(),
                };
                queue.insert(
                    ScheduledReducerId {
                        table_id: scheduled_table.table_id,
                        scheduled_id: scheduler.scheduled_id,
                    },
                    at_time.to_duration_from_now(),
                );
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
    DelayTooLong(Timestamp),

    #[error("Unable to generate a ScheduledReducerId: {0:?}")]
    IdTransactionError(#[from] TransactionError<BsatnError>),
}

impl Scheduler {
    pub fn schedule(&self, table_id: TableId, row_id: u64, at: Timestamp) -> Result<ScheduledReducerId, ScheduleError> {
        // Check that `at` is within `tokio_utils::time::DelayQueue`'s accepted time-range.
        //
        // `DelayQueue` uses a sliding window,
        // and there may be some non-zero delay between this check
        // and the actual call to `DelayQueue::insert`.
        //
        // Assuming a monotonic clock,
        // this means we may reject some otherwise acceptable schedule calls.
        //
        // If `Timestamp::to_duration_from_now` is not monotonic,
        // i.e. `std::time::SystemTime` is not monotonic,
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
        let delay = at.to_duration_from_now();
        if delay >= MAX_SCHEDULE_DELAY {
            return Err(ScheduleError::DelayTooLong(at));
        }

        let id = ScheduledReducerId {
            table_id,
            scheduled_id: row_id,
        };
        // if the actor has exited, it's fine to ignore; it means that the host actor calling
        // schedule will exit soon as well, and it'll be scheduled to run when the module host restarts
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Schedule { id, at }));
        Ok(id)
    }

    pub fn cancel(&self, id: ScheduledReducerId) {
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Cancel { id }));
    }

    pub fn close(&self) {
        let _ = self.tx.send(MsgOrExit::Exit);
    }
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    queue: DelayQueue<ScheduledReducerId>,
    key_map: FxHashMap<ScheduledReducerId, delay_queue::Key>,
    module_host: WeakModuleHost,
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
                let key = self.queue.insert(id, at.to_duration_from_now());
                self.key_map.insert(id, key);
            }
            SchedulerMessage::Cancel { id } => {
                if let Some(key) = self.key_map.remove(&id) {
                    self.queue.remove(&key);
                }
            }
        }
    }

    async fn handle_queued(&mut self, id: Expired<ScheduledReducerId>) {
        let id = id.into_inner();
        self.key_map.remove(&id);

        if let Some(module_host) = self.module_host.upgrade() {
            let db = module_host.dbic().relational_db.clone();
            let tx = db.begin_mut_tx(IsolationLevel::Serializable);
            let identity = module_host.info().identity;
            match self.proccess_schedule(id, &db, &tx) {
                Ok((reducer, is_repeated)) => {
                    tokio::spawn(async move {
                        let res = module_host
                            .call_reducer(
                                Some(tx),
                                identity,
                                None,
                                None,
                                None,
                                None,
                                &reducer.reducer,
                                ReducerArgs::Bsatn(reducer.bsatn_args.into()),
                            )
                            .await;

                        // Delete the entry from table if it is not repeated schedule in new tx
                        if !is_repeated {
                            let _ = delete_schedule_from_table(id, &db);
                        }
                        if let Err(e) = res {
                            log::error!("invoking scheduled reducer failed: {e:#}");
                        };
                    });
                }
                Err(e) => {
                    log::error!("invoking scheduled reducer failed: {e:#}");
                },
            }
        }
    }

    fn proccess_schedule(
        &mut self,
        id: ScheduledReducerId,
        db: &RelationalDB,
        tx: &MutTxId,
    ) -> Result<(ScheduledReducer, bool), anyhow::Error> {
        let ctx = ExecutionContext::internal(db.address());

        let scheduled_id = AlgebraicValue::from(id.scheduled_id);
        let table_id = id.table_id;

        // Find reducer name from `ST_SCHEDULED` table
        let reducer_entry = db
            .iter_by_col_eq_mut(&ctx, &tx, ST_SCHEDULED_ID, 0, &AlgebraicValue::from(table_id))?
            .next()
            .ok_or(anyhow!("scheduled table entry not exist in st_scheduled"))?;
        let reducer_name = reducer_entry.read_col::<Box<str>>(0)?;

        // Check if expired schedule exists in the relational_db
        let schedule = db
            .iter_by_col_eq_mut(&ctx, &tx, table_id, 0, &scheduled_id)?
            .next()
            .ok_or(anyhow!("scheduler not found in rdb"))?;
        let schedule_at = TryInto::<StModuleScheduledRow>::try_into(schedule)?.scheduled_at;

        let reducer_args_av = &schedule.to_product_value().elements[2..0];
        let reducer_arg_bsatn = bsatn::to_vec(&reducer_args_av)?;

        let mut is_repeated = false;
        if let ScheduleAt::Interval(dur) = schedule_at {
            is_repeated = true;
            self.queue.insert(
                ScheduledReducerId {
                    table_id,
                    scheduled_id: id.scheduled_id,
                },
                dur.into(),
            );
        }

        Ok((
            ScheduledReducer {
                reducer: reducer_name,
                bsatn_args: reducer_arg_bsatn,
            },
            is_repeated,
        ))
    }
}

fn delete_schedule_from_table(id: ScheduledReducerId, db: &RelationalDB) -> Result<(), anyhow::Error> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);
    let ctx = ExecutionContext::internal(db.address());

    let scheduled_id = AlgebraicValue::from(id.scheduled_id);
    let table_id = id.table_id;

    let schedule_row = db
        .iter_by_col_eq_mut(&ctx, &mut tx, table_id, 0, &scheduled_id)?
        .next()
        .ok_or(anyhow!("scheduler not found in rdb"))?;
    
    let schedule_at = TryInto::<StModuleScheduledRow>::try_into(schedule_row)?.scheduled_at;
    let row_ptr = schedule_row.pointer();

    if let ScheduleAt::Time(_) = schedule_at {
        db.delete(&mut tx, table_id, [row_ptr]);
    }
    tx.commit(&ctx);
    Ok(())
}
