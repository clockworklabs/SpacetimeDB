//! Answering the protocol-level [`ws_v2::Ping`] message.
//!
//! Not to be confused with WebSocket `Ping` and `Pong` frames,
//! which the server sends for connection keep-alive.

use std::sync::Arc;

use futures::{Stream, StreamExt};
use spacetimedb_client_api_messages::websocket::v2 as ws_v2;
use spacetimedb_lib::{bsatn, Timestamp};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{ActorState, UnorderedWsMessage, WsError, WsMessage, WsVersion};

/// Stream that answers lone [`ws_v2::Ping`]s as soon as they are read from `ws`,
/// and passes every other message through.
///
/// Client messages are otherwise handled one at a time, and the handler runs
/// e.g. `Subscribe` and `OneOffQuery` to completion before moving on.
/// A `Ping` waiting behind them would measure that work rather than the
/// connection, so we intercept it before [`super::ws_recv_queue`] and reply via
/// `unordered_tx`.
///
/// Pings batched with other messages in one v3 payload pass through,
/// to be answered in order by the message handler.
/// Protocol v1 has no `Ping` message, so v1 messages always pass through.
pub(super) fn ws_answer_pings(
    state: Arc<ActorState>,
    unordered_tx: mpsc::UnboundedSender<UnorderedWsMessage>,
    ws_version: WsVersion,
    ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin,
) -> impl Stream<Item = Result<WsMessage, WsError>> + Unpin {
    ws.filter(move |item| {
        let ping = match item {
            Ok(WsMessage::Binary(payload)) if ws_version != WsVersion::V1 && !state.closed() => {
                decode_lone_ping(payload)
            }
            _ => None,
        };
        let Some(ping) = ping else {
            return futures::future::ready(true);
        };
        // The ping never reaches `ws_recv_loop`, so record the activity here.
        state.record_activity();
        // If the send task has exited, the connection is going away,
        // and there is nobody left to reply to.
        let _ = unordered_tx.send(PendingPong::received(ping).into());
        futures::future::ready(false)
    })
}

/// Upper bound on the encoded size of a lone [`ws_v2::ClientMessage::Ping`].
///
/// [`decode_lone_ping`] ignores larger payloads without decoding them,
/// so only small messages are ever decoded twice.
const MAX_PING_PAYLOAD_SIZE: usize = 32;

/// Decode `payload` as a [`ws_v2::Ping`], if that is all it contains.
fn decode_lone_ping(payload: &[u8]) -> Option<ws_v2::Ping> {
    if payload.len() > MAX_PING_PAYLOAD_SIZE {
        return None;
    }
    let mut remaining = payload;
    match bsatn::from_reader::<ws_v2::ClientMessage>(&mut remaining) {
        Ok(ws_v2::ClientMessage::Ping(ping)) if remaining.is_empty() => Some(ping),
        _ => None,
    }
}

/// A [`ws_v2::Ping`] which has been read from the socket,
/// and is waiting for its [`ws_v2::Pong`] to be encoded.
#[derive(Debug)]
pub(super) struct PendingPong {
    ping: ws_v2::Ping,
    received_at: Instant,
    server_receive_time: Timestamp,
}

impl PendingPong {
    fn received(ping: ws_v2::Ping) -> Self {
        Self {
            ping,
            received_at: Instant::now(),
            server_receive_time: Timestamp::now(),
        }
    }

    /// Build the reply, counting the time since the ping was read as held by the server.
    ///
    /// Call this as late as possible before encoding, so the hold duration is accurate.
    pub(super) fn into_pong(self) -> ws_v2::Pong {
        ws_v2::Pong {
            request_id: self.ping.request_id,
            client_send_time: self.ping.client_send_time,
            server_receive_time: self.server_receive_time,
            server_hold_duration: self.received_at.elapsed().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::stream;
    use pretty_assertions::assert_matches;

    use super::super::tests::dummy_actor_state;
    use super::super::v2_outbound_message;
    use super::*;

    fn encoded_ping(request_id: u32, client_send_time: u64) -> Vec<u8> {
        bsatn::to_vec(&ws_v2::ClientMessage::Ping(ws_v2::Ping {
            request_id,
            client_send_time,
        }))
        .unwrap()
    }

    #[test]
    fn lone_ping_fits_in_max_ping_payload_size() {
        assert!(encoded_ping(u32::MAX, u64::MAX).len() <= MAX_PING_PAYLOAD_SIZE);
    }

    #[tokio::test]
    async fn answer_pings_replies_to_lone_pings_and_passes_other_messages_through() {
        let state = Arc::new(dummy_actor_state());
        let (unordered_tx, mut unordered_rx) = mpsc::unbounded_channel();
        let batched_pings = [encoded_ping(2, 2), encoded_ping(3, 3)].concat();
        let input = stream::iter([
            Ok::<_, WsError>(WsMessage::Binary(encoded_ping(1, 1).into())),
            Ok(WsMessage::Binary(batched_pings.into())),
            Ok(WsMessage::text("hello")),
        ]);

        let passed = ws_answer_pings(state, unordered_tx, WsVersion::V3, input)
            .collect::<Vec<_>>()
            .await;

        assert_matches!(&passed[..], [Ok(WsMessage::Binary(_)), Ok(WsMessage::Text(_))]);
        assert_matches!(
            unordered_rx.try_recv(),
            Ok(UnorderedWsMessage::Pong(PendingPong {
                ping: ws_v2::Ping {
                    request_id: 1,
                    client_send_time: 1
                },
                ..
            }))
        );
        assert_matches!(unordered_rx.try_recv(), Err(_));
    }

    #[tokio::test]
    async fn answer_pings_passes_pings_through_on_v1_or_when_closed() {
        for (version, closed) in [(WsVersion::V1, false), (WsVersion::V3, true)] {
            let state = Arc::new(dummy_actor_state());
            if closed {
                state.close();
            }
            let (unordered_tx, mut unordered_rx) = mpsc::unbounded_channel();
            let input = stream::iter([Ok::<_, WsError>(WsMessage::Binary(encoded_ping(1, 1).into()))]);

            let passed = ws_answer_pings(state, unordered_tx, version, input)
                .collect::<Vec<_>>()
                .await;

            assert_eq!(passed.len(), 1);
            assert_matches!(unordered_rx.try_recv(), Err(_));
        }
    }

    #[tokio::test(start_paused = true)] // see [NOTE: start_paused]
    async fn pending_pong_counts_time_until_encoding_as_held() {
        let pong = PendingPong::received(ws_v2::Ping {
            request_id: 3,
            client_send_time: 7,
        });
        let server_receive_time = pong.server_receive_time;
        tokio::time::advance(Duration::from_millis(5)).await;

        let message = v2_outbound_message(pong.into()).unwrap();

        assert_eq!(message.num_rows, None);
        assert_matches!(
            message.message,
            ws_v2::ServerMessage::Pong(pong)
                if pong.request_id == 3
                    && pong.client_send_time == 7
                    && pong.server_receive_time == server_receive_time
                    && pong.server_hold_duration.to_duration_abs() >= Duration::from_millis(5)
        );
    }
}
