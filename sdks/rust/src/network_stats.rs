//! Network statistics derived from Ping/Pong round trips.
//!
//! [`NetworkStatsTracker`] holds all the maths, and is pure:
//! every time value is a number of microseconds supplied by the caller,
//! so it can be tested deterministically.
//! Monotonic times are on the client's monotonic clock,
//! measured from an arbitrary per-connection epoch;
//! server times are microseconds since the Unix epoch.

use spacetimedb_lib::Timestamp;
use std::{collections::VecDeque, time::Duration};

/// The monotonic clock instant type used for [`PingResult::sent_at`] and [`PingResult::received_at`].
///
/// This is [`std::time::Instant`], except under the `browser` feature,
/// where it's [`web_time::Instant`], since `std::time::Instant` is unavailable on `wasm32-unknown-unknown`.
#[cfg(not(feature = "browser"))]
pub(crate) use std::time::Instant;
#[cfg(feature = "browser")]
pub(crate) use web_time::Instant;

/// A snapshot of a connection's network statistics, derived from Ping/Pong round trips.
///
/// Get one from `DbConnection::network_stats`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NetworkStats {
    /// Smoothed round-trip time (RFC 6298 SRTT).
    pub rtt: Duration,
    /// The most recent round-trip time sample.
    pub rtt_latest: Duration,
    /// The lowest round-trip time over the recent sample window.
    pub rtt_min: Duration,
    /// Round-trip time variation (RFC 6298 RTTVAR).
    pub jitter: Duration,
    /// The estimated current time on the server, on the same clock as a reducer's `ctx.timestamp`.
    ///
    /// Adjusts gradually and never goes backwards.
    /// This is the estimate at the moment the snapshot was taken;
    /// read the stats again for a fresh one.
    pub server_now: Timestamp,
    /// When the server received the most recent Ping, as reported in its Pong.
    ///
    /// This is the raw measurement `server_now` is derived from: it doesn't advance between Pongs.
    pub server_time_latest: Timestamp,
    /// How long the oldest unanswered Ping has been waiting for its Pong,
    /// or zero when every Ping has been answered.
    ///
    /// On a healthy connection this stays below `rtt`.
    /// The other fields only change when a Pong arrives,
    /// so they keep showing the last good values while the network is silently down;
    /// this is the field that keeps rising.
    pub pong_wait: Duration,
}

/// The result of a single Ping sent with `DbConnection::ping`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PingResult {
    /// The round-trip time for this Ping, excluding the time the server held it:
    /// `round_trip - server_hold`, floored at zero.
    ///
    /// This is the sample that feeds [`NetworkStats`].
    pub rtt: Duration,
    /// The time from sending the Ping to receiving its Pong.
    pub round_trip: Duration,
    /// How long the server held the Ping before replying.
    pub server_hold: Duration,
    /// When the server received the Ping, on the same clock as a reducer's `ctx.timestamp`.
    pub server_receive_time: Timestamp,
    /// When the Ping was sent, on the local monotonic clock.
    ///
    /// This is a [`std::time::Instant`], or a `web_time::Instant` under the `browser` feature.
    pub sent_at: Instant,
    /// When the Pong arrived, on the local monotonic clock.
    ///
    /// This is a [`std::time::Instant`], or a `web_time::Instant` under the `browser` feature.
    pub received_at: Instant,
}

/// How many recent samples feed `rtt_min` and the clock offset estimate.
pub(crate) const NETWORK_STATS_WINDOW: usize = 10;

/// The fastest the applied clock offset may move towards a new estimate,
/// as a fraction of the monotonic time that has elapsed.
const NETWORK_STATS_MAX_SLEW: f64 = 0.05;

// RFC 6298 smoothing factors and clock granularity.
const ALPHA: f64 = 1.0 / 8.0;
const BETA: f64 = 1.0 / 4.0;
#[cfg(any(feature = "browser", test))]
const K: f64 = 4.0;
#[cfg(any(feature = "browser", test))]
const GRANULARITY_US: f64 = 1_000.0;

/// How many RTOs a Ping may go unanswered before the connection is considered unresponsive,
/// when that is longer than the configured Ping timeout.
#[cfg(any(feature = "browser", test))]
pub(crate) const PING_TIMEOUT_RTO_MULTIPLE: f64 = 4.0;

#[derive(Clone, Copy, Debug)]
struct Sample {
    r: f64,
    theta: f64,
}

/// Tracks round-trip time and server clock offset from Pong replies.
///
/// Pure: every time value is supplied by the caller in microseconds.
#[derive(Debug, Default)]
pub(crate) struct NetworkStatsTracker {
    srtt: f64,
    rttvar: f64,
    latest_r: f64,
    latest_server_receive_us: i64,
    /// The most recent samples, oldest first, at most [`NETWORK_STATS_WINDOW`] of them.
    samples: VecDeque<Sample>,
    estimate: f64,
    /// `None` until the first Pong.
    applied_offset: Option<f64>,
    last_slew_at: f64,
    /// The never-backwards clamp on `server_now`: `None` until the first snapshot.
    last_server_now_us: Option<f64>,
    /// Send times of Pings with no Pong yet, oldest first.
    unanswered_send_times_us: Vec<u64>,
}

impl NetworkStatsTracker {
    /// Record that a Ping went out, with `sent_us` as its `client_send_time`.
    pub(crate) fn record_ping(&mut self, sent_us: u64) {
        self.unanswered_send_times_us.push(sent_us);
    }

    /// Record a Pong which arrived at monotonic time `now_us`.
    ///
    /// `client_send_time_us` is the Ping's send time echoed back by the server,
    /// `server_receive_time_us` when the server received the Ping,
    /// and `server_hold_us` how long the server held it before replying.
    pub(crate) fn record_pong(
        &mut self,
        now_us: u64,
        client_send_time_us: u64,
        server_receive_time_us: i64,
        server_hold_us: i64,
    ) {
        // A Pong proves the link was alive when its Ping arrived, so it answers
        // every Ping sent up to then. Pongs can overtake one another on the
        // server, so this doesn't assume they arrive in order.
        self.unanswered_send_times_us
            .retain(|&sent_us| sent_us > client_send_time_us);

        let r = (now_us as f64 - client_send_time_us as f64 - server_hold_us as f64).max(0.0);
        if self.samples.is_empty() {
            self.srtt = r;
            self.rttvar = r / 2.0;
        } else {
            // RFC 6298 requires RTTVAR to be updated using the previous SRTT.
            self.rttvar = (1.0 - BETA) * self.rttvar + BETA * (self.srtt - r).abs();
            self.srtt = (1.0 - ALPHA) * self.srtt + ALPHA * r;
        }
        self.latest_r = r;
        self.latest_server_receive_us = server_receive_time_us;

        let theta = server_receive_time_us as f64 - (client_send_time_us as f64 + r / 2.0);
        self.samples.push_back(Sample { r, theta });
        if self.samples.len() > NETWORK_STATS_WINDOW {
            self.samples.pop_front();
        }

        if self.applied_offset.is_none() {
            self.estimate = self.min_sample().theta;
            self.applied_offset = Some(self.estimate);
            self.last_slew_at = now_us as f64;
        } else {
            // Slew up to now towards the old estimate before switching to the new one.
            self.advance(now_us);
            self.estimate = self.min_sample().theta;
        }
    }

    /// A fresh snapshot of the statistics at monotonic time `now_us`,
    /// or `None` if no Pong has been recorded since construction or [`Self::reset`].
    pub(crate) fn snapshot(&mut self, now_us: u64) -> Option<NetworkStats> {
        if self.samples.is_empty() {
            return None;
        }
        self.advance(now_us);
        let offset = self.applied_offset?;
        let server_now_us = (now_us as f64 + offset).max(self.last_server_now_us.unwrap_or(f64::NEG_INFINITY));
        self.last_server_now_us = Some(server_now_us);
        Some(NetworkStats {
            rtt: duration_from_us(self.srtt),
            rtt_latest: duration_from_us(self.latest_r),
            rtt_min: duration_from_us(self.min_sample().r),
            jitter: duration_from_us(self.rttvar),
            server_now: Timestamp::from_micros_since_unix_epoch(server_now_us.round() as i64),
            server_time_latest: Timestamp::from_micros_since_unix_epoch(self.latest_server_receive_us),
            pong_wait: Duration::from_micros(self.pong_wait_us(now_us)),
        })
    }

    /// Whether the connection has stopped answering Pings at monotonic time `now_us`:
    /// the oldest unanswered Ping has waited longer than `timeout`
    /// (or than [`PING_TIMEOUT_RTO_MULTIPLE`] RTOs, if longer),
    /// and at least one more Ping has gone out since it.
    ///
    /// Requiring a second Ping means a single stale send time,
    /// such as one from before the page was frozen or the machine slept,
    /// can't trip the timeout on its own: the connection must also miss a Ping sent since.
    #[cfg(any(feature = "browser", test))]
    pub(crate) fn is_unresponsive(&self, now_us: u64, timeout: Duration) -> bool {
        if self.unanswered_send_times_us.len() < 2 {
            return false;
        }
        let limit_us = (timeout.as_micros() as f64).max(PING_TIMEOUT_RTO_MULTIPLE * self.rto_us());
        self.pong_wait_us(now_us) as f64 > limit_us
    }

    /// Forget all samples, unanswered Pings, the clock offset and the never-backwards clamp.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// How long the oldest unanswered Ping has waited at monotonic time `now_us`, in microseconds,
    /// or 0 if every Ping has been answered.
    ///
    /// Unlike [`Self::snapshot`], this is available before the first Pong.
    pub(crate) fn pong_wait_us(&self, now_us: u64) -> u64 {
        self.unanswered_send_times_us
            .first()
            .map_or(0, |&oldest| now_us.saturating_sub(oldest))
    }

    // The RFC 6298 retransmission timeout, measured from the samples alone.
    // RFC 6298's 1 s initial value and floor are left out, so that the
    // configured timeout governs until a slow link shows it needs longer.
    #[cfg(any(feature = "browser", test))]
    fn rto_us(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.srtt + GRANULARITY_US.max(K * self.rttvar)
    }

    // The lowest-R sample in the window. Ties go to the most recent sample.
    // Only called when there is at least one sample.
    fn min_sample(&self) -> Sample {
        self.samples
            .iter()
            .copied()
            .reduce(|min, sample| if sample.r <= min.r { sample } else { min })
            .expect("`min_sample` called with no samples")
    }

    // Move the applied offset towards the estimate, by at most
    // `NETWORK_STATS_MAX_SLEW` of the monotonic time elapsed since the last move.
    fn advance(&mut self, now_us: u64) {
        let Some(applied) = self.applied_offset else {
            return;
        };
        let now_us = now_us as f64;
        let step = NETWORK_STATS_MAX_SLEW * (now_us - self.last_slew_at).max(0.0);
        let delta = self.estimate - applied;
        self.applied_offset = Some(applied + delta.clamp(-step, step));
        self.last_slew_at = self.last_slew_at.max(now_us);
    }
}

/// Convert a non-negative number of microseconds to a [`Duration`], rounded to the nanosecond.
fn duration_from_us(us: f64) -> Duration {
    Duration::from_nanos((us.max(0.0) * 1_000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A plausible server clock offset: monotonic client time vs. Unix epoch micros.
    const T0: i64 = 1_700_000_000_000_000;

    fn ms(ms: f64) -> Duration {
        duration_from_us(ms * 1_000.0)
    }

    // Record a Pong arriving at `now` with round-trip `r` and clock offset `theta`,
    // with no server hold time.
    fn pong(tracker: &mut NetworkStatsTracker, now: u64, r: u64, theta: i64) {
        let client_send_time = now - r;
        tracker.record_pong(
            now,
            client_send_time,
            client_send_time as i64 + (r / 2) as i64 + theta,
            0,
        );
    }

    fn server_now_us(stats: Option<NetworkStats>) -> i64 {
        stats.unwrap().server_now.to_micros_since_unix_epoch()
    }

    fn offset_at(tracker: &mut NetworkStatsTracker, now: u64) -> i64 {
        server_now_us(tracker.snapshot(now)) - now as i64
    }

    #[test]
    fn is_none_before_any_pong() {
        assert_eq!(NetworkStatsTracker::default().snapshot(1_000_000), None);
    }

    #[test]
    fn first_sample_seeds_srtt_and_rttvar() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_pong(1_000_000, 900_000, T0, 20_000);
        let stats = tracker.snapshot(1_000_000).unwrap();
        assert_eq!(stats.rtt, ms(80.0));
        assert_eq!(stats.jitter, ms(40.0));
        assert_eq!(stats.rtt_latest, ms(80.0));
        assert_eq!(stats.rtt_min, ms(80.0));
    }

    #[test]
    fn clamps_a_negative_round_trip_to_zero() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_pong(1_000_000, 990_000, T0, 50_000);
        let stats = tracker.snapshot(1_000_000).unwrap();
        assert_eq!(stats.rtt_latest, Duration::ZERO);
        assert_eq!(stats.rtt, Duration::ZERO);
        assert_eq!(stats.jitter, Duration::ZERO);
    }

    #[test]
    fn updates_rttvar_using_the_previous_srtt_then_srtt() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 100_000, T0);
        pong(&mut tracker, 2_000_000, 200_000, T0);
        let stats = tracker.snapshot(2_000_000).unwrap();
        assert_eq!(stats.jitter, ms(62.5));
        assert_eq!(stats.rtt, ms(112.5));
        assert_eq!(stats.rtt_latest, ms(200.0));
    }

    #[test]
    fn rtt_min_covers_only_the_recent_window() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        for i in 1..NETWORK_STATS_WINDOW as u64 {
            let now = 1_000_000 + i * 1_000_000;
            pong(&mut tracker, now, 50_000, T0);
            assert_eq!(tracker.snapshot(now).unwrap().rtt_min, ms(10.0));
        }
        // The 10_000 sample is now the oldest of a full window; one more evicts it.
        pong(&mut tracker, 20_000_000, 50_000, T0);
        assert_eq!(tracker.snapshot(20_000_000).unwrap().rtt_min, ms(50.0));
    }

    #[test]
    fn uses_the_offset_of_the_lowest_r_sample_not_the_latest() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0 + 5_000);
        pong(&mut tracker, 2_000_000, 50_000, T0 + 99_999);
        // Long enough for any slew to converge.
        assert_eq!(offset_at(&mut tracker, 1_000_000_000), T0 + 5_000);

        // Evict the low-R sample; ties then go to the most recent sample.
        for i in 0..NETWORK_STATS_WINDOW as u64 - 1 {
            pong(&mut tracker, 1_000_000_001 + i, 50_000, T0 + 7_000);
        }
        assert_eq!(offset_at(&mut tracker, 2_000_000_000), T0 + 7_000);
    }

    #[test]
    fn applies_the_first_offset_immediately() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        assert_eq!(server_now_us(tracker.snapshot(1_000_000)), 1_000_000 + T0);
    }

    #[test]
    fn server_time_latest_is_the_latest_pongs_server_receive_time() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_pong(1_000_000, 900_000, T0, 0);
        tracker.record_pong(2_000_000, 1_900_000, T0 + 1_000_000, 0);
        // It's the raw measurement, so it doesn't advance between Pongs.
        let stats = tracker.snapshot(2_500_000).unwrap();
        assert_eq!(stats.server_time_latest.to_micros_since_unix_epoch(), T0 + 1_000_000);
    }

    #[test]
    fn slews_towards_a_new_offset_at_5_percent_of_elapsed_time_then_stops() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        // Same R, so the new sample wins the tie and the estimate jumps by +1 s.
        pong(&mut tracker, 2_000_000, 10_000, T0 + 1_000_000);
        assert_eq!(offset_at(&mut tracker, 2_000_000), T0);
        assert_eq!(offset_at(&mut tracker, 3_000_000), T0 + 50_000);
        assert_eq!(offset_at(&mut tracker, 22_000_000), T0 + 1_000_000);
        assert_eq!(offset_at(&mut tracker, 30_000_000), T0 + 1_000_000);
    }

    #[test]
    fn server_now_never_goes_backwards() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        let first = server_now_us(tracker.snapshot(5_000_000));
        assert_eq!(server_now_us(tracker.snapshot(5_000_000 - 5_000)), first);

        // A large negative estimate pulls the offset down, but never the clock.
        pong(&mut tracker, 6_000_000, 10_000, T0 - 1_000_000_000);
        let mut previous = first;
        for now in [6_000_000, 6_000_001, 7_000_000, 6_500_000, 60_000_000] {
            let current = server_now_us(tracker.snapshot(now));
            assert!(current >= previous, "{current} < {previous} at {now}");
            previous = current;
        }
    }

    #[test]
    fn reset_clears_the_stats_and_the_never_backwards_clamp() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        let before = server_now_us(tracker.snapshot(5_000_000));

        tracker.reset();
        assert_eq!(tracker.snapshot(5_000_000), None);

        pong(&mut tracker, 1_000_000, 30_000, T0 - 1_000_000);
        let stats = tracker.snapshot(1_000_000);
        assert_eq!(server_now_us(stats), 1_000_000 + T0 - 1_000_000);
        assert!(server_now_us(stats) < before);
        assert_eq!(stats.unwrap().rtt, ms(30.0));
        assert_eq!(stats.unwrap().rtt_min, ms(30.0));
    }

    #[test]
    fn successive_snapshots_at_the_same_time_are_equal() {
        let mut tracker = NetworkStatsTracker::default();
        pong(&mut tracker, 1_000_000, 10_000, T0);
        let a = tracker.snapshot(1_000_000);
        let b = tracker.snapshot(1_000_000);
        assert_eq!(a, b);
    }

    // Answer the Ping sent at `sent_us`, arriving `r` later.
    fn answer(tracker: &mut NetworkStatsTracker, sent_us: u64, r: u64) {
        tracker.record_pong(sent_us + r, sent_us, T0 + sent_us as i64, 0);
    }

    #[test]
    fn pong_wait_is_zero_when_every_ping_has_been_answered() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        answer(&mut tracker, 1_000_000, 20_000);
        assert_eq!(tracker.snapshot(5_000_000).unwrap().pong_wait, Duration::ZERO);
    }

    #[test]
    fn pong_wait_keeps_rising_while_pongs_stop_arriving() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        answer(&mut tracker, 1_000_000, 20_000);
        tracker.record_ping(2_000_000);
        tracker.record_ping(3_000_000);

        // The other fields are frozen at their last good values...
        let early = tracker.snapshot(2_500_000).unwrap();
        let late = tracker.snapshot(6_000_000).unwrap();
        assert_eq!(late.rtt, early.rtt);
        // ...but pong_wait measures from the oldest unanswered Ping.
        assert_eq!(early.pong_wait, ms(500.0));
        assert_eq!(late.pong_wait, ms(4_000.0));
    }

    #[test]
    fn a_pong_answers_every_ping_sent_up_to_its_own() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        tracker.record_ping(2_000_000);
        tracker.record_ping(3_000_000);

        // The Pong for the second Ping overtakes the first's.
        answer(&mut tracker, 2_000_000, 20_000);
        assert_eq!(tracker.pong_wait_us(3_500_000), 500_000);

        // The first Pong arriving late changes nothing.
        answer(&mut tracker, 1_000_000, 2_100_000);
        assert_eq!(tracker.pong_wait_us(3_500_000), 500_000);
    }

    #[test]
    fn pong_wait_us_is_available_before_the_first_pong() {
        let mut tracker = NetworkStatsTracker::default();
        assert_eq!(tracker.pong_wait_us(1_000_000), 0);
        tracker.record_ping(1_000_000);
        assert_eq!(tracker.snapshot(4_000_000), None);
        assert_eq!(tracker.pong_wait_us(4_000_000), 3_000_000);
    }

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn is_unresponsive_once_two_pings_are_unanswered_past_the_timeout() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        tracker.record_ping(2_000_000);
        assert!(!tracker.is_unresponsive(11_000_000, TIMEOUT));
        assert!(tracker.is_unresponsive(11_000_001, TIMEOUT));
    }

    #[test]
    fn a_single_unanswered_ping_is_never_unresponsive() {
        // As after a long sleep: only one Ping went out, and the next tick will
        // send another before deciding.
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        assert!(!tracker.is_unresponsive(1_000_000_000, TIMEOUT));
    }

    #[test]
    fn allows_a_slow_link_several_rtos_before_giving_up() {
        let mut tracker = NetworkStatsTracker::default();
        // A steady 3 s round trip: RTO = SRTT + 4 * RTTVAR = 3 s + 4 * 1.5 s.
        tracker.record_ping(0);
        answer(&mut tracker, 0, 3_000_000);
        let rto_us = 9_000_000;
        tracker.record_ping(10_000_000);
        tracker.record_ping(11_000_000);

        let limit_us = PING_TIMEOUT_RTO_MULTIPLE as u64 * rto_us;
        assert!(!tracker.is_unresponsive(10_000_000 + limit_us, TIMEOUT));
        assert!(tracker.is_unresponsive(10_000_001 + limit_us, TIMEOUT));
    }

    #[test]
    fn reset_forgets_unanswered_pings() {
        let mut tracker = NetworkStatsTracker::default();
        tracker.record_ping(1_000_000);
        tracker.record_ping(2_000_000);
        tracker.reset();
        assert_eq!(tracker.pong_wait_us(60_000_000), 0);
        assert!(!tracker.is_unresponsive(60_000_000, TIMEOUT));
    }
}
