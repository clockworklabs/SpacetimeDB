use std::collections::{HashMap, VecDeque};
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

/// The next step for a stream taken from the ready queue.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(in crate::host) enum StreamWork {
    /// Load message `seq`, or the one after `ack_prefix` when `None`.
    /// Call `start` if it exists, or `drain` otherwise.
    Send { seq: Option<u64> },
    /// Run the callback and cleanup for `seq`, the oldest message in the window.
    /// Call `retired` or `retire_failed`.
    Retire { seq: u64 },
}

/// Progress of one message in the window.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum Slot {
    /// A delivery attempt failed and the message must be sent again.
    Unsent,
    /// A delivery attempt is in progress.
    Sending,
    /// The result is stored and the message waits to retire.
    Delivered,
    /// The callback and cleanup are running.
    Retiring,
}

/// Pipelined delivery state for one ordered stream.
///
/// Messages are sent in sequence order, so the unretired ones form a sliding
/// window with no gaps. Up to `depth` messages fit in it. Deliveries finish in
/// any order, but only the oldest message retires, because cleanup advances
/// `ack_prefix` and `on_result` must observe sender order.
struct OrderedStream {
    /// Unretired messages in sequence order, with no gaps. When empty, every
    /// loaded message has retired and the next one to load is `ack_prefix + 1`.
    window: VecDeque<(u64, Slot)>,
    /// The last lookup found no message after the window. A new insert clears this.
    caught_up: bool,
    /// Whether the stream is in the ready queue.
    queued: bool,
    /// Earliest time a failed attempt may retry.
    blocked_until: Option<Instant>,
    /// Delay after the next failed attempt. Resets when cleanup succeeds.
    backoff: Duration,
}

impl OrderedStream {
    fn new() -> Self {
        Self {
            window: VecDeque::new(),
            caught_up: false,
            queued: false,
            blocked_until: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn is_blocked(&self) -> bool {
        self.blocked_until.is_some_and(|until| Instant::now() < until)
    }

    fn has_unsent(&self) -> bool {
        self.window.iter().any(|&(_, slot)| slot == Slot::Unsent)
    }

    /// The oldest message, if it may start retiring.
    fn retirable(&self) -> Option<u64> {
        match self.window.front() {
            Some(&(seq, Slot::Delivered)) => Some(seq),
            _ => None,
        }
    }

    /// The first message to send again, or else the one after the newest.
    /// `None` means the one after `ack_prefix`.
    fn send_cursor(&self) -> Option<u64> {
        if let Some(&(seq, _)) = self.window.iter().find(|&&(_, slot)| slot == Slot::Unsent) {
            return Some(seq);
        }
        self.window.back().map(|&(newest, _)| newest + 1)
    }

    /// Resends always fit, because their messages are already in the window.
    fn can_send(&self, depth: usize) -> bool {
        self.has_unsent() || (!self.caught_up && self.window.len() < depth)
    }

    /// Whether the stream could make progress once any backoff ends.
    fn has_work(&self, depth: usize) -> bool {
        self.retirable().is_some() || self.can_send(depth)
    }

    /// Move `seq` from `from` to `to`.
    fn transition(&mut self, seq: u64, from: Slot, to: Slot) {
        let (_, slot) = self
            .window
            .iter_mut()
            .find(|(found, _)| *found == seq)
            .unwrap_or_else(|| panic!("sequence {seq} is not in the window"));
        assert_eq!(*slot, from, "sequence {seq} is in the wrong state");
        *slot = to;
    }

    /// Delay retries after a failure. Failures that land during an existing backoff,
    /// such as other pipelined attempts to an unavailable receiver, do not extend it.
    fn block(&mut self) -> Duration {
        let now = Instant::now();
        if let Some(until) = self.blocked_until.filter(|&until| now < until) {
            return until - now;
        }
        let delay = self.backoff;
        self.blocked_until = Some(now + delay);
        self.backoff = (delay * 2).min(MAX_BACKOFF);
        delay
    }
}

/// Keeps stream state and the ready queue in sync. Each stream is queued at most once.
/// Streams with more work go to the back of the queue, so streams take turns.
pub(in crate::host) struct DeliveryStreams {
    /// Maximum messages in each stream's window.
    depth: usize,
    streams: HashMap<StreamId, OrderedStream>,
    /// Streams with `queued` set, in the order they take turns.
    ready: VecDeque<StreamId>,
}

impl DeliveryStreams {
    pub fn new(depth: usize) -> Self {
        assert!(depth > 0, "pipeline depth must be positive");
        Self {
            depth,
            streams: HashMap::new(),
            ready: VecDeque::new(),
        }
    }

    /// Wake a stream after an insert or retry deadline without interrupting active attempts or backoff.
    pub fn wake(&mut self, stream_id: StreamId) {
        let state = self.streams.entry(stream_id).or_insert_with(OrderedStream::new);
        state.caught_up = false;
        self.schedule(stream_id);
    }

    /// Take the next stream with work. Retiring takes priority over sending.
    pub fn pop_ready(&mut self) -> Option<(StreamId, StreamWork)> {
        while let Some(stream_id) = self.ready.pop_front() {
            // Skip entries left behind by a stream that was removed, and possibly recreated, while queued.
            let Some(state) = self.streams.get_mut(&stream_id).filter(|state| state.queued) else {
                continue;
            };
            state.queued = false;
            // Completions may have changed the stream since it was queued.
            if state.is_blocked() || !state.has_work(self.depth) {
                continue;
            }
            state.blocked_until = None;
            if let Some(seq) = state.retirable() {
                self.start_retire(stream_id, seq);
                return Some((stream_id, StreamWork::Retire { seq }));
            }
            let seq = state.send_cursor();
            return Some((stream_id, StreamWork::Send { seq }));
        }
        None
    }

    /// Record that `seq` is being sent. It must be the message that `Send` asked for.
    pub fn start(&mut self, stream_id: StreamId, seq: u64) {
        let state = self.state(stream_id);
        if let Some(expected) = state.send_cursor() {
            assert_eq!(seq, expected, "must send the message at the stream cursor");
        }
        if state.has_unsent() {
            state.transition(seq, Slot::Unsent, Slot::Sending);
        } else {
            state.window.push_back((seq, Slot::Sending));
        }
        self.schedule(stream_id);
    }

    /// Record that `Send` found no message after the newest one in the window.
    /// Forget the stream if the window is empty. A later insert wakes it again.
    pub fn drain(&mut self, stream_id: StreamId) {
        let state = self.state(stream_id);
        assert!(!state.has_unsent(), "a message to send again cannot be missing");
        state.caught_up = true;
        if state.window.is_empty() {
            self.streams.remove(&stream_id);
        } else {
            self.schedule(stream_id);
        }
    }

    /// Record that the result for `seq` is stored. It retires after every earlier sequence.
    pub fn delivered(&mut self, stream_id: StreamId, seq: u64) {
        self.state(stream_id).transition(seq, Slot::Sending, Slot::Delivered);
        self.schedule(stream_id);
    }

    /// End a failed delivery attempt and return its retry delay.
    /// The message is sent again after the backoff.
    pub fn send_failed(&mut self, stream_id: StreamId, seq: u64) -> Duration {
        let state = self.state(stream_id);
        state.transition(seq, Slot::Sending, Slot::Unsent);
        state.block()
    }

    /// Mark `seq` as retiring. `pop_ready` calls this before returning `Retire`.
    pub fn start_retire(&mut self, stream_id: StreamId, seq: u64) {
        let state = self.state(stream_id);
        assert_eq!(state.retirable(), Some(seq), "only the oldest message may retire");
        state.transition(seq, Slot::Delivered, Slot::Retiring);
        self.schedule(stream_id);
    }

    /// Record that callback and cleanup committed for `seq`, and reset the retry delay.
    pub fn retired(&mut self, stream_id: StreamId, seq: u64) {
        let state = self.state(stream_id);
        assert_eq!(
            state.window.pop_front(),
            Some((seq, Slot::Retiring)),
            "only the oldest message may retire"
        );
        state.backoff = INITIAL_BACKOFF;
        if state.caught_up && state.window.is_empty() {
            self.streams.remove(&stream_id);
        } else {
            self.schedule(stream_id);
        }
    }

    /// End a failed cleanup and return its retry delay. The result stays stored,
    /// so the retry does not send the message again.
    pub fn retire_failed(&mut self, stream_id: StreamId, seq: u64) -> Duration {
        let state = self.state(stream_id);
        state.transition(seq, Slot::Retiring, Slot::Delivered);
        state.block()
    }

    /// Find the next stream to retry and how long it needs to wait.
    pub fn next_retry_delay(&self) -> Option<(StreamId, Duration)> {
        self.streams
            .iter()
            // A due retry may remain queued while the actor is at capacity.
            // Waking it again would spin without making progress.
            .filter(|(_, state)| state.has_work(self.depth) && !state.queued)
            .filter_map(|(&id, state)| state.blocked_until.map(|deadline| (id, deadline)))
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(id, deadline)| (id, deadline.saturating_duration_since(Instant::now())))
    }

    fn state(&mut self, stream_id: StreamId) -> &mut OrderedStream {
        self.streams
            .get_mut(&stream_id)
            .expect("active stream has delivery state")
    }

    /// Queue the stream if it can make progress now.
    fn schedule(&mut self, stream_id: StreamId) {
        let Some(state) = self.streams.get_mut(&stream_id) else {
            return;
        };
        if !state.queued && !state.is_blocked() && state.has_work(self.depth) {
            state.queued = true;
            self.ready.push_back(stream_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: StreamId = StreamId(1);
    const SECOND: StreamId = StreamId(2);

    fn send(seq: Option<u64>) -> StreamWork {
        StreamWork::Send { seq }
    }

    fn unblock(streams: &mut DeliveryStreams, id: StreamId) {
        streams.streams.get_mut(&id).unwrap().blocked_until = Some(Instant::now());
        streams.wake(id);
    }

    #[test]
    fn notifications_queue_each_stream_once_and_streams_take_turns() {
        let mut streams = DeliveryStreams::new(2);
        streams.wake(FIRST);
        streams.wake(FIRST);
        streams.wake(SECOND);
        assert_eq!(streams.pop_ready(), Some((FIRST, send(None))));
        streams.start(FIRST, 1);
        streams.wake(FIRST);
        assert_eq!(streams.pop_ready(), Some((SECOND, send(None))));
        streams.start(SECOND, 1);
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(2)))));
        streams.start(FIRST, 2);
        assert_eq!(streams.pop_ready(), Some((SECOND, send(Some(2)))));
        streams.drain(SECOND);
        // FIRST is full and SECOND is caught up.
        assert_eq!(streams.pop_ready(), None);

        // An insert makes a caught-up stream look again.
        streams.wake(SECOND);
        assert_eq!(streams.pop_ready(), Some((SECOND, send(Some(2)))));
    }

    #[test]
    fn deliveries_finish_in_any_order_but_retire_in_sequence() {
        let mut streams = DeliveryStreams::new(3);
        streams.wake(FIRST);
        for (seq, cursor) in [(1, None), (2, Some(2)), (3, Some(3))] {
            assert_eq!(streams.pop_ready(), Some((FIRST, send(cursor))));
            streams.start(FIRST, seq);
        }
        assert_eq!(streams.pop_ready(), None);

        streams.delivered(FIRST, 3);
        streams.delivered(FIRST, 2);
        // Sequence 1 has not been delivered, so nothing can retire.
        assert_eq!(streams.pop_ready(), None);

        streams.delivered(FIRST, 1);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 1 })));
        // One retirement at a time.
        assert_eq!(streams.pop_ready(), None);
        streams.retired(FIRST, 1);
        // Retiring comes before sending, and retiring frees a slot.
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 2 })));
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(4)))));
        streams.start(FIRST, 4);
        streams.retired(FIRST, 2);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 3 })));
        streams.retired(FIRST, 3);
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(5)))));
        streams.drain(FIRST);
        streams.delivered(FIRST, 4);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 4 })));
        streams.retired(FIRST, 4);
        // A caught-up stream with nothing active is forgotten.
        assert!(!streams.streams.contains_key(&FIRST));
    }

    #[test]
    fn failed_delivery_resends_from_that_sequence_after_backoff() {
        let mut streams = DeliveryStreams::new(3);
        streams.wake(FIRST);
        for seq in 1..=3 {
            streams.pop_ready().unwrap();
            streams.start(FIRST, seq);
        }
        streams.delivered(FIRST, 2);
        assert_eq!(streams.send_failed(FIRST, 1), INITIAL_BACKOFF);
        // Other attempts that fail during the backoff do not extend it.
        assert!(streams.send_failed(FIRST, 3) <= INITIAL_BACKOFF);
        streams.wake(FIRST);
        assert_eq!(streams.pop_ready(), None);
        assert_eq!(streams.next_retry_delay().map(|(id, _)| id), Some(FIRST));

        unblock(&mut streams, FIRST);
        assert_eq!(streams.next_retry_delay(), None);
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(1)))));
        streams.start(FIRST, 1);
        // Sequence 2 is already delivered, so the next resend is sequence 3.
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(3)))));
        streams.start(FIRST, 3);
        assert_eq!(streams.pop_ready(), None);

        streams.delivered(FIRST, 1);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 1 })));
        streams.retired(FIRST, 1);
        assert_eq!(streams.streams[&FIRST].backoff, INITIAL_BACKOFF);
    }

    #[test]
    fn failed_cleanup_retries_without_resending() {
        let mut streams = DeliveryStreams::new(2);
        streams.wake(FIRST);
        streams.pop_ready().unwrap();
        streams.start(FIRST, 1);
        streams.delivered(FIRST, 1);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 1 })));
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(2)))));
        streams.drain(FIRST);
        assert_eq!(streams.retire_failed(FIRST, 1), INITIAL_BACKOFF);
        assert_eq!(streams.pop_ready(), None);

        unblock(&mut streams, FIRST);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 1 })));
        // A second failure in a new backoff period doubles the delay.
        assert_eq!(streams.retire_failed(FIRST, 1), INITIAL_BACKOFF * 2);
        unblock(&mut streams, FIRST);
        assert_eq!(streams.pop_ready(), Some((FIRST, StreamWork::Retire { seq: 1 })));
        streams.retired(FIRST, 1);
        // The wake from `unblock` cleared `caught_up`, so the stream looks for more.
        // The window is empty, so the lookup starts after `ack_prefix`.
        assert_eq!(streams.pop_ready(), Some((FIRST, send(None))));
        streams.drain(FIRST);
        assert!(!streams.streams.contains_key(&FIRST));
    }

    #[test]
    fn retries_skip_queued_and_idle_streams() {
        let mut streams = DeliveryStreams::new(1);
        for id in [FIRST, SECOND] {
            streams.wake(id);
            assert_eq!(streams.pop_ready(), Some((id, send(None))));
            streams.start(id, 1);
            assert_eq!(streams.send_failed(id, 1), INITIAL_BACKOFF);
            // Use a future deadline so this check does not depend on test execution speed.
            streams.streams.get_mut(&id).unwrap().blocked_until = Some(Instant::now() + Duration::from_secs(3600));
            streams.wake(id);
            assert_eq!(streams.pop_ready(), None);
        }

        let now = Instant::now();
        streams.streams.get_mut(&FIRST).unwrap().blocked_until = Some(now);
        streams.streams.get_mut(&SECOND).unwrap().blocked_until = Some(now - INITIAL_BACKOFF);
        assert_eq!(streams.next_retry_delay(), Some((SECOND, Duration::ZERO)));
        streams.wake(SECOND);
        // Queued retries no longer need a timer, even before they can start.
        assert_eq!(streams.next_retry_delay(), Some((FIRST, Duration::ZERO)));
        assert_eq!(streams.pop_ready(), Some((SECOND, send(Some(1)))));
        streams.start(SECOND, 1);
        // A full stream needs no timer; its completion schedules it.
        assert_eq!(streams.next_retry_delay(), Some((FIRST, Duration::ZERO)));
        streams.wake(FIRST);
        assert_eq!(streams.pop_ready(), Some((FIRST, send(Some(1)))));
        streams.start(FIRST, 1);
        assert_eq!(streams.next_retry_delay(), None);
    }
}
