use super::client_connection_index::CLIENT_ACTOR_INDEX;
use crate::address::Address;
use crate::client::ClientActorId;
use crate::hash::Hash;
use crate::host::host_controller;
use crate::host::ReducerArgs;
use crate::json::client_api::IdentityTokenJson;
use crate::json::client_api::MessageJson;
use crate::protobuf::client_api::IdentityToken;
use crate::protobuf::client_api::{message, Message};
use crate::worker_metrics::{WEBSOCKET_REQUESTS, WEBSOCKET_REQUEST_MSG_SIZE, WEBSOCKET_SENT, WEBSOCKET_SENT_MSG_SIZE};
use futures::{prelude::*, stream::SplitStream, SinkExt};
use hyper::upgrade::Upgraded;
use prost::bytes::Bytes;
use prost::Message as OtherMessage;
use tokio::spawn;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Message as WebSocketMessage};
use tokio_tungstenite::WebSocketStream;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Protocol {
    Text,
    Binary,
}

#[derive(Debug)]
struct SendCommand {
    message: WebSocketMessage,
    ostx: oneshot::Sender<Result<(), tokio_tungstenite::tungstenite::Error>>,
}

#[derive(Clone, Debug)]
pub struct ClientConnectionSender {
    pub id: ClientActorId,
    pub protocol: Protocol,
    sendtx: mpsc::Sender<SendCommand>,
}

impl ClientConnectionSender {
    pub async fn send(self, message: WebSocketMessage) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        let bytes_len = message.len();

        // TODO: It's tricky to avoid allocation here because of multithreading,
        // but maybe we can do that in the future with a custom allocator or a
        // buffer pool or something
        // We could also maybe use read-write locks to pass the data across
        // We could also not do multithreading or do something more intelligent
        // I'm sure there's something out there
        let (ostx, osrx) = tokio::sync::oneshot::channel::<Result<(), tokio_tungstenite::tungstenite::Error>>();
        self.sendtx
            .send(SendCommand { message, ostx })
            .await
            .expect("Unable to send SendCommand");
        let result = osrx.await.unwrap();

        WEBSOCKET_SENT
            .with_label_values(&[self.id.identity.to_hex().as_str()])
            .inc();

        WEBSOCKET_SENT_MSG_SIZE
            .with_label_values(&[self.id.identity.to_hex().as_str()])
            .observe(bytes_len as f64);

        result
    }

    pub async fn send_identity_token_message(self, identity: Hash, identity_token: String) {
        let message = if self.protocol == Protocol::Binary {
            let message = Message {
                r#type: Some(message::Type::IdentityToken(IdentityToken {
                    identity: identity.as_slice().to_vec(),
                    token: identity_token,
                })),
            };
            let mut buf = Vec::new();
            message.encode(&mut buf).unwrap();
            WebSocketMessage::Binary(buf)
        } else {
            let message = MessageJson::IdentityToken(IdentityTokenJson {
                identity: identity.to_hex(),
                token: identity_token,
            });
            let json = serde_json::to_string(&message).unwrap();
            WebSocketMessage::Text(json)
        };
        self.send_message_warn_fail(message).await;
    }

    pub async fn send_message_warn_fail(self, message: WebSocketMessage) {
        let id = self.id;
        if let Err(error) = self.send(message).await {
            log::warn!("Message send failed for client {:?}: {}", id, error);
        }
    }

    // Waits for the close frame to be sent, but not for the connection to be closed
    // Once the client sends a close frame back, the connection will be closed
    // and the client will be removed from the GCI
    pub async fn _close_warn_fail(self, close_frame: Option<CloseFrame<'_>>) {
        let close_frame = close_frame.map(|msg| msg.into_owned());
        let id = self.id;
        if let Err(error) = self.send(WebSocketMessage::Close(close_frame)).await {
            log::warn!("Failed to send close frame for client {:?}: {}", id, error);
        }
    }

    pub async fn _close_normally(self) {
        self._close_warn_fail(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "Connection closed by server.".into(),
        }))
        .await
    }
}

pub struct ClientConnection {
    pub id: ClientActorId,
    pub alive: bool,
    pub target_address: Address,
    pub database_instance_id: u64,
    pub protocol: Protocol,
    stream: Option<SplitStream<WebSocketStream<Upgraded>>>,
    sendtx: mpsc::Sender<SendCommand>,
    read_handle: Option<JoinHandle<()>>,
}

impl ClientConnection {
    pub fn new(
        id: ClientActorId,
        ws: WebSocketStream<Upgraded>,
        target_address: Address,
        protocol: Protocol,
        database_instance_id: u64,
    ) -> ClientConnection {
        let (mut sink, stream) = ws.split();

        // Buffer up to 64 client messages
        let (sendtx, mut sendrx) = mpsc::channel::<SendCommand>(64);
        spawn(async move {
            // NOTE: This recv returns None if the channel is closed
            while let Some(command) = sendrx.recv().await {
                command.ostx.send(sink.send(command.message).await).unwrap();
            }

            log::debug!("Dropped all senders to client websocket: {}", id);
        });

        Self {
            id,
            alive: true,
            target_address,
            database_instance_id,
            protocol,
            stream: Some(stream),
            sendtx,
            read_handle: None,
        }
    }

    pub fn sender(&self) -> ClientConnectionSender {
        ClientConnectionSender {
            id: self.id,
            sendtx: self.sendtx.clone(),
            protocol: self.protocol,
        }
    }

    pub fn recv(&mut self) {
        let id = self.id;
        let mut stream = self.stream.take().unwrap();
        let instance_id = self.database_instance_id;
        self.read_handle = Some(spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(WebSocketMessage::Text(message)) => {
                        if let Err(e) = Self::on_text(id, instance_id, message).await {
                            log::debug!("Client caused error on text message: {}", e);
                            break;
                        }
                    }
                    Ok(WebSocketMessage::Binary(message_buf)) => {
                        if let Err(e) = Self::on_binary(id, instance_id, message_buf).await {
                            log::debug!("Client caused error on binary message: {}", e);
                            break;
                        }
                    }
                    Ok(WebSocketMessage::Ping(_message)) => {
                        log::trace!("Received ping from client {}", id);
                    }
                    Ok(WebSocketMessage::Pong(_message)) => {
                        log::trace!("Received heartbeat from client {}", id);
                        let mut cai = CLIENT_ACTOR_INDEX.lock().unwrap();
                        match cai.get_client_mut(&id) {
                            Some(client) => client.alive = true,
                            None => log::warn!("Received heartbeat from missing client {}", id), // Oh well, client must be gone.
                        }
                    }
                    Ok(WebSocketMessage::Close(close_frame)) => {
                        // This can mean 1 of 2 things:
                        //
                        // 1. The client has sent an unsolicited close frame.
                        // This means the client wishes to close the connection
                        // and will send no further messages along the
                        // connection. Don't destroy the connection yet.
                        // Wait for the stream to end.
                        // NOTE: No need to send a close message, this is sent
                        // automatically by tungstenite.
                        //
                        // 2. We sent a close frame and the library is telling us about
                        // it. Very silly if you ask me.
                        // There's no need to do anything here, because we're the ones
                        // that sent the initial close. The close frame we just received
                        // was an acknowledgement by the client (their side of the handshake)
                        // Maybe check their close frame or something
                        log::trace!("Close frame {:?}", close_frame);
                    }
                    Ok(WebSocketMessage::Frame(_frame)) => {
                        // TODO: I don't know what this is for, since it's new
                        // I assume probably for sending large files?
                    }
                    Err(error) => match error {
                        tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                            // Do nothing. There's no need to listen to this error because
                            // according to the tungstenite documentation its really more of a
                            // notification anyway, and tokio-tungstenite will end the stream
                            // so we'll drop the websocket at after the while loop.
                        }
                        error => log::warn!("Websocket receive error: {}", error),
                    },
                }
            }
            log::debug!("Client connection ended");
            // The stream is ended by tokio-tungsten when we receive a `ConnectionClosed` error.
            // That's not actually an error, but rather a notification saying that the handshake has been
            // completed. At this point it's safe to drop the underlying connection.
            {
                let mut cai = CLIENT_ACTOR_INDEX.lock().unwrap();
                cai.drop_client(&id);
            }
        }));
    }

    async fn on_binary(client_id: ClientActorId, instance_id: u64, message_buf: Vec<u8>) -> Result<(), anyhow::Error> {
        WEBSOCKET_REQUEST_MSG_SIZE
            .with_label_values(&[format!("{}", instance_id).as_str(), "binary"])
            .observe(message_buf.len() as f64);

        WEBSOCKET_REQUESTS
            .with_label_values(&[format!("{}", instance_id).as_str(), "binary"])
            .inc();

        let message = Message::decode(Bytes::from(message_buf))?;
        match message.r#type {
            Some(message::Type::FunctionCall(f)) => {
                let reducer = f.reducer;
                let args = ReducerArgs::Json(f.arg_bytes.into());

                let host = host_controller::get_host();
                match host.call_reducer(instance_id, client_id.identity, &reducer, args).await {
                    Ok(Some(_)) => {}
                    Ok(None) => log::error!("reducer {reducer} not found"),
                    Err(e) => {
                        log::error!("{}", e)
                    }
                }

                Ok(())
            }
            Some(_) => Err(anyhow::anyhow!("Unexpected client message type.")),
            None => Err(anyhow::anyhow!("No message from client")),
        }
    }

    async fn on_text(client_id: ClientActorId, instance_id: u64, message: String) -> Result<(), anyhow::Error> {
        WEBSOCKET_REQUEST_MSG_SIZE
            .with_label_values(&[format!("{}", instance_id).as_str(), "text"])
            .observe(message.len() as f64);

        WEBSOCKET_REQUESTS
            .with_label_values(&[format!("{}", instance_id).as_str(), "text"])
            .inc();

        #[derive(serde::Deserialize)]
        struct Call<'a> {
            #[serde(borrow, rename = "fn")]
            func: std::borrow::Cow<'a, str>,
            args: &'a serde_json::value::RawValue,
        }
        let bytes = Bytes::from(message);
        let Call { func, args } = serde_json::from_slice(&bytes)?;
        let args = ReducerArgs::Json(bytes.slice_ref(args.get().as_bytes()));

        let host = host_controller::get_host();
        match host.call_reducer(instance_id, client_id.identity, &func, args).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                log::error!("reducer {func} not found")
            }
            Err(e) => {
                log::error!("{}", e)
            }
        }

        Ok(())
    }
}

impl Drop for ClientConnection {
    fn drop(&mut self) {
        // Schedule removal of the module subscription for the future.
        let instance_id = self.database_instance_id;
        let client_id = self.id;

        spawn(async move {
            let host = host_controller::get_host();
            let module = host.get_module(instance_id);
            match module {
                Ok(module) => {
                    module
                        .call_identity_connected_disconnected(client_id.identity, false)
                        .await
                        .unwrap();
                    module
                        .remove_subscriber(client_id)
                        .await
                        .expect("Could not remove module subscription")
                }
                Err(e) => {
                    log::warn!(
                        "Could not find module {} to unsubscribe dropped connection {:?}",
                        instance_id,
                        e
                    )
                }
            }
        });
        if let Some(read_handle) = self.read_handle.take() {
            read_handle.abort();
        }
        log::trace!("Client {} dropped", self.id);
    }
}
