//! Virtual time for the local DST simulator.

pub use spacetimedb_runtime::sim::time::{
    advance, now, sleep, timeout, try_current_handle, TimeHandle, TimeoutElapsed,
};

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::{seed::DstSeed, sim};

    #[test]
    fn sleep_fast_forwards_virtual_time() {
        let mut runtime = sim::Runtime::new(DstSeed(101)).unwrap();

        runtime.block_on(async {
            assert_eq!(super::now(), Duration::ZERO);
            super::sleep(Duration::from_millis(5)).await;
            assert_eq!(super::now(), Duration::from_millis(5));
        });
    }

    #[test]
    fn shorter_timer_wakes_first() {
        let mut runtime = sim::Runtime::new(DstSeed(102)).unwrap();
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
        let mut runtime = sim::Runtime::new(DstSeed(103)).unwrap();

        runtime.block_on(async {
            super::advance(Duration::from_millis(7));
            assert_eq!(super::now(), Duration::from_millis(7));
        });
    }

    #[test]
    fn timeout_returns_future_output_before_deadline() {
        let mut runtime = sim::Runtime::new(DstSeed(104)).unwrap();

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
        let mut runtime = sim::Runtime::new(DstSeed(105)).unwrap();

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
