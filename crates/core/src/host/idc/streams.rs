use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;
use std::time::{Duration, Instant};

// Retry delays start at 100 ms and double up to 30 seconds.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub(in crate::host) struct StreamId(pub u64);

impl Display for StreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Allows one message per stream to run through delivery, callback, and cleanup.
/// TODO(idc): Support pipelining.
struct OrderedStream {
    /// The sequence being processed, or `None` between attempts.
    active_seq: Option<u64>,
    /// Earliest time a failed attempt may retry.
    blocked_until: Option<Instant>,
    /// Delay after the next failed attempt. Resets when cleanup succeeds.
    backoff: Duration,
}

impl OrderedStream {
    fn new() -> Self {
        Self {
            active_seq: None,
            blocked_until: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn is_ready(&self) -> bool {
        self.active_seq.is_none() && self.blocked_until.is_none_or(|until| Instant::now() >= until)
    }
}

/// Keeps stream state and the ready queue in sync. Each stream is queued at most once.
/// It maintains a FIFO ready queue poll streams.
#[derive(Default)]
pub(in crate::host) struct DeliveryStreams {
    streams: HashMap<StreamId, OrderedStream>,
    ready: VecDeque<StreamId>,
    queued: HashSet<StreamId>,
}

impl DeliveryStreams {
    /// Wake a stream after an insert or retry deadline without interrupting an active attempt or backoff.
    pub fn wake(&mut self, stream_id: StreamId) {
        let state = self.streams.entry(stream_id).or_insert_with(OrderedStream::new);
        if state.is_ready() && self.queued.insert(stream_id) {
            self.ready.push_back(stream_id);
        }
    }

    /// Take a stream for a database lookup. Call `start` if it has work, or `remove_idle` otherwise.
    pub fn pop_ready(&mut self) -> Option<StreamId> {
        let stream_id = self.ready.pop_front()?;
        self.queued.remove(&stream_id);
        Some(stream_id)
    }

    /// Keep this stream busy until delivery, callback, and cleanup finish.
    pub fn start(&mut self, stream_id: StreamId, seq: u64) {
        assert!(
            !self.queued.contains(&stream_id),
            "stream must be taken from the ready queue first"
        );
        let state = self
            .streams
            .get_mut(&stream_id)
            .expect("ready stream has delivery state");
        assert!(
            state.is_ready(),
            "ordered stream must be ready before starting a message"
        );
        state.active_seq = Some(seq);
    }

    /// Queue the next message after cleanup commits and reset the retry delay.
    pub fn finish(&mut self, stream_id: StreamId, seq: u64) {
        let state = self
            .streams
            .get_mut(&stream_id)
            .expect("active stream has delivery state");
        assert_eq!(state.active_seq, Some(seq), "completion must match the active sequence");
        state.active_seq = None;
        state.blocked_until = None;
        state.backoff = INITIAL_BACKOFF;
        self.wake(stream_id);
    }

    /// End the failed attempt and return its retry delay. New inserts do not shorten this delay.
    pub fn fail(&mut self, stream_id: StreamId, seq: u64) -> Duration {
        let state = self
            .streams
            .get_mut(&stream_id)
            .expect("active stream has delivery state");
        assert_eq!(state.active_seq, Some(seq), "failure must match the active sequence");
        // Clear the active sequence so the same message can run again after backoff.
        state.active_seq = None;
        let delay = state.backoff;
        state.blocked_until = Some(Instant::now() + delay);
        state.backoff = (delay * 2).min(MAX_BACKOFF);
        delay
    }

    /// Forget an empty stream after its database lookup. A later insert recreates it.
    pub fn remove_idle(&mut self, stream_id: StreamId) {
        assert!(
            !self.queued.contains(&stream_id),
            "stream must be taken from the ready queue first"
        );
        let state = self.streams.get(&stream_id).expect("idle stream has delivery state");
        assert!(state.is_ready(), "cannot remove an active or blocked stream");
        self.streams.remove(&stream_id);
    }

    /// Find the next stream to retry and how long it needs to wait.
    pub fn next_retry_delay(&self) -> Option<(StreamId, Duration)> {
        self.streams
            .iter()
            .filter(|(_, state)| state.active_seq.is_none())
            .filter_map(|(&id, state)| state.blocked_until.map(|deadline| (id, deadline)))
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(id, deadline)| (id, deadline.saturating_duration_since(Instant::now())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_and_completion_queue_each_stream_once() {
        let mut streams = DeliveryStreams::default();
        let first = StreamId(1);
        let second = StreamId(2);
        streams.wake(first);
        streams.wake(first);
        streams.wake(second);
        assert_eq!(streams.pop_ready(), Some(first));
        streams.start(first, 1);
        streams.wake(first);
        assert_eq!(streams.pop_ready(), Some(second));
        streams.start(second, 1);
        assert_eq!(streams.pop_ready(), None);

        streams.finish(first, 1);
        streams.wake(first);
        assert_eq!(streams.pop_ready(), Some(first));
        assert_eq!(streams.pop_ready(), None);
        streams.remove_idle(first);
        assert!(!streams.streams.contains_key(&first));
        streams.wake(first);
        assert_eq!(streams.pop_ready(), Some(first));
        assert_eq!(streams.streams[&second].active_seq, Some(1));
    }

    #[test]
    fn retries_preserve_backoff_and_skip_active_streams() {
        let mut streams = DeliveryStreams::default();
        let first = StreamId(1);
        let second = StreamId(2);
        for id in [first, second] {
            streams.wake(id);
            assert_eq!(streams.pop_ready(), Some(id));
            streams.start(id, 1);
            assert_eq!(streams.fail(id, 1), INITIAL_BACKOFF);
            // Use a future deadline so this check does not depend on test execution speed.
            streams.streams.get_mut(&id).unwrap().blocked_until = Some(Instant::now() + Duration::from_secs(3600));
            streams.wake(id);
            assert_eq!(streams.pop_ready(), None);
        }

        let now = Instant::now();
        streams.streams.get_mut(&first).unwrap().blocked_until = Some(now);
        streams.streams.get_mut(&second).unwrap().blocked_until = Some(now - INITIAL_BACKOFF);
        assert_eq!(streams.next_retry_delay(), Some((second, Duration::ZERO)));
        streams.wake(second);
        assert_eq!(streams.pop_ready(), Some(second));
        streams.start(second, 1);
        assert_eq!(streams.next_retry_delay(), Some((first, Duration::ZERO)));
        streams.wake(first);
        assert_eq!(streams.pop_ready(), Some(first));
        streams.start(first, 1);
        assert_eq!(streams.next_retry_delay(), None);
        assert_eq!(streams.fail(first, 1), INITIAL_BACKOFF * 2);

        streams.streams.get_mut(&first).unwrap().blocked_until = Some(now);
        streams.wake(first);
        assert_eq!(streams.pop_ready(), Some(first));
        streams.start(first, 1);
        streams.finish(first, 1);
        assert_eq!(streams.pop_ready(), Some(first));
        streams.start(first, 2);
        assert_eq!(streams.fail(first, 2), INITIAL_BACKOFF);
    }
}
