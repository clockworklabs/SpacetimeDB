//! Virtual time for the local simulation runtime.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::Duration,
};

use futures::future::{select, Either};

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
        self.inner.lock().expect("sim time poisoned").now
    }

    pub fn advance(&self, duration: Duration) {
        if duration.is_zero() {
            return;
        }

        let wakers = {
            let mut state = self.inner.lock().expect("sim time poisoned");
            state.now = state.now.saturating_add(duration);
            state.take_due_wakers()
        };
        wake_all(wakers);
    }

    pub fn wake_next_timer(&self) -> bool {
        let wakers = {
            let mut state = self.inner.lock().expect("sim time poisoned");
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

    fn register_timer(&self, id: TimerId, deadline: Duration, waker: &Waker) {
        let mut state = self.inner.lock().expect("sim time poisoned");
        state.timers.insert(
            id,
            TimerEntry {
                deadline,
                waker: waker.clone(),
            },
        );
    }

    fn cancel_timer(&self, id: TimerId) {
        self.inner.lock().expect("sim time poisoned").timers.remove(&id);
    }

    fn next_timer_id(&self) -> TimerId {
        let mut state = self.inner.lock().expect("sim time poisoned");
        let id = TimerId(state.next_timer_id);
        state.next_timer_id = state.next_timer_id.saturating_add(1);
        id
    }
}

impl Default for TimeHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct TimeState {
    now: Duration,
    next_timer_id: u64,
    timers: BTreeMap<TimerId, TimerEntry>,
}

impl TimeState {
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

#[derive(Debug)]
struct TimerEntry {
    deadline: Duration,
    waker: Waker,
}

thread_local! {
    static CURRENT_TIME: RefCell<Option<TimeHandle>> = const { RefCell::new(None) };
}

pub struct TimeContextGuard {
    previous: Option<TimeHandle>,
}

pub fn enter_time_context(handle: TimeHandle) -> TimeContextGuard {
    let previous = CURRENT_TIME.with(|current| current.replace(Some(handle)));
    TimeContextGuard { previous }
}

pub fn try_current_handle() -> Option<TimeHandle> {
    CURRENT_TIME.with(|current| current.borrow().clone())
}

pub fn now() -> Duration {
    try_current_handle().map(|handle| handle.now()).unwrap_or_default()
}

pub fn advance(duration: Duration) {
    if let Some(handle) = try_current_handle() {
        handle.advance(duration);
    }
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        duration,
        state: SleepState::Unregistered,
    }
}

pub async fn timeout<T>(duration: Duration, future: impl Future<Output = T>) -> Result<T, TimeoutElapsed> {
    futures::pin_mut!(future);
    let sleep = sleep(duration);
    futures::pin_mut!(sleep);

    match select(future, sleep).await {
        Either::Left((output, _)) => Ok(output),
        Either::Right(((), _)) => Err(TimeoutElapsed { duration }),
    }
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

impl std::error::Error for TimeoutElapsed {}

impl Drop for TimeContextGuard {
    fn drop(&mut self) {
        CURRENT_TIME.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

pub struct Sleep {
    duration: Duration,
    state: SleepState,
}

enum SleepState {
    Unregistered,
    Registered {
        handle: TimeHandle,
        id: TimerId,
        deadline: Duration,
    },
    Done,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if matches!(self.state, SleepState::Done) {
            return Poll::Ready(());
        }

        if matches!(self.state, SleepState::Unregistered) {
            let handle = try_current_handle().expect("sim::time::sleep polled outside sim runtime");
            let deadline = handle.now().saturating_add(self.duration);
            let id = handle.next_timer_id();
            self.state = SleepState::Registered { handle, id, deadline };
        }

        let SleepState::Registered { handle, id, deadline } = &self.state else {
            unreachable!("sleep state should be registered or done");
        };

        if handle.now() >= *deadline {
            let handle = handle.clone();
            let id = *id;
            handle.cancel_timer(id);
            self.state = SleepState::Done;
            Poll::Ready(())
        } else {
            handle.register_timer(*id, *deadline, cx.waker());
            Poll::Pending
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let SleepState::Registered { handle, id, .. } = &self.state {
            handle.cancel_timer(*id);
        }
    }
}

fn wake_all(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::sim;

    #[test]
    fn sleep_fast_forwards_virtual_time() {
        let mut runtime = sim::Runtime::new(101).unwrap();

        runtime.block_on(async {
            assert_eq!(super::now(), Duration::ZERO);
            super::sleep(Duration::from_millis(5)).await;
            assert_eq!(super::now(), Duration::from_millis(5));
        });
    }

    #[test]
    fn shorter_timer_wakes_first() {
        let mut runtime = sim::Runtime::new(102).unwrap();
        let handle = runtime.handle();
        let order = Arc::new(Mutex::new(Vec::new()));

        runtime.block_on({
            let order = Arc::clone(&order);
            async move {
                let slow_order = Arc::clone(&order);
                let slow = handle.spawn_on(sim::NodeId::MAIN, async move {
                    super::sleep(Duration::from_millis(10)).await;
                    slow_order.lock().expect("order poisoned").push(10);
                });

                let fast_order = Arc::clone(&order);
                let fast = handle.spawn_on(sim::NodeId::MAIN, async move {
                    super::sleep(Duration::from_millis(3)).await;
                    fast_order.lock().expect("order poisoned").push(3);
                });

                fast.await;
                slow.await;
            }
        });

        assert_eq!(*order.lock().expect("order poisoned"), vec![3, 10]);
        assert_eq!(runtime.elapsed(), Duration::from_millis(10));
    }

    #[test]
    fn explicit_advance_moves_virtual_time() {
        let mut runtime = sim::Runtime::new(103).unwrap();

        runtime.block_on(async {
            super::advance(Duration::from_millis(7));
            assert_eq!(super::now(), Duration::from_millis(7));
        });
    }

    #[test]
    fn timeout_returns_future_output_before_deadline() {
        let mut runtime = sim::Runtime::new(104).unwrap();

        let output = runtime.block_on(async {
            super::timeout(Duration::from_millis(10), async {
                super::sleep(Duration::from_millis(3)).await;
                9
            })
            .await
        });

        assert_eq!(output, Ok(9));
        assert_eq!(runtime.elapsed(), Duration::from_millis(3));
    }

    #[test]
    fn timeout_expires_at_virtual_deadline() {
        let mut runtime = sim::Runtime::new(105).unwrap();

        let output = runtime.block_on(async {
            super::timeout(Duration::from_millis(4), async {
                super::sleep(Duration::from_millis(20)).await;
                9
            })
            .await
        });

        assert_eq!(output.unwrap_err().duration(), Duration::from_millis(4));
        assert_eq!(runtime.elapsed(), Duration::from_millis(4));
    }
}
