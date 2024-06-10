use std::sync::Arc;

use futures::StreamExt;
use rustc_hash::FxHashMap;
use sled::transaction::{ConflictableTransactionError::Abort as TxAbort, TransactionError};
use spacetimedb_lib::bsatn::ser::BsatnError;
use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::scheduler::ScheduleAt;
use spacetimedb_lib::{bsatn, AlgebraicType, AlgebraicValue, ProductValue, SumType, Timestamp};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::algebraic_value::ser::ValueSerializer;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::SumValue;
use spacetimedb_table::layout::PrimitiveType;
use sqlparser::ast::Interval;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue, DelayQueue};

use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::datastore::system_tables::{StScheduledRow, ST_SCHEDULED_ID};
use crate::db::relational_db::RelationalDB;
use crate::execution_context::ExecutionContext;

use super::module_host::WeakModuleHost;
use super::{ModuleHost, ReducerArgs, ReducerCallError};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ScheduledReducerId {
    table_id: TableId,
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

#[derive(spacetimedb_sats::ser::Serialize, spacetimedb_sats::de::Deserialize)]
struct ScheduledReducer {
    at: Timestamp,
    reducer: String,
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

            // Insert each entry (row) in DelayQueue
            for row_ref in self.db.iter(&ctx, &tx, scheduled_table.table_id)? {
                // First two columns for schjedile table are fixed i.e `schedule_id` and
                // `ScheuleAt` respectivelty

                let scheduled_id: u64 = row_ref
                    .read_col::<u64>(1)
                    .map_err(|_| anyhow::anyhow!("Error reading scheduled_at"))?;

                let schedule_at: ScheduleAt = row_ref
                    .read_col::<AlgebraicValue>(2)
                    .map_err(|_| anyhow::anyhow!("Error reading scheduled_at"))?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Error reading scheduled_at"))?;

                let at_time = match schedule_at {
                    ScheduleAt::Time(time) => time,
                    ScheduleAt::Interval(dur) => todo!(),
                };
                queue.insert(
                    ScheduledReducerId {
                        table_id: scheduled_table.table_id,
                        scheduled_id,
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
    pub fn schedule(
        &self,
        reducer: String,
        bsatn_args: Vec<u8>,
        at: Timestamp,
    ) -> Result<ScheduledReducerId, ScheduleError> {
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

        let reducer = ScheduledReducer {
            at,
            reducer,
            bsatn_args,
        };
        let id = ScheduledReducerId {
            table_id: TableId::default(),
            scheduled_id: 0,
        };
        // if the actor has exited, it's fine to ignore; it means that the host actor calling
        // schedule will exit soon as well, and it'll be scheduled to run when the module host restarts
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Schedule { id, at }));
        Ok(id)
    }

    pub fn cancel(&self, id: ScheduledReducerId) {
        // let res = self.db.transaction(|tx| {
        //     tx.remove(&id.0.to_le_bytes())?;
        //     Ok(())
        // });
        // match res {
        //     Ok(()) => {
        //         // if it's exited it's not gonna run it :) see also the comment in schedule()
        //         let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Cancel { id }));
        //     }
        //     // we could return an error here, but that would give them information that
        //     // there exists a scheduled reducer with this id. like returning a HTTP 400
        //     // instead of a 404
        //     Err(TransactionError::Abort(())) => {}
        //     Err(TransactionError::Storage(e)) => panic!("sled error: {e:?}"),
        // }
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
        // let id = id.into_inner();
        // self.key_map.remove(&id);
        // let Some(module_host) = self.module_host.upgrade() else {
        //     return;
        // };
        // let Some(scheduled) = self.db.get(id.0.to_le_bytes()).unwrap() else {
        //     return;
        // };
        // let scheduled: ScheduledReducer = bsatn::from_slice(&scheduled).unwrap();

        // let db = self.db.clone();
        // tokio::spawn(async move {
        //     let info = module_host.info();
        //     let identity = info.identity;
        //     // TODO: pass a logical "now" timestamp to this reducer call, but there's some
        //     //       intricacies to get right (how much drift to tolerate? what kind of tokio::time::MissedTickBehavior do we want?)
        //     let res = module_host
        //         .call_reducer(
        //             identity,
        //             // Scheduled reducers take `None` as the caller address.
        //             None,
        //             None,
        //             None,
        //             None,
        //             &scheduled.reducer,
        //             ReducerArgs::Bsatn(scheduled.bsatn_args.into()),
        //         )
        //         .await;
        //     if !matches!(res, Err(ReducerCallError::NoSuchModule(_))) {
        //         // if we didn't actually call the reducer because the module exited, leave
        //         // the ScheduledReducer in the database for when the module restarts
        //         let _ = db.remove(id.0.to_le_bytes());
        //     }
        //     match res {
        //         Ok(_) => {}
        //         Err(e) => log::error!("invoking scheduled reducer failed: {e:#}"),
        //     }
        // });
    }
}
