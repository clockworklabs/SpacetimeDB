use std::mem;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::TypedHeader;
use futures::{SinkExt, StreamExt};
use http::{HeaderValue, StatusCode};
use serde::Deserialize;
use spacetimedb::client::messages::{IdentityTokenMessage, ServerMessage};
use spacetimedb::client::{ClientActorId, ClientClosed, ClientConnection, DataMessage, MessageHandleError, Protocol};
use spacetimedb::host::NoSuchModule;
use spacetimedb::util::future_queue;
use tokio::sync::mpsc;

use crate::auth::{SpacetimeAuthHeader, SpacetimeIdentity, SpacetimeIdentityToken};
use crate::util::websocket::{
    CloseCode, CloseFrame, Message as WsMessage, WebSocketConfig, WebSocketStream, WebSocketUpgrade,
};
use crate::util::{NameOrAddress, XForwardedFor};
use crate::{log_and_500, WorkerCtx};

#[allow(clippy::declare_interior_mutable_const)]
pub const TEXT_PROTOCOL: HeaderValue = HeaderValue::from_static("v1.text.spacetimedb");
#[allow(clippy::declare_interior_mutable_const)]
pub const BIN_PROTOCOL: HeaderValue = HeaderValue::from_static("v1.bin.spacetimedb");

#[derive(Deserialize)]
pub struct SubscribeParams {
    pub name_or_address: NameOrAddress,
}

pub async fn handle_websocket(
    State(worker_ctx): State<Arc<dyn WorkerCtx>>,
    Path(SubscribeParams { name_or_address }): Path<SubscribeParams>,
    forwarded_for: Option<TypedHeader<XForwardedFor>>,
    auth: SpacetimeAuthHeader,
    ws: WebSocketUpgrade,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth.get_or_create(&*worker_ctx).await?;

    let address = name_or_address.resolve(&*worker_ctx).await?.into();

    let (res, ws_upgrade, protocol) =
        ws.select_protocol([(BIN_PROTOCOL, Protocol::Binary), (TEXT_PROTOCOL, Protocol::Text)]);

    let protocol = protocol.ok_or((StatusCode::BAD_REQUEST, "no valid protocol selected"))?;

    // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
    // to connect to multiple modules
    let database = worker_ctx
        .get_database_by_address(&address)
        .await
        .unwrap()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let database_instance = worker_ctx
        .get_leader_database_instance_by_database(database.id)
        .await
        .ok_or(StatusCode::BAD_REQUEST)?;
    let instance_id = database_instance.id;

    let identity_token = auth.creds.token().to_owned();

    let host = worker_ctx.host_controller();
    let module = match host.get_module_host(instance_id) {
        Ok(m) => m,
        Err(_) => {
            let dbic = worker_ctx
                .load_module_host_context(database, instance_id)
                .await
                .map_err(log_and_500)?;
            host.spawn_module_host(dbic).await.map_err(log_and_500)?
        }
    };

    let client_id = ClientActorId {
        identity: auth.identity,
        name: worker_ctx.client_actor_index().next_client_name(),
    };

    let ws_config = WebSocketConfig {
        max_send_queue: None,
        max_message_size: Some(0x2000000),
        max_frame_size: None,
        accept_unmasked_frames: false,
    };

    tokio::spawn(async move {
        let ws = match ws_upgrade.upgrade(ws_config).await {
            Ok(ws) => ws,
            Err(err) => {
                log::error!("WebSocket init error: {}", err);
                return;
            }
        };

        match forwarded_for {
            Some(TypedHeader(XForwardedFor(ip))) => {
                log::debug!("New client connected from ip {}", ip)
            }
            None => log::debug!("New client connected from unknown ip"),
        }

        let actor = |client, sendrx| ws_client_actor(client, ws, sendrx);
        let client = match ClientConnection::spawn(client_id, protocol, instance_id, module, actor).await {
            Ok(s) => s,
            Err(NoSuchModule) => {
                // debug here should be fine because we *just* found a module, so this should be really rare
                log::warn!("ModuleHost died while we were connecting");
                return;
            }
        };

        // Send the client their identity token message as the first message
        // NOTE: We're adding this to the protocol because some client libraries are
        // unable to access the http response headers.
        // Clients that receive the token from the response headers should ignore this
        // message.
        let message = IdentityTokenMessage {
            identity: auth.identity,
            identity_token,
        };
        if let Err(ClientClosed) = client.send_message(message).await {
            log::warn!("client closed before identity token was sent")
        }
    });

    Ok((
        TypedHeader(SpacetimeIdentity(auth.identity)),
        TypedHeader(SpacetimeIdentityToken(auth.creds)),
        res,
    ))
}

const LIVELINESS_TIMEOUT: Duration = Duration::from_secs(60);

async fn ws_client_actor(client: ClientConnection, mut ws: WebSocketStream, mut sendrx: mpsc::Receiver<DataMessage>) {
    let mut liveness_check_interval = tokio::time::interval(LIVELINESS_TIMEOUT);
    let mut got_pong = true;
    // TODO: do we want this to have a fixed capacity? or should it be unbounded
    let mut handle_queue = pin!(future_queue(|message| client.handle_message(message)));
    let mut closed = false;
    loop {
        enum Item {
            Message(ClientMessage),
            HandleResult(Result<(), MessageHandleError>),
        }
        let message = tokio::select! {
            // NOTE: all of the futures for these branches **must** be cancel safe. do not
            //       change this if you don't know what that means.
            Some(res) = handle_queue.next() => Item::HandleResult(res),
            message = ws.next() => match message {
                Some(Ok(m)) => Item::Message(ClientMessage::from_message(m)),
                Some(Err(error)) => {
                    log::warn!("Websocket receive error: {}", error);
                    continue;
                }
                // the client sent us a close frame
                None => break,
            },
            Some(message) = sendrx.recv() => {
                if closed {
                    // TODO: this isn't great. when we receive a close request from the peer,
                    //       tungstenite doesn't let us send any new messages on the socket,
                    //       even though the websocket RFC allows it. should we fork tungstenite?
                    log::info!("dropping message due to ws already being closed: {message:?}");
                } else {
                    // TODO: I think we can be smarter about feeding messages here?
                    if let Err(error) = ws.send(datamsg_to_wsmsg(message)).await {
                        log::warn!("Websocket send error: {error}")
                    }
                }
                continue;
            }
            () = client.module.exited(), if !closed => {
                if let Err(e) = ws.close(Some(CloseFrame { code: CloseCode::Away, reason: "module exited".into() })).await {
                    log::warn!("error closing: {e:#}")
                }
                closed = true;
                continue;
            }
            _ = liveness_check_interval.tick() => {
                if mem::take(&mut got_pong) {
                    if let Err(e) = ws.send(WsMessage::Ping(Vec::new())).await {
                        log::warn!("error sending ping: {e:#}");
                    }
                    continue;
                } else {
                    // the client never responded to our ping; drop them without trying to send them a Close
                    log::warn!("client {} timed out", client.id);
                    break;
                }
            }
        };
        match message {
            Item::Message(ClientMessage::Message(message)) => handle_queue.as_mut().push(message),
            Item::HandleResult(res) => {
                if let Err(e) = res {
                    if let MessageHandleError::Execution(err) = e {
                        log::error!("{err:#}");
                        let msg = err.serialize(client.protocol);
                        if let Err(error) = ws.send(datamsg_to_wsmsg(msg)).await {
                            log::warn!("Websocket send error: {error}")
                        }
                        continue;
                    }
                    log::debug!("Client caused error on text message: {}", e);
                    if let Err(e) = ws
                        .close(Some(CloseFrame {
                            code: CloseCode::Error,
                            reason: format!("{e:#}").into(),
                        }))
                        .await
                    {
                        log::warn!("error closing websocket: {e:#}")
                    };
                }
            }
            Item::Message(ClientMessage::Ping(_message)) => {
                log::trace!("Received ping from client {}", client.id);
            }
            Item::Message(ClientMessage::Pong(_message)) => {
                log::trace!("Received heartbeat from client {}", client.id);
                got_pong = true;
            }
            Item::Message(ClientMessage::Close(close_frame)) => {
                // This happens in 2 cases:
                // a) We sent a Close frame and this is the ack.
                // b) This is the client telling us they want to close.
                // in either case, after the remaining messages in the queue flush,
                // ws.next() will return None and we'll exit the loop.
                // NOTE: No need to send a close frame, it's is queued
                //       automatically by tungstenite.

                // if this is the closed-by-them case, let the ClientConnectionSenders know now.
                sendrx.close();
                closed = true;
                log::trace!("Close frame {:?}", close_frame);
            }
        }
    }
    log::debug!("Client connection ended");
    sendrx.close();

    // ignore NoSuchModule; if the module's already closed, that's fine
    let _ = client.module.subscription().remove_subscriber(client.id);
    let _ = client
        .module
        .call_identity_connected_disconnected(client.id.identity, false)
        .await;
}

enum ClientMessage {
    Message(DataMessage),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame<'static>>),
}
impl ClientMessage {
    fn from_message(msg: WsMessage) -> Self {
        match msg {
            WsMessage::Text(s) => Self::Message(DataMessage::Text(s)),
            WsMessage::Binary(b) => Self::Message(DataMessage::Binary(b)),
            WsMessage::Ping(b) => Self::Ping(b),
            WsMessage::Pong(b) => Self::Pong(b),
            WsMessage::Close(frame) => Self::Close(frame),
            // WebSocket::read_message() never returns a raw Message::Frame
            WsMessage::Frame(_) => unreachable!(),
        }
    }
}

fn datamsg_to_wsmsg(msg: DataMessage) -> WsMessage {
    match msg {
        DataMessage::Text(text) => WsMessage::Text(text),
        DataMessage::Binary(bin) => WsMessage::Binary(bin),
    }
}
