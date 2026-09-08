//! Direct cross-thread termination, without executing a V8 interrupt callback.
//!
//! Pinned v8 145's IsolateHandle is Send+Sync and protects termination with its
//! annex mutex. No scope, raw isolate pointer, Rc or Cell leaves the isolate
//! thread. One persistent timer serves bounded active registrations. Finishing
//! or dropping a guard waits out any in-flight termination before the isolate
//! may reset termination or begin another invocation.

use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
#[error("JavaScript execution exceeded its wall-clock limit")]
pub(super) struct ExecutionTimedOut;

const MAX_ACTIVE_DEADLINES: usize = 16_384;
static ACTIVE_DEADLINES: AtomicUsize = AtomicUsize::new(0);
type Key = (Instant, u64);
static SERVICE: OnceLock<Result<Arc<DeadlineService>, String>> = OnceLock::new();

struct Capacity;
impl Capacity {
    fn acquire() -> io::Result<Self> {
        ACTIVE_DEADLINES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                (count < MAX_ACTIVE_DEADLINES).then_some(count + 1)
            })
            .map_err(|_| io::Error::other("JavaScript execution deadline capacity exhausted"))?;
        Ok(Self)
    }
}
impl Drop for Capacity {
    fn drop(&mut self) {
        ACTIVE_DEADLINES.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Registration {
    expires: Instant,
    state: Mutex<State>,
}
struct State {
    finished: bool,
    expired: bool,
    handle: v8::IsolateHandle,
}
#[derive(Default)]
struct Queue {
    sequence: u64,
    entries: BTreeMap<Key, Arc<Registration>>,
}
struct DeadlineService {
    queue: Mutex<Queue>,
    wake: Condvar,
}

impl DeadlineService {
    fn start() -> io::Result<Arc<Self>> {
        let service = Arc::new(Self {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
        });
        std::thread::Builder::new().name("v8-deadlines".into()).spawn({
            let service = service.clone();
            move || service.run()
        })?;
        Ok(service)
    }

    fn run(&self) {
        loop {
            let registration = {
                let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
                loop {
                    let Some((&(expires, _), _)) = queue.entries.first_key_value() else {
                        queue = self.wake.wait(queue).unwrap_or_else(|error| error.into_inner());
                        continue;
                    };
                    let remaining = expires.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break queue.entries.pop_first().unwrap().1;
                    }
                    queue = self
                        .wake
                        .wait_timeout(queue, remaining)
                        .unwrap_or_else(|error| error.into_inner())
                        .0;
                }
            };
            // Never hold the queue lock while acquiring a registration lock.
            // Cancellation may hold this lock while removing its queued entry.
            let mut state = registration.state.lock().unwrap_or_else(|error| error.into_inner());
            if !state.finished {
                state.expired = true;
                // finish() must acquire this same lock before resetting V8.
                state.handle.terminate_execution();
            }
        }
    }
}

pub(super) struct ExecutionDeadline {
    service: Arc<DeadlineService>,
    registration: Arc<Registration>,
    key: Key,
    finished: bool,
    // Includes expired calls still returning from a native syscall. The timer
    // popping its queue entry never releases this active-call reservation.
    _capacity: Capacity,
}

impl ExecutionDeadline {
    pub fn start(handle: v8::IsolateHandle, timeout: Duration) -> io::Result<Self> {
        let capacity = Capacity::acquire()?;
        let service = SERVICE
            .get_or_init(|| DeadlineService::start().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|error| io::Error::other(error.clone()))?
            .clone();
        let expires = Instant::now() + timeout;
        let registration = Arc::new(Registration {
            expires,
            state: Mutex::new(State {
                finished: false,
                expired: false,
                handle,
            }),
        });
        let mut queue = service.queue.lock().unwrap_or_else(|error| error.into_inner());
        queue.sequence = queue
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("JavaScript execution deadline sequence exhausted"))?;
        let key = (expires, queue.sequence);
        queue.entries.insert(key, registration.clone());
        drop(queue);
        service.wake.notify_one();
        Ok(Self {
            service,
            registration,
            key,
            finished: false,
            _capacity: capacity,
        })
    }

    pub fn finish(mut self) -> bool {
        let expired = self.cancel();
        self.finished = true;
        expired
    }

    fn cancel(&self) -> bool {
        let mut state = self
            .registration
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !state.finished {
            // A timer delayed by OS scheduling must still make a late return
            // roll back, even if it has not issued its termination request yet.
            state.expired |= Instant::now() >= self.registration.expires;
            state.finished = true;
        }
        let expired = state.expired;
        // The timer releases the queue lock before taking this lock, so this
        // order cannot invert its locks. Remove completed calls immediately,
        // rather than accumulating their registrations for 120 seconds.
        self.service
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .remove(&self.key);
        drop(state);
        self.service.wake.notify_one();
        expired
    }
}

impl Drop for ExecutionDeadline {
    fn drop(&mut self) {
        if !self.finished {
            self.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::v8::to_value::test::with_scope;

    fn run(scope: &mut v8::PinScope<'_, '_>, source: &str) -> bool {
        let source = v8::String::new(scope, source).unwrap();
        v8::Script::compile(scope, source, None).unwrap().run(scope).is_some()
    }

    #[test]
    fn deadline_terminates_and_isolate_can_run_again() {
        with_scope(|scope| {
            let deadline = ExecutionDeadline::start(scope.thread_safe_handle(), Duration::from_millis(40)).unwrap();
            assert!(!run(scope, "for (;;) {}"));
            assert!(deadline.finish());
            scope.cancel_terminate_execution();
            assert!(run(scope, "1 + 1"));
        });
    }

    #[test]
    fn finished_and_dropped_registrations_cannot_terminate_later_execution() {
        with_scope(|scope| {
            for drop_guard in [false, true] {
                let deadline = ExecutionDeadline::start(scope.thread_safe_handle(), Duration::from_millis(40)).unwrap();
                let service = deadline.service.clone();
                let key = deadline.key;
                if drop_guard {
                    drop(deadline);
                } else {
                    assert!(!deadline.finish());
                }
                assert!(!service.queue.lock().unwrap().entries.contains_key(&key));
                assert!(run(
                    scope,
                    "{ const end = Date.now() + 80; while (Date.now() < end) {} }"
                ));
            }
            for _ in 0..1_000 {
                let deadline = ExecutionDeadline::start(scope.thread_safe_handle(), Duration::from_secs(1)).unwrap();
                let service = deadline.service.clone();
                let key = deadline.key;
                assert!(run(scope, "1 + 1"));
                assert!(!deadline.finish());
                assert!(!service.queue.lock().unwrap().entries.contains_key(&key));
            }
        });
    }

    #[test]
    fn late_return_is_expired_even_before_timer_observes_it() {
        with_scope(|scope| {
            // A service without a timer deterministically models OS delay.
            let expires = Instant::now();
            let deadline = ExecutionDeadline {
                service: Arc::new(DeadlineService {
                    queue: Mutex::new(Queue::default()),
                    wake: Condvar::new(),
                }),
                registration: Arc::new(Registration {
                    expires,
                    state: Mutex::new(State {
                        finished: false,
                        expired: false,
                        handle: scope.thread_safe_handle(),
                    }),
                }),
                key: (expires, 1),
                finished: false,
                _capacity: Capacity::acquire().unwrap(),
            };
            assert!(deadline.finish());
            assert!(run(scope, "1 + 1"));
        });
    }
}
