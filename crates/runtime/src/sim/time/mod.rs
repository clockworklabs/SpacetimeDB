//! Virtual time for the local simulation runtime.

mod sleep;

use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::{fmt, future::Future, task::Waker, time::Duration};

use futures_util::{select_biased, FutureExt};
use sleep::wake_all;
use spin::Mutex;

pub use sleep::Sleep;

/// Shared virtual clock and timer registry for one simulation runtime.
///
/// All cloned handles observe the same virtual `now`, pending timers, and
/// timer-id sequence. The executor uses this handle both for explicit
/// time-travel operations and for jumping directly to the next pending timer
/// when the runnable queue is empty.
#[derive(Clone, Debug)]
pub struct TimeHandle {
    inner: Arc<Mutex<TimeState>>,
}

impl TimeHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TimeState::default())),
        }
    }

    pub fn now(&self) -> Duration {
        self.inner.lock().now
    }

    /// Move virtual time forward by an explicit amount.
    ///
    /// This is the direct "advance the clock" operation used by tests and
    /// higher-level simulation code. It updates `now`, removes any timers that
    /// became due at the new instant, and wakes the corresponding tasks after
    /// releasing the lock.
    pub fn advance(&self, duration: Duration) {
        if duration.is_zero() {
            return;
        }

        let wakers = {
            let mut state = self.inner.lock();
            state.now = state.now.saturating_add(duration);
            state.take_due_wakers()
        };
        wake_all(wakers);
    }

    /// Jump virtual time to the earliest outstanding timer and wake it.
    ///
    /// The executor calls this when there are no runnable tasks left. Instead
    /// of incrementing time in wall-clock steps, simulation time jumps
    /// directly to the minimum timer deadline. Returns `false` if there are no
    /// timers to wake.
    pub fn wake_next_timer(&self) -> bool {
        let wakers = {
            let mut state = self.inner.lock();
            let Some(next_deadline) = state.timers.values().map(|timer| timer.deadline).min() else {
                return false;
            };
            if next_deadline > state.now {
                state.now = next_deadline;
            }
            state.take_due_wakers()
        };
        let woke = !wakers.is_empty();
        wake_all(wakers);
        woke
    }

    /// Register or refresh a timer entry for a sleeping future.
    ///
    /// Sleep futures keep a stable `TimerId` across polls. Re-registering with
    /// the same id updates the stored waker without creating duplicate timers.
    fn register_timer(&self, id: TimerId, deadline: Duration, waker: &Waker) {
        let mut state = self.inner.lock();
        state.timers.insert(
            id,
            TimerEntry {
                deadline,
                waker: waker.clone(),
            },
        );
    }

    /// Remove a timer entry if it is still present.
    ///
    /// Cancellation is best-effort because the timer may already have been
    /// removed by a wakeup path before the caller reaches this point.
    fn cancel_timer(&self, id: TimerId) {
        self.inner.lock().timers.remove(&id);
    }

    /// Allocate a fresh timer id for a new sleep future.
    ///
    /// Stable timer ids are what let a `Sleep` future re-register itself
    /// across polls while still mapping back to a single timer entry.
    fn next_timer_id(&self) -> TimerId {
        let mut state = self.inner.lock();
        let id = TimerId(state.next_timer_id);
        state.next_timer_id = state.next_timer_id.saturating_add(1);
        id
    }

    /// Create a future that becomes ready after `duration` of virtual time.
    ///
    /// The returned future is lazy: it does not allocate a timer entry until
    /// the first poll, when it can anchor its deadline to the current virtual
    /// time.
    pub fn sleep(&self, duration: Duration) -> Sleep {
        Sleep::new(self.clone(), duration)
    }

    /// Race a future against a virtual-time sleep.
    ///
    /// This is implemented as `future` versus `sleep(duration)` using a biased
    /// select. If both become ready in the same simulated step, the main
    /// future wins the tie so completion beats timeout deterministically.
    pub async fn timeout<T>(&self, duration: Duration, future: impl Future<Output = T>) -> Result<T, TimeoutElapsed> {
        let sleep = self.sleep(duration);
        futures::pin_mut!(future);
        futures::pin_mut!(sleep);

        select_biased! {
            output = future.fuse() => Ok(output),
            () = sleep.fuse() => Err(TimeoutElapsed { duration }),
        }
    }
}

impl Default for TimeHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Mutable state behind a [`TimeHandle`].
///
/// `timers` is keyed by stable `TimerId` so a `Sleep` future can refresh its
/// waker across polls without accumulating duplicate entries. A `BTreeMap` is
/// used to keep due-timer iteration deterministic.
#[derive(Debug, Default)]
struct TimeState {
    now: Duration,
    next_timer_id: u64,
    timers: BTreeMap<TimerId, TimerEntry>,
}

impl TimeState {
    /// Remove every timer whose deadline is at or before the current virtual
    /// time and return their wakers.
    fn take_due_wakers(&mut self) -> Vec<Waker> {
        let due = self
            .timers
            .iter()
            .filter_map(|(id, timer)| (timer.deadline <= self.now).then_some(*id))
            .collect::<Vec<_>>();
        due.into_iter()
            .filter_map(|id| self.timers.remove(&id).map(|timer| timer.waker))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct TimerId(u64);

/// Stored metadata for one pending timer.
#[derive(Debug)]
struct TimerEntry {
    deadline: Duration,
    waker: Waker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeoutElapsed {
    duration: Duration,
}

impl TimeoutElapsed {
    pub fn duration(self) -> Duration {
        self.duration
    }
}

impl fmt::Display for TimeoutElapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "simulated timeout elapsed after {:?}", self.duration)
    }
}

#[cfg(any(feature = "tokio", feature = "simulation"))]
impl std::error::Error for TimeoutElapsed {}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::sim;
    use spin::Mutex;

    #[test]
    fn sleep_fast_forwards_virtual_time() {
        let mut runtime = sim::Runtime::new(101);
        let handle = runtime.handle();

        runtime.block_on(async move {
            assert_eq!(handle.now(), Duration::ZERO);
            handle.sleep(Duration::from_millis(5)).await;
            assert_eq!(handle.now(), Duration::from_millis(5));
        });
    }

    #[test]
    fn shorter_timer_wakes_first() {
        let mut runtime = sim::Runtime::new(102);
        let handle = runtime.handle();
        let order = Arc::new(Mutex::new(Vec::new()));

        runtime.block_on({
            let order = Arc::clone(&order);
            async move {
                let slow_order = Arc::clone(&order);
                let slow_handle = handle.clone();
                let slow = handle.spawn_on(sim::NodeId::MAIN, async move {
                    slow_handle.sleep(Duration::from_millis(10)).await;
                    slow_order.lock().push(10);
                });

                let fast_order = Arc::clone(&order);
                let fast_handle = handle.clone();
                let fast = handle.spawn_on(sim::NodeId::MAIN, async move {
                    fast_handle.sleep(Duration::from_millis(3)).await;
                    fast_order.lock().push(3);
                });

                fast.await.expect("fast timer task should complete");
                slow.await.expect("slow timer task should complete");
            }
        });

        assert_eq!(*order.lock(), vec![3, 10]);
        assert_eq!(runtime.elapsed(), Duration::from_millis(10));
    }

    #[test]
    fn explicit_advance_moves_virtual_time() {
        let mut runtime = sim::Runtime::new(103);
        let handle = runtime.handle();

        runtime.block_on(async move {
            handle.advance(Duration::from_millis(7));
            assert_eq!(handle.now(), Duration::from_millis(7));
        });
    }

    #[test]
    fn timeout_returns_future_output_before_deadline() {
        let mut runtime = sim::Runtime::new(104);
        let handle = runtime.handle();

        let output = runtime.block_on(async move {
            handle
                .timeout(Duration::from_millis(10), async {
                    handle.sleep(Duration::from_millis(3)).await;
                    9
                })
                .await
        });

        assert_eq!(output, Ok(9));
        assert_eq!(runtime.elapsed(), Duration::from_millis(3));
    }

    #[test]
    fn timeout_expires_at_virtual_deadline() {
        let mut runtime = sim::Runtime::new(105);
        let handle = runtime.handle();

        let output = runtime.block_on(async move {
            handle
                .timeout(Duration::from_millis(4), async {
                    handle.sleep(Duration::from_millis(20)).await;
                    9
                })
                .await
        });

        assert_eq!(output.unwrap_err().duration(), Duration::from_millis(4));
        assert_eq!(runtime.elapsed(), Duration::from_millis(4));
    }
}
