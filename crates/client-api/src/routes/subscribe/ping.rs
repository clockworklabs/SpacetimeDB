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

/// Stream that answers every [`ws_v2::Ping`] as soon as it is read from `ws`,
/// and passes every other message through.
///
/// Client messages otherwise wait in [`super::ws_recv_queue`] to be handled one
/// at a time, and replies go out through the client's ordered outbound channel.
/// A `Pong` answered there would include both waits, which the server can't
/// report accurately as hold time, so we intercept the `Ping` before the queue
/// and reply via `unordered_tx`.
///
/// A v3 payload may batch a `Ping` with other messages. Its `Ping`s are answered
/// here, possibly before the replies to the messages ahead of them, and the
/// payload passes through unchanged for the handler, which ignores `Ping`s.
/// A payload holding nothing but `Ping`s is dropped once they are answered.
/// A payload which fails to decode passes through with none of its `Ping`s
/// answered, so that the handler reports the error.
/// Protocol v1 has no `Ping` message, so v1 messages always pass through.
pub(super) fn ws_answer_pings(
    state: Arc<ActorState>,
    unordered_tx: mpsc::UnboundedSender<UnorderedWsMessage>,
    ws_version: WsVersion,
    ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin,
) -> impl Stream<Item = Result<WsMessage, WsError>> + Unpin {
    ws.filter(move |item| {
        let found = match item {
            Ok(WsMessage::Binary(payload)) if !state.closed() => find_pings(ws_version, payload),
            _ => None,
        };
        let Some(FoundPings { pings, only_pings }) = found else {
            return futures::future::ready(true);
        };
        for ping in pings {
            // If the send task has exited, the connection is going away,
            // and there is nobody left to reply to.
            let _ = unordered_tx.send(PendingPong::received(ping).into());
        }
        if only_pings {
            // The payload never reaches `ws_recv_loop`, so record the activity here.
            state.record_activity();
        }
        futures::future::ready(!only_pings)
    })
}

/// The [`ws_v2::Ping`]s in a client payload.
#[derive(Debug)]
struct FoundPings {
    pings: Vec<ws_v2::Ping>,
    /// Whether the payload holds nothing but `pings`,
    /// leaving the message handler nothing to do.
    only_pings: bool,
}

/// The BSATN tag of [`ws_v2::ClientMessage::Ping`].
const PING_TAG: u8 = 6;

/// Find the [`ws_v2::Ping`]s in `payload`.
///
/// Returns `None` if there are none, or if `payload` fails to decode.
fn find_pings(ws_version: WsVersion, payload: &[u8]) -> Option<FoundPings> {
    match ws_version {
        WsVersion::V1 => None,
        // One message per payload, so the tag tells us whether to decode it.
        WsVersion::V2 if payload.first() != Some(&PING_TAG) => None,
        WsVersion::V2 => match bsatn::from_slice(payload).ok()? {
            ws_v2::ClientMessage::Ping(ping) => Some(FoundPings {
                pings: vec![ping],
                only_pings: true,
            }),
            _ => None,
        },
        // v3 messages are not length-prefixed,
        // so finding the pings means decoding every message in the payload.
        WsVersion::V3 => {
            let mut remaining = payload;
            let mut pings = Vec::new();
            let mut only_pings = true;
            while !remaining.is_empty() {
                match bsatn::from_reader::<ws_v2::ClientMessage>(&mut remaining).ok()? {
                    ws_v2::ClientMessage::Ping(ping) => pings.push(ping),
                    _ => only_pings = false,
                }
            }
            (!pings.is_empty()).then_some(FoundPings { pings, only_pings })
        }
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

    fn encoded_query(request_id: u32) -> Vec<u8> {
        bsatn::to_vec(&ws_v2::ClientMessage::OneOffQuery(ws_v2::OneOffQuery {
            request_id,
            query_string: "SELECT * FROM t".into(),
        }))
        .unwrap()
    }

    /// The request ids of the pings found, and whether the payload held only pings.
    fn found(found: Option<FoundPings>) -> Option<(Vec<u32>, bool)> {
        found.map(|f| (f.pings.iter().map(|p| p.request_id).collect(), f.only_pings))
    }

    #[test]
    fn ping_tag_matches_encoding() {
        assert_eq!(encoded_ping(1, 1)[0], PING_TAG);
    }

    #[test]
    fn find_pings_on_v2_finds_only_a_lone_ping() {
        let v2 = |payload: &[u8]| found(find_pings(WsVersion::V2, payload));
        assert_eq!(v2(&encoded_ping(1, 1)), Some((vec![1], true)));
        assert_eq!(v2(&encoded_query(1)), None);
        // v2 is one message per payload. Like the message handler,
        // decode the first and ignore anything after it.
        assert_eq!(
            v2(&[encoded_ping(1, 1), encoded_ping(2, 2)].concat()),
            Some((vec![1], true))
        );
        assert_eq!(v2(&[encoded_query(1), encoded_ping(2, 2)].concat()), None);
        assert_eq!(v2(&[]), None);
    }

    #[test]
    fn find_pings_on_v3_finds_every_ping() {
        let v3 = |parts: &[Vec<u8>]| found(find_pings(WsVersion::V3, &parts.concat()));
        assert_eq!(v3(&[encoded_ping(1, 1)]), Some((vec![1], true)));
        assert_eq!(v3(&[encoded_ping(1, 1), encoded_ping(2, 2)]), Some((vec![1, 2], true)));
        assert_eq!(
            v3(&[
                encoded_query(1),
                encoded_ping(2, 2),
                encoded_query(3),
                encoded_ping(4, 4)
            ]),
            Some((vec![2, 4], false))
        );
        assert_eq!(v3(&[encoded_query(1)]), None);
        assert_eq!(v3(&[]), None);
    }

    #[test]
    fn find_pings_finds_none_in_a_malformed_payload() {
        let truncated = encoded_query(2);
        let truncated = truncated[..truncated.len() - 1].to_vec();
        let payload = [encoded_ping(1, 1), truncated].concat();
        assert!(find_pings(WsVersion::V3, &payload).is_none());
        assert!(find_pings(WsVersion::V3, &[encoded_ping(1, 1), vec![0xff]].concat()).is_none());
    }

    #[tokio::test]
    async fn answer_pings_replies_to_every_ping_and_passes_other_messages_through() {
        let state = Arc::new(dummy_actor_state());
        let (unordered_tx, mut unordered_rx) = mpsc::unbounded_channel();
        let mixed = [encoded_query(2), encoded_ping(3, 3), encoded_ping(4, 4)].concat();
        let malformed = [encoded_ping(5, 5), vec![0xff]].concat();
        let input = stream::iter([
            Ok::<_, WsError>(WsMessage::Binary(encoded_ping(1, 1).into())),
            Ok(WsMessage::Binary(mixed.clone().into())),
            Ok(WsMessage::Binary(malformed.clone().into())),
            Ok(WsMessage::text("hello")),
        ]);

        let passed = ws_answer_pings(state, unordered_tx, WsVersion::V3, input)
            .collect::<Vec<_>>()
            .await;

        assert_matches!(
            &passed[..],
            [Ok(WsMessage::Binary(a)), Ok(WsMessage::Binary(b)), Ok(WsMessage::Text(_))]
                if a[..] == mixed[..] && b[..] == malformed[..]
        );
        let answered = std::iter::from_fn(|| match unordered_rx.try_recv() {
            Ok(UnorderedWsMessage::Pong(pong)) => Some(pong.ping.request_id),
            Ok(other) => panic!("unexpected message: {other:?}"),
            Err(_) => None,
        })
        .collect::<Vec<_>>();
        assert_eq!(answered, [1, 3, 4]);
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
