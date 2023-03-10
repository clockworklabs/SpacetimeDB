use crate::address::Address;
use crate::identity::Identity;
use crate::worker_metrics::CONNECTED_CLIENTS;
use hyper::upgrade::Upgraded;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::Mutex;
use tokio::{task::JoinHandle, time::sleep};
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use tokio_tungstenite::WebSocketStream;

use super::client_connection::Protocol;
pub use super::client_connection::{ClientConnection, ClientConnectionSender};
pub use crate::client::ClientActorId;
pub use crate::host::module_host::{ModuleHost, NoSuchModule};

lazy_static! {
    pub static ref CLIENT_ACTOR_INDEX: ClientActorIndex = ClientActorIndex::new(0, HashMap::new(), Vec::new());
}

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
struct Pointer(usize);

struct Inner {
    client_name_auto_increment_state: u64,
    id_index: HashMap<ClientActorId, Pointer>,
    pub clients: Vec<ClientConnection>,
}

impl Inner {
    pub fn get_client(&self, id: &ClientActorId) -> Option<&ClientConnection> {
        let index = self.id_index.get(id);
        if let Some(i) = index {
            return Some(self.clients.get(i.0).unwrap());
        }
        None
    }

    pub fn get_client_mut(&mut self, id: &ClientActorId) -> Option<&mut ClientConnection> {
        let index = self.id_index.get_mut(id);
        if let Some(i) = index {
            return Some(self.clients.get_mut(i.0).unwrap());
        }
        None
    }

    pub fn drop_client(&mut self, id: &ClientActorId) {
        CONNECTED_CLIENTS.dec();

        let index = self.id_index.remove(id);

        if let Some(index) = index {
            // Swizzle around the indexes to match the swap remove
            self.clients.swap_remove(index.0);
            let last = self.clients.get(index.0);
            if let Some(last) = last {
                log::debug!("Swizzle insert...");
                let last_id = last.id;
                self.id_index.insert(last_id, index);
            }
        }
    }

    pub fn mark_client_as_alive(&mut self, id: &ClientActorId) -> Option<()> {
        let client = self.get_client_mut(id)?;
        client.alive = true;
        Some(())
    }

    pub fn get_sender_for_client(&self, id: &ClientActorId) -> Option<ClientConnectionSender> {
        let client = self.get_client(id)?;
        Some(client.sender())
    }

    pub async fn new_client(
        &mut self,
        identity: Identity,
        target_address: Address,
        protocol: Protocol,
        ws: WebSocketStream<Upgraded>,
        database_instance_id: u64,
        module: ModuleHost,
    ) -> Result<(ClientActorId, ClientConnectionSender), NoSuchModule> {
        CONNECTED_CLIENTS.inc();

        let client_name = self.client_name_auto_increment_state;
        self.client_name_auto_increment_state += 1;

        let pointer = Pointer(self.clients.len());

        let client_id = ClientActorId {
            identity,
            name: client_name,
        };
        let mut game_client = ClientConnection::new(client_id, ws, target_address, protocol, database_instance_id);
        let sender = game_client.sender();

        // NOTE: Begin receiving when we create a new client. This only really works
        // because authentication is provided in the headers of the request. That is to say,
        // by the time we're creating a client connection, we already know that this is
        // a valid client actor connection
        game_client.recv();
        self.clients.push(game_client);

        // Update id index
        self.id_index.insert(client_id, pointer);

        // Add this client as a subscriber
        // TODO: Right now this is connecting clients directly to an instance, but their requests should be
        // logically subscribed to the database, not any particular instance. We should handle failover for
        // them and stuff. Not right now though.
        if module
            .call_identity_connected_disconnected(identity, true)
            .await
            .is_err()
        {
            sender.clone().close_with_error("Database could not be found").await;
        }

        Ok((client_id, sender))
    }
}

#[derive(Clone)]
pub struct ClientActorIndex {
    inner: Arc<Mutex<Inner>>,
}

impl ClientActorIndex {
    fn new(
        client_name_auto_increment_state: u64,
        id_index: HashMap<ClientActorId, Pointer>,
        clients: Vec<ClientConnection>,
    ) -> Self {
        let inner = Arc::new(Mutex::new(Inner {
            client_name_auto_increment_state,
            id_index,
            clients,
        }));
        Self { inner }
    }

    pub fn start_liveliness_check(&self) -> JoinHandle<()> {
        let cai = self.clone();
        tokio::spawn(async move {
            loop {
                cai.perform_liveliness_check().await;
                sleep(Duration::from_secs(10)).await;
            }
        })
    }

    pub async fn perform_liveliness_check(&self) {
        log::trace!("Beginning client liveliness check");
        let futures = {
            let mut cai = self.inner.lock().await;
            let mut futures = Vec::new();
            let mut i = 0;
            while i < cai.clients.len() {
                let alive = cai.clients[i].alive;
                let id = cai.clients[i].id;
                if !alive {
                    // Drop it like it's hot.
                    log::trace!("Dropping dead client {}", id);
                    cai.drop_client(&id);
                    continue;
                }
                let client = &mut cai.clients[i];
                client.alive = false;
                let sender = client.sender();
                log::trace!("Pinging client {}", id);
                futures.push(sender.send(WebSocketMessage::Ping(Vec::new())));
                i += 1;
            }
            futures
        };
        futures::future::join_all(futures).await;
    }

    // TODO: not sure if Option<()> makes sense here, maybe it should be a Result
    // or even just a boolean?
    pub async fn mark_client_as_alive(&self, id: &ClientActorId) -> Option<()> {
        self.inner.lock().await.mark_client_as_alive(id)
    }

    pub async fn drop_client(&self, id: &ClientActorId) {
        self.inner.lock().await.drop_client(id)
    }

    pub async fn get_sender_for_client(&self, id: &ClientActorId) -> Option<ClientConnectionSender> {
        self.inner.lock().await.get_sender_for_client(id)
    }

    pub async fn new_client(
        &self,
        identity: Identity,
        target_address: Address,
        protocol: Protocol,
        ws: WebSocketStream<Upgraded>,
        database_instance_id: u64,
        module: ModuleHost,
    ) -> Result<(ClientActorId, ClientConnectionSender), NoSuchModule> {
        self.inner
            .lock()
            .await
            .new_client(identity, target_address, protocol, ws, database_instance_id, module)
            .await
    }
}
