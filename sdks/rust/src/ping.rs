//! Ping/Pong plumbing shared by the WebSocket loop and the connection.
//!
//! The WebSocket loop owns the timing: it decides when to send Pings,
//! stamps them as it sends them, and takes each Pong's arrival time
//! straight off the socket, before any parse or apply queue,
//! so client-side queueing can't inflate the round-trip time.
//! It drives a [`PingDriver`], while the connection holds a [`PingHandle`]
//! through which it reads the [`NetworkStats`] and requests on-demand Pings.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::{
    db_connection::next_request_id,
    network_stats::{Instant, NetworkStats, NetworkStatsTracker, PingResult},
};
use futures_channel::{mpsc, oneshot};
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_data_structures::map::HashMap;
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

/// The default interval between periodic Pings.
pub(crate) const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(1);

/// The default for how long Pings may go unanswered before the connection is treated as lost.
#[cfg(feature = "browser")]
pub(crate) const DEFAULT_PING_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub(crate) struct PingConfig {
    /// How often to send a periodic Ping. Zero disables periodic pinging.
    pub interval: Duration,
    /// How long Pings may go unanswered before the connection is treated as lost.
    ///
    /// Only the `browser` build has this check: native builds rely on the
    /// WebSocket-level idle check in `websocket.rs` instead.
    #[cfg(feature = "browser")]
    pub timeout: Duration,
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            interval: DEFAULT_PING_INTERVAL,
            #[cfg(feature = "browser")]
            timeout: DEFAULT_PING_TIMEOUT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for the server's `InitialConnection` message. No Pings may be sent yet.
    Connecting,
    /// `InitialConnection` has arrived, so Pings may be sent.
    Established,
    /// The WebSocket has closed, or is closing.
    Closed,
}

type PendingPing = oneshot::Sender<crate::Result<PingResult>>;

struct PingState {
    phase: Phase,
    tracker: NetworkStatsTracker,
    /// On-demand Pings sent by [`PingHandle::ping`] and awaiting their Pong, by `request_id`.
    pending: HashMap<u32, PendingPing>,
}

/// The monotonic clock which Pings' `client_send_time` is measured on:
/// microseconds since an arbitrary per-connection epoch.
#[derive(Clone, Copy, Debug)]
struct Clock {
    epoch: Instant,
}

impl Clock {
    fn micros(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.epoch).as_micros() as u64
    }

    fn now_micros(&self) -> u64 {
        self.micros(Instant::now())
    }

    fn instant(&self, micros: u64) -> Instant {
        self.epoch + Duration::from_micros(micros)
    }
}

/// Requests from [`PingHandle::ping`] for the WebSocket loop to send an on-demand Ping,
/// each to be passed to [`PingDriver::on_request`].
pub(crate) type PingRequests = mpsc::UnboundedReceiver<u32>;

/// Create a connection's Ping plumbing:
/// the connection's handle, and the WebSocket loop's driver and request stream.
pub(crate) fn new(config: PingConfig) -> (PingHandle, PingDriver, PingRequests) {
    let state = Arc::new(Mutex::new(PingState {
        phase: Phase::Connecting,
        tracker: NetworkStatsTracker::default(),
        pending: HashMap::default(),
    }));
    let clock = Clock { epoch: Instant::now() };
    let (requests_send, requests_recv) = mpsc::unbounded();
    let handle = PingHandle {
        state: Arc::clone(&state),
        clock,
        requests: requests_send,
    };
    let driver = PingDriver { state, clock, config };
    (handle, driver, requests_recv)
}

/// The connection's side of the Ping plumbing.
#[derive(Clone)]
pub(crate) struct PingHandle {
    state: Arc<Mutex<PingState>>,
    clock: Clock,
    /// Asks the WebSocket loop to send an on-demand Ping with the given `request_id`.
    requests: mpsc::UnboundedSender<u32>,
}

impl PingHandle {
    /// A fresh snapshot of the network statistics, or `None` before the first Pong.
    pub(crate) fn network_stats(&self) -> Option<NetworkStats> {
        let now_us = self.clock.now_micros();
        self.state.lock().unwrap().tracker.snapshot(now_us)
    }

    /// Send a Ping now, resolving with its timings once the Pong arrives.
    ///
    /// The Ping is sent eagerly, when this method is called, rather than when the future is first polled.
    pub(crate) fn ping(&self) -> impl Future<Output = crate::Result<PingResult>> + Send + 'static {
        let pong = self.start_ping();
        async move {
            // The sender is dropped without a reply only if the WebSocket loop has gone away.
            pong?.await.unwrap_or(Err(crate::Error::Disconnected))
        }
    }

    fn start_ping(&self) -> crate::Result<oneshot::Receiver<crate::Result<PingResult>>> {
        let mut state = self.state.lock().unwrap();
        match state.phase {
            Phase::Connecting => return Err(crate::Error::NotYetConnected),
            Phase::Closed => return Err(crate::Error::Disconnected),
            Phase::Established => {}
        }
        let request_id = next_request_id();
        self.requests
            .unbounded_send(request_id)
            .map_err(|_| crate::Error::Disconnected)?;
        let (send, recv) = oneshot::channel();
        state.pending.insert(request_id, send);
        Ok(recv)
    }
}

/// The WebSocket loop's side of the Ping plumbing.
pub(crate) struct PingDriver {
    state: Arc<Mutex<PingState>>,
    clock: Clock,
    config: PingConfig,
}

impl PingDriver {
    /// The interval between periodic Pings, or `None` if periodic pinging is disabled.
    pub(crate) fn interval(&self) -> Option<Duration> {
        Some(self.config.interval).filter(|interval| !interval.is_zero())
    }

    /// Called when `InitialConnection` arrives.
    /// Returns the first periodic Ping to send, if periodic pinging is enabled.
    pub(crate) fn on_initial_connection(&self) -> Option<ws::v2::ClientMessage> {
        let mut state = self.state.lock().unwrap();
        if state.phase != Phase::Connecting {
            return None;
        }
        state.phase = Phase::Established;
        self.interval()?;
        Some(self.stamp_ping(&mut state, next_request_id()))
    }

    /// Called on each tick of the periodic Ping timer.
    /// Returns the Ping to send, if Pings may be sent yet.
    pub(crate) fn on_tick(&self) -> Option<ws::v2::ClientMessage> {
        let mut state = self.state.lock().unwrap();
        (state.phase == Phase::Established).then(|| self.stamp_ping(&mut state, next_request_id()))
    }

    /// Called with each of the [`PingRequests`] from [`PingHandle::ping`].
    /// Returns the Ping to send, or fails the request if the connection is closing.
    pub(crate) fn on_request(&self, request_id: u32) -> Option<ws::v2::ClientMessage> {
        let mut state = self.state.lock().unwrap();
        if state.phase == Phase::Established {
            Some(self.stamp_ping(&mut state, request_id))
        } else {
            if let Some(pending) = state.pending.remove(&request_id) {
                let _ = pending.send(Err(crate::Error::Disconnected));
            }
            None
        }
    }

    /// Whether Pings have gone unanswered for so long that the connection should be treated as lost,
    /// returning the error to disconnect with if so.
    ///
    /// Call on each tick of the periodic Ping timer, before [`Self::on_tick`].
    /// A silently failed network leaves the socket open with nothing arriving,
    /// so the only sign is Pings going unanswered.
    #[cfg(feature = "browser")]
    pub(crate) fn check_unresponsive(&self) -> Option<crate::Error> {
        let state = self.state.lock().unwrap();
        let now_us = self.clock.now_micros();
        state
            .tracker
            .is_unresponsive(now_us, self.config.timeout)
            .then(|| crate::Error::ConnectionUnresponsive {
                waited: Duration::from_micros(state.tracker.pong_wait_us(now_us)),
            })
    }

    /// Record a Pong which arrived at `received_at`,
    /// and resolve the on-demand Ping it answers, if any.
    pub(crate) fn on_pong(&self, received_at: Instant, pong: ws::v2::Pong) {
        let ws::v2::Pong {
            request_id,
            client_send_time,
            server_receive_time,
            server_hold_duration,
        } = pong;
        let now_us = self.clock.micros(received_at);
        let server_hold = server_hold_duration.to_duration_saturating();

        let mut state = self.state.lock().unwrap();
        state.tracker.record_pong(
            now_us,
            client_send_time,
            server_receive_time.to_micros_since_unix_epoch(),
            server_hold_duration.to_micros(),
        );
        if let Some(pending) = state.pending.remove(&request_id) {
            let round_trip = Duration::from_micros(now_us.saturating_sub(client_send_time));
            let _ = pending.send(Ok(PingResult {
                rtt: round_trip.saturating_sub(server_hold),
                round_trip,
                server_hold,
                server_receive_time,
                sent_at: self.clock.instant(client_send_time),
                received_at: self.clock.instant(now_us),
            }));
        }
    }

    /// Called when the WebSocket closes, or starts to close.
    ///
    /// Network stats describe one socket, so this resets them,
    /// and fails every on-demand Ping still waiting for its Pong.
    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.phase = Phase::Closed;
        state.tracker.reset();
        for (_, pending) in state.pending.drain() {
            let _ = pending.send(Err(crate::Error::Disconnected));
        }
    }

    // Build a Ping stamped with the current time, and record it as sent.
    // The caller must send it straight away.
    fn stamp_ping(&self, state: &mut PingState, request_id: u32) -> ws::v2::ClientMessage {
        let client_send_time = self.clock.now_micros();
        state.tracker.record_ping(client_send_time);
        log::trace!("Sending ping: request_id={request_id} client_send_time={client_send_time}");
        ws::v2::ClientMessage::Ping(ws::v2::Ping {
            request_id,
            client_send_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, StreamExt};
    use spacetimedb_lib::{TimeDuration, Timestamp};

    fn ping_parts(msg: ws::v2::ClientMessage) -> ws::v2::Ping {
        match msg {
            ws::v2::ClientMessage::Ping(ping) => ping,
            _ => panic!("Expected a Ping"),
        }
    }

    fn pong_for(ping: ws::v2::Ping, server_hold_micros: i64) -> ws::v2::Pong {
        ws::v2::Pong {
            request_id: ping.request_id,
            client_send_time: ping.client_send_time,
            server_receive_time: Timestamp::from_micros_since_unix_epoch(1_700_000_000_000_000),
            server_hold_duration: TimeDuration::from_micros(server_hold_micros),
        }
    }

    #[test]
    fn sends_nothing_before_initial_connection() {
        let (handle, driver, _requests) = new(PingConfig::default());
        assert!(driver.on_tick().is_none());
        assert!(matches!(
            handle.ping().now_or_never(),
            Some(Err(crate::Error::NotYetConnected))
        ));
    }

    #[test]
    fn sends_a_ping_on_initial_connection_and_each_tick() {
        let (handle, driver, _requests) = new(PingConfig::default());
        let first = ping_parts(driver.on_initial_connection().unwrap());
        let second = ping_parts(driver.on_tick().unwrap());
        assert_ne!(first.request_id, second.request_id);
        assert!(second.client_send_time >= first.client_send_time);
        // No Pong yet, so no stats.
        assert_eq!(handle.network_stats(), None);
    }

    #[test]
    fn interval_zero_disables_periodic_pings_but_not_ping() {
        let config = PingConfig {
            interval: Duration::ZERO,
            #[cfg(feature = "browser")]
            timeout: DEFAULT_PING_TIMEOUT,
        };
        let (handle, driver, mut requests) = new(config);
        assert_eq!(driver.interval(), None);
        assert!(driver.on_initial_connection().is_none());

        let result = handle.ping();
        let request_id = requests.next().now_or_never().flatten().unwrap();
        let ping = ping_parts(driver.on_request(request_id).unwrap());
        assert_eq!(ping.request_id, request_id);
        driver.on_pong(Instant::now(), pong_for(ping, 0));
        assert!(result.now_or_never().unwrap().is_ok());
        assert!(handle.network_stats().is_some());
    }

    #[test]
    fn ping_resolves_from_its_own_pong() {
        let (handle, driver, mut requests) = new(PingConfig::default());
        driver.on_initial_connection();

        let result = handle.ping();
        let request_id = requests.next().now_or_never().flatten().unwrap();
        let ping = ping_parts(driver.on_request(request_id).unwrap());
        let received_at = Instant::now() + Duration::from_millis(50);
        driver.on_pong(received_at, pong_for(ping, 20_000));

        let result = result.now_or_never().unwrap().unwrap();
        assert_eq!(result.server_hold, Duration::from_millis(20));
        assert_eq!(result.received_at - result.sent_at, result.round_trip);
        assert_eq!(result.rtt, result.round_trip - result.server_hold);
        assert!(result.round_trip >= Duration::from_millis(50));
        assert_eq!(
            result.server_receive_time,
            Timestamp::from_micros_since_unix_epoch(1_700_000_000_000_000)
        );
        // The sample feeds the stats too.
        assert_eq!(handle.network_stats().unwrap().rtt_latest, result.rtt);
    }

    #[test]
    fn close_fails_pending_pings_and_resets_the_stats() {
        let (handle, driver, mut requests) = new(PingConfig::default());
        let first = ping_parts(driver.on_initial_connection().unwrap());
        driver.on_pong(Instant::now(), pong_for(first, 0));
        assert!(handle.network_stats().is_some());

        let result = handle.ping();
        driver.close();
        assert!(matches!(result.now_or_never(), Some(Err(crate::Error::Disconnected))));
        assert_eq!(handle.network_stats(), None);

        // A request already in flight when the connection closed fails too.
        let request_id = requests.next().now_or_never().flatten().unwrap();
        assert!(driver.on_request(request_id).is_none());
        assert!(driver.on_tick().is_none());
        assert!(matches!(
            handle.ping().now_or_never(),
            Some(Err(crate::Error::Disconnected))
        ));
    }
}
