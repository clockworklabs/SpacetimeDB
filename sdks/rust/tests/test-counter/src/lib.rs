#![allow(clippy::disallowed_macros)]

use spacetimedb_data_structures::map::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

const TEST_TIMEOUT_SECS: u64 = 5 * 60;

#[derive(Default)]
struct TestCounterInner {
    /// Maps test names to their outcomes
    outcomes: HashMap<String, anyhow::Result<()>>,
    /// Set of tests which have started.
    registered: HashSet<String>,
}

pub struct TestCounter {
    inner: Mutex<TestCounterInner>,
    wait_until_done: Condvar,
}

impl Default for TestCounter {
    fn default() -> Self {
        TestCounter {
            inner: Mutex::new(TestCounterInner::default()),
            wait_until_done: Condvar::new(),
        }
    }
}

impl TestCounter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    #[must_use]
    pub fn add_test(
        self: &Arc<Self>,
        test_name: impl Into<String> + Clone + std::fmt::Display + Send + 'static,
    ) -> Box<dyn FnOnce(anyhow::Result<()>) + Send + 'static> {
        {
            let mut lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
            if !lock.registered.insert(test_name.clone().into()) {
                panic!("Duplicate test name: {test_name}");
            }
        }
        let dup = Arc::clone(self);

        Box::new(move |outcome| {
            let mut lock = dup.inner.lock().expect("TestCounterInner Mutex is poisoned");
            lock.outcomes.insert(test_name.into(), outcome);
            dup.wait_until_done.notify_all();
        })
    }

    pub async fn wait_for_all(&self) {
        // wasm/browser test clients run callbacks on a single-threaded event loop,
        // so waiting must be async to allow callback tasks to make progress.
        #[cfg(target_arch = "wasm32")]
        self.wait_for_all_wasm_async().await;

        #[cfg(not(target_arch = "wasm32"))]
        self.wait_for_all_native();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn wait_for_all_native(&self) {
        let lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
        let (lock, timeout_result) = self
            .wait_until_done
            .wait_timeout_while(lock, Duration::from_secs(TEST_TIMEOUT_SECS), |inner| {
                inner.outcomes.len() != inner.registered.len()
            })
            .expect("TestCounterInner Mutex is poisoned");
        if timeout_result.timed_out() {
            let mut timeout_count = 0;
            let mut failed_count = 0;
            for test in lock.registered.iter() {
                match lock.outcomes.get(test) {
                    None => {
                        timeout_count += 1;
                        println!("TIMEOUT: {test}");
                    }
                    Some(Err(e)) => {
                        failed_count += 1;
                        println!("FAILED:  {test}:\n\t{e:?}\n");
                    }
                    Some(Ok(())) => {
                        println!("PASSED:  {test}");
                    }
                }
            }
            panic!("{timeout_count} tests timed out and {failed_count} tests failed")
        } else {
            let mut failed_count = 0;
            for (test, outcome) in lock.outcomes.iter() {
                match outcome {
                    Ok(()) => println!("PASSED: {test}"),
                    Err(e) => {
                        failed_count += 1;
                        println!("FAILED: {test}:\n\t{e:?}\n");
                    }
                }
            }
            if failed_count != 0 {
                panic!("{failed_count} tests failed");
            } else {
                println!("All tests passed");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn wait_for_all_wasm_async(&self) {
        use gloo_timers::future::TimeoutFuture;

        const WAIT_INTERVAL_MS: u32 = 10;
        const MAX_WAIT_ITERATIONS: u32 = (TEST_TIMEOUT_SECS as u32 * 1000) / WAIT_INTERVAL_MS;

        // Native can block on a Condvar because callbacks keep moving on a different SDK thread.
        // wasm/browser does not have that escape hatch in this harness: the websocket/message loop and
        // the test body share the same single-threaded JS event loop, so blocking here would stop
        // callback delivery entirely. We poll with timer yields so websocket/callback tasks can
        // continue to run, and then do the same final pass native uses to convert recorded failures
        // into a panic.
        let all_tests_finished = || {
            let inner = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
            inner.outcomes.len() == inner.registered.len()
        };

        let mut finished = false;
        for _ in 0..MAX_WAIT_ITERATIONS {
            if all_tests_finished() {
                // We still need the final outcome pass below. Returning here would incorrectly
                // treat recorded `Err(...)` test outcomes as success, including harness tests that
                // intentionally exercise the failure path.
                finished = true;
                break;
            }
            TimeoutFuture::new(WAIT_INTERVAL_MS).await;
        }

        let lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
        if !finished || lock.outcomes.len() != lock.registered.len() {
            let mut timeout_count = 0;
            let mut failed_count = 0;
            for test in lock.registered.iter() {
                match lock.outcomes.get(test) {
                    None => {
                        timeout_count += 1;
                        println!("TIMEOUT: {test}");
                    }
                    Some(Err(e)) => {
                        failed_count += 1;
                        println!("FAILED:  {test}:\n\t{e:?}\n");
                    }
                    Some(Ok(())) => {
                        println!("PASSED:  {test}");
                    }
                }
            }
            panic!("{timeout_count} tests timed out and {failed_count} tests failed");
        } else {
            let mut failed_count = 0;
            for (test, outcome) in lock.outcomes.iter() {
                match outcome {
                    Ok(()) => println!("PASSED: {test}"),
                    Err(e) => {
                        failed_count += 1;
                        println!("FAILED: {test}:\n\t{e:?}\n");
                    }
                }
            }
            if failed_count != 0 {
                panic!("{failed_count} tests failed");
            } else {
                println!("All tests passed");
            }
        }
    }
}
