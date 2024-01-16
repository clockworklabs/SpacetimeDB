use std::path::Path;

use futures::StreamExt;
use rustc_hash::FxHashMap;
use sled::transaction::{ConflictableTransactionError::Abort as TxAbort, TransactionError};
use spacetimedb_lib::bsatn;
use spacetimedb_lib::bsatn::ser::BsatnError;
use tokio::sync::mpsc;
use tokio_util::time::delay_queue::Expired;
use tokio_util::time::{delay_queue, DelayQueue};

use crate::worker_metrics::{MAX_REDUCER_DELAY, WORKER_METRICS};

use super::module_host::WeakModuleHost;
use super::{ModuleHost, ReducerArgs, ReducerCallError, Timestamp};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct ScheduledReducerId(pub u64);

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
    db: sled::Db,
}

pub struct SchedulerStarter {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    db: sled::Db,
}

impl Scheduler {
    pub fn dummy(dummy_path: &Path) -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        let db = sled::open(dummy_path).unwrap();
        Self { tx, db }
    }

    pub fn open(scheduler_db_path: impl AsRef<Path>) -> anyhow::Result<(Self, SchedulerStarter)> {
        let db = sled::Config::default()
            .path(scheduler_db_path)
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput)
            .open()?;

        Ok(Self::from_db(db))
    }

    fn from_db(db: sled::Db) -> (Self, SchedulerStarter) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Scheduler { tx, db: db.clone() }, SchedulerStarter { rx, db })
    }

    pub fn new_with_same_db(&self) -> (Self, SchedulerStarter) {
        Self::from_db(self.db.clone())
    }

    pub fn clear(&self) {
        self.db.clear().unwrap()
    }
}

impl SchedulerStarter {
    // TODO(cloutiertyler): This whole start dance is scuffed, but I don't have
    // time to make it better right now.
    pub fn start(self, module_host: &ModuleHost) -> anyhow::Result<()> {
        let mut queue = DelayQueue::new();

        for entry in self.db.iter() {
            let (k, v) = entry?;
            let get_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
            let id = ScheduledReducerId(get_u64(&k));
            let at = Timestamp(get_u64(&v[..8]));
            queue.insert(id, at.to_duration_from_now());
        }

        tokio::spawn(
            SchedulerActor {
                rx: self.rx,
                queue,
                key_map: FxHashMap::default(),
                db: self.db,
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

        let id = self.db.transaction(|tx| {
            let id = tx.generate_id()?;
            let reducer = bsatn::to_vec(&reducer).map_err(TxAbort)?;
            tx.insert(&id.to_le_bytes(), reducer)?;
            Ok(ScheduledReducerId(id))
        })?;

        // if the actor has exited, it's fine to ignore; it means that the host actor calling
        // schedule will exit soon as well, and it'll be scheduled to run when the module host restarts
        let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Schedule { id, at }));
        Ok(id)
    }

    pub fn cancel(&self, id: ScheduledReducerId) {
        let res = self.db.transaction(|tx| {
            tx.remove(&id.0.to_le_bytes())?;
            Ok(())
        });
        match res {
            Ok(()) => {
                // if it's exited it's not gonna run it :) see also the comment in schedule()
                let _ = self.tx.send(MsgOrExit::Msg(SchedulerMessage::Cancel { id }));
            }
            // we could return an error here, but that would give them information that
            // there exists a scheduled reducer with this id. like returning a HTTP 400
            // instead of a 404
            Err(TransactionError::Abort(())) => {}
            Err(TransactionError::Storage(e)) => panic!("sled error: {e:?}"),
        }
    }

    pub fn close(&self) {
        let _ = self.tx.send(MsgOrExit::Exit);
    }
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<MsgOrExit<SchedulerMessage>>,
    queue: DelayQueue<ScheduledReducerId>,
    key_map: FxHashMap<ScheduledReducerId, delay_queue::Key>,
    db: sled::Db,
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
        let delay = id.deadline().elapsed().as_secs_f64();
        let id = id.into_inner();
        self.key_map.remove(&id);
        let Some(module_host) = self.module_host.upgrade() else {
            return;
        };
        let Some(scheduled) = self.db.get(id.0.to_le_bytes()).unwrap() else {
            return;
        };
        let scheduled: ScheduledReducer = bsatn::from_slice(&scheduled).unwrap();

        let db = module_host.info().address;
        let reducer = scheduled.reducer.clone();
        let mut guard = MAX_REDUCER_DELAY.lock().unwrap();
        let max_reducer_delay = *guard
            .entry((db, reducer))
            .and_modify(|max| {
                if delay > *max {
                    *max = delay;
                }
            })
            .or_insert_with(|| delay);

        // Note, we are only tracking the time a reducer spends delayed in the queue.
        // This does not account for any time the executing thread spends blocked by the os.
        WORKER_METRICS
            .scheduled_reducer_delay_sec
            .with_label_values(&db, &scheduled.reducer)
            .observe(delay);
        WORKER_METRICS
            .scheduled_reducer_delay_sec_max
            .with_label_values(&db, &scheduled.reducer)
            .set(max_reducer_delay);
        drop(guard);

        let db = self.db.clone();
        tokio::spawn(async move {
            let info = module_host.info();
            let identity = info.identity;
            // TODO: pass a logical "now" timestamp to this reducer call, but there's some
            //       intricacies to get right (how much drift to tolerate? what kind of tokio::time::MissedTickBehavior do we want?)
            let res = module_host
                .call_reducer(
                    identity,
                    // Scheduled reducers take `None` as the caller address.
                    None,
                    None,
                    &scheduled.reducer,
                    ReducerArgs::Bsatn(scheduled.bsatn_args.into()),
                )
                .await;
            if !matches!(res, Err(ReducerCallError::NoSuchModule(_))) {
                // if we didn't actually call the reducer because the module exited, leave
                // the ScheduledReducer in the database for when the module restarts
                let _ = db.remove(id.0.to_le_bytes());
            }
            match res {
                Ok(_) => {}
                Err(e) => log::error!("invoking scheduled reducer failed: {e:#}"),
            }
        });
    }
}
