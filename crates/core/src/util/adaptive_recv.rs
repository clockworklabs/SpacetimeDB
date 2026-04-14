use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;

/// Receives from a Tokio unbounded channel with an adaptive linger policy.
///
/// This helper is intended for single-consumer background workers that want
/// to avoid parking on `recv()` after every message during bursty traffic.
///
/// The receiver has two modes - hot and cold. In cold mode it blocks on
/// `recv()` until the next message arrives. In hot mode it prefers to stay
/// awake, so after receiving a message, it will drain the channel, sleep for
/// a short period (linger), and only then poll the channel again. This keeps
/// the receiver off `recv()` during the linger window, so producers can enqueue
/// more work without waking a parked task.
///
/// The linger policy is as follows: If work is present when a linger window
/// expires, double the window up to `max_linger`. If a linger window expires
/// and the queue is still empty, reset the window to `baseline_linger`.
///
/// Note, messages returned immediately by `try_recv()` do not count as hits,
/// and do not double the linger window.
#[derive(Debug)]
pub struct AdaptiveUnboundedReceiver<T> {
    rx: mpsc::UnboundedReceiver<T>,
    linger: AdaptiveLinger,
    is_hot: bool,
}

impl<T> AdaptiveUnboundedReceiver<T> {
    /// Create an adaptive receiver around a Tokio unbounded channel.
    ///
    /// `baseline_linger` is the linger window used after a cold wakeup or any
    /// linger miss. `max_linger` caps how far the linger window may grow after
    /// repeated linger hits.
    ///
    /// This constructor does not spawn any tasks and does not alter the
    /// channel's ordering semantics. It only configures how aggressively the
    /// consumer stays awake after work arrives.
    pub fn new(rx: mpsc::UnboundedReceiver<T>, baseline_linger: Duration, max_linger: Duration) -> Self {
        Self {
            rx,
            linger: AdaptiveLinger::new(baseline_linger, max_linger),
            is_hot: false,
        }
    }

    /// Receive the next message while adapting how aggressively we linger
    /// before parking again.
    ///
    /// Once a worker has been woken up by one message, subsequent calls try to
    /// stay on the hot path:
    ///
    /// 1. Drain any already-queued work immediately with `try_recv()`
    /// 2. If the queue is empty, sleep for the current linger window
    /// 3. When the sleep fires, poll the queue again with `try_recv()`
    /// 4. On a linger hit, double the window and continue lingering
    /// 5. On a linger miss, reset the window to the baseline and park on `recv()`
    ///
    /// This trades a small amount of hot-path latency for lower wake overhead.
    /// While the receiver is hot, senders enqueue into the channel without
    /// waking a parked `recv()` future.
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            if !self.is_hot {
                let message = self.rx.recv().await?;
                self.is_hot = true;
                return Some(message);
            }

            match self.rx.try_recv() {
                Ok(message) => return Some(message),
                Err(mpsc::error::TryRecvError::Disconnected) => return None,
                Err(mpsc::error::TryRecvError::Empty) => {}
            }

            let linger = self.linger.current();
            if linger.is_zero() {
                self.cool_down();
                continue;
            }

            sleep(linger).await;

            match self.rx.try_recv() {
                Ok(message) => {
                    self.linger.on_hit();
                    return Some(message);
                }
                Err(mpsc::error::TryRecvError::Disconnected) => return None,
                Err(mpsc::error::TryRecvError::Empty) => {
                    self.cool_down();
                }
            }
        }
    }

    /// Return the receiver to its cold state after a linger miss.
    ///
    /// The next call to [`Self::recv`] will block on the underlying channel
    /// instead of continuing to linger, and the linger policy is reset to its
    /// baseline window.
    fn cool_down(&mut self) {
        self.is_hot = false;
        self.linger.on_miss();
    }
}

#[derive(Debug)]
struct AdaptiveLinger {
    baseline: Duration,
    current: Duration,
    max: Duration,
}

impl AdaptiveLinger {
    /// Create a linger policy with a baseline window and an upper bound.
    ///
    /// `baseline` is the window restored after any linger miss. `max` caps how
    /// far the window may grow after repeated linger hits.
    fn new(baseline: Duration, max: Duration) -> Self {
        assert!(
            baseline <= max,
            "baseline linger ({baseline:?}) must not exceed max linger ({max:?})"
        );
        Self {
            baseline,
            current: baseline,
            max,
        }
    }

    /// Return the current linger window.
    fn current(&self) -> Duration {
        self.current
    }

    /// Record a linger hit by growing the next linger window.
    ///
    /// The window doubles on each hit until it reaches `self.max`.
    fn on_hit(&mut self) {
        self.current = self.current.saturating_mul(2).min(self.max);
    }

    /// Record a linger miss by resetting to the baseline window.
    fn on_miss(&mut self) {
        self.current = self.baseline;
    }
}
