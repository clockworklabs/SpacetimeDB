#![allow(clippy::disallowed_macros)]
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

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
                panic!("Duplicate test name: {}", test_name);
            }
        }
        let dup = Arc::clone(self);

        Box::new(move |outcome| {
            let mut lock = dup.inner.lock().expect("TestCounterInner Mutex is poisoned");
            lock.outcomes.insert(test_name.into(), outcome);
            dup.wait_until_done.notify_all();
        })
    }

    pub fn wait_for_all(&self) {
        let lock = self.inner.lock().expect("TestCounterInner Mutex is poisoned");
        let (lock, timeout_result) = self
            .wait_until_done
            .wait_timeout_while(lock, Duration::from_secs(30), |inner| {
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
                        println!("TIMEOUT: {}", test);
                    }
                    Some(Err(e)) => {
                        failed_count += 1;
                        println!("FAILED:  {}:\n\t{:?}\n", test, e);
                    }
                    Some(Ok(())) => {
                        println!("PASSED:  {}", test);
                    }
                }
            }
            panic!("{} tests timed out and {} tests failed", timeout_count, failed_count)
        } else {
            let mut failed_count = 0;
            for (test, outcome) in lock.outcomes.iter() {
                match outcome {
                    Ok(()) => println!("PASSED: {}", test),
                    Err(e) => {
                        failed_count += 1;
                        println!("FAILED: {}:\n\t{:?}\n", test, e);
                    }
                }
            }
            if failed_count != 0 {
                panic!("{} tests failed", failed_count);
            } else {
                println!("All tests passed");
            }
        }
    }
}
