use futures::{prelude::*, stream::SplitStream, SinkExt};
use hyper::upgrade::Upgraded;
use prost::Message;
use tokio::spawn;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Message as WebSocketMessage};
use tokio_tungstenite::WebSocketStream;

use crate::nodes::control_node::budget_controller::refresh_all_budget_allocations;
use crate::nodes::control_node::{budget_controller, controller};
use spacetimedb::hash::Hash;
use spacetimedb::protobuf::control_worker_api::{control_bound_message, ControlBoundMessage, WorkerBudgetSpend};

use super::worker_connection_index::WORKER_CONNECTION_INDEX;

#[derive(Debug)]
struct SendCommand {
    message: WebSocketMessage,
    ostx: oneshot::Sender<Result<(), tokio_tungstenite::tungstenite::Error>>,
}

#[derive(Clone, Debug)]
pub struct WorkerConnectionSender {
    _id: u64,
    sendtx: mpsc::Sender<SendCommand>,
}

impl WorkerConnectionSender {
    pub async fn send(self, message: WebSocketMessage) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        // TODO: It's tricky to avoid allocation here because of multithreading,
        // but maybe we can do that in the future with a custom allocator or a
        // buffer pool or something
        // We could also maybe use read-write locks to pass the data across
        // We could also not do multithreading or do something more intelligent
        // I'm sure there's something out there
        let (ostx, osrx) = tokio::sync::oneshot::channel::<Result<(), tokio_tungstenite::tungstenite::Error>>();
        self.sendtx.send(SendCommand { message, ostx }).await.unwrap();
        osrx.await.unwrap()
    }

    pub async fn _send_message_warn_fail(self, message: WebSocketMessage) {
        let id = self._id;
        if let Err(error) = self.send(message).await {
            log::warn!("Message send failed for worker {:?}: {}", id, error);
        }
    }

    // Waits for the close frame to be sent, but not for the connection to be closed
    // Once the client sends a close frame back, the connection will be closed
    // and the client will be removed from the GCI
    pub async fn _close_warn_fail(self, close_frame: Option<CloseFrame<'_>>) {
        let close_frame = close_frame.map(|msg| msg.into_owned());
        let id = self._id;
        if let Err(error) = self.send(WebSocketMessage::Close(close_frame)).await {
            log::warn!("Failed to send close frame for worker {:?}: {}", id, error);
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

pub struct WorkerConnection {
    pub id: u64,
    pub alive: bool,
    stream: Option<SplitStream<WebSocketStream<Upgraded>>>,
    sendtx: mpsc::Sender<SendCommand>,
    read_handle: Option<JoinHandle<()>>,
}

impl WorkerConnection {
    pub fn new(id: u64, ws: WebSocketStream<Upgraded>) -> WorkerConnection {
        let (mut sink, stream) = ws.split();

        // Buffer up to 64 client messages
        let (sendtx, mut sendrx) = mpsc::channel::<SendCommand>(64);
        spawn(async move {
            // NOTE: This recv returns None if the channel is closed
            while let Some(command) = sendrx.recv().await {
                command.ostx.send(sink.send(command.message).await).unwrap();
            }
        });

        Self {
            id,
            alive: true,
            stream: Some(stream),
            sendtx,
            read_handle: None,
        }
    }

    pub fn sender(&self) -> WorkerConnectionSender {
        WorkerConnectionSender {
            _id: self.id,
            sendtx: self.sendtx.clone(),
        }
    }

    pub fn recv(&mut self) {
        let id = self.id;
        let mut stream = self.stream.take().unwrap();
        self.read_handle = Some(spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(WebSocketMessage::Text(_)) => {
                        log::debug!("Text not supported for worker API. Drop worker.");
                        break;
                    }
                    Ok(WebSocketMessage::Binary(message_buf)) => {
                        if let Err(e) = Self::on_binary(id, message_buf).await {
                            log::debug!("Worker caused error on binary message: {}", e);
                            break;
                        }
                    }
                    Ok(WebSocketMessage::Ping(_message)) => {
                        log::trace!("Received ping from worker {}", id);
                    }
                    Ok(WebSocketMessage::Pong(_message)) => {
                        log::trace!("Received heartbeat from worker {}", id);
                        let mut wci = WORKER_CONNECTION_INDEX.lock().unwrap();
                        match wci.get_client_mut(&id) {
                            Some(client) => client.alive = true,
                            None => log::warn!("Received heartbeat from missing worker {}", id), // Oh well, client must be gone.
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
                    Ok(WebSocketMessage::Frame(_)) => {
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
            log::debug!("Worker connection ended");
            // The stream is ended by tokio-tungsten when we receive a `ConnectionClosed` error.
            // That's not actually an error, but rather a notification saying that the handshake has been
            // completed. At this point it's safe to drop the underlying connection.
            {
                let mut cai = WORKER_CONNECTION_INDEX.lock().unwrap();
                cai.drop_client(&id);
            }
            // NOTE: we sign the player in before recv so we'll sign the player out here
            // let player_id = actor_id;
            // GameClient::sign_out(player_id).await;
        }));
    }

    async fn on_binary(worker_node_id: u64, message_buf: Vec<u8>) -> Result<(), anyhow::Error> {
        let message = ControlBoundMessage::decode(&message_buf[..]);
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                log::warn!("Worker node sent poorly formed message: {}", error);
                return Err(anyhow::anyhow!("{:?}", error));
            }
        };
        let message = match message.r#type {
            Some(value) => value,
            None => {
                log::warn!("Worker node sent a message with no type");
                return Err(anyhow::anyhow!("Control node sent a message with no type"));
            }
        };
        match message {
            control_bound_message::Type::WorkerBudgetSpend(node_budget_update) => {
                match on_budget_spend_update(worker_node_id, node_budget_update).await {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Error while updating budget status from node {}: {}", worker_node_id, e);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Drop for WorkerConnection {
    fn drop(&mut self) {
        if let Some(read_handle) = self.read_handle.take() {
            read_handle.abort();
        }
        let id = self.id;
        spawn(async move {
            controller::node_disconnected(id).await.unwrap();
        });
        log::trace!("Worker connection {} dropped", self.id);
    }
}

async fn on_budget_spend_update(node_id: u64, node_budget_update: WorkerBudgetSpend) -> Result<(), anyhow::Error> {
    for spend in node_budget_update.identity_spend {
        budget_controller::node_energy_spend_update(
            node_id,
            &Hash::from_slice(spend.identity.as_slice()),
            spend.spend,
        )?;
    }

    // Now redo all budget allocations
    refresh_all_budget_allocations().await;
    Ok(())
}
