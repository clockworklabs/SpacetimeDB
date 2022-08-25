use super::super::host_controller;
use crate::metrics::CONNECTED_GAME_CLIENTS;
use crate::{hash::Hash, nodes::worker_node::worker_db};
use hyper::upgrade::Upgraded;
use lazy_static::lazy_static;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::{spawn, task::JoinHandle, time::sleep};
use tokio_tungstenite::tungstenite::protocol::Message as WebSocketMessage;
use tokio_tungstenite::WebSocketStream;

use super::client_connection::Protocol;
pub use super::client_connection::{ClientActorId, ClientConnection, ClientConnectionSender};

lazy_static! {
    pub static ref CLIENT_ACTOR_INDEX: Mutex<ClientActorIndex> = {
        Mutex::new(ClientActorIndex {
            client_name_auto_increment_state: 0,
            id_index: HashMap::new(),
            clients: Vec::new(),
            liveliness_check_handle: None,
        })
    };
}

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
struct Pointer(usize);

pub struct ClientActorIndex {
    client_name_auto_increment_state: u64,
    id_index: HashMap<ClientActorId, Pointer>,
    pub clients: Vec<ClientConnection>,
    liveliness_check_handle: Option<JoinHandle<()>>,
}

impl ClientActorIndex {
    pub fn start_liveliness_check() {
        let mut cai = CLIENT_ACTOR_INDEX.lock().unwrap();
        if cai.liveliness_check_handle.is_some() {
            return;
        }
        cai.liveliness_check_handle = Some(tokio::spawn(async move {
            loop {
                log::trace!("Beginning client liveliness check");
                let futures = {
                    let mut cai = CLIENT_ACTOR_INDEX.lock().unwrap();
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
                sleep(Duration::from_secs(10)).await;
            }
        }));
    }

    pub fn get_client(&self, id: &ClientActorId) -> Option<&ClientConnection> {
        let index = self.id_index.get(id);
        if let Some(i) = index {
            return Some(self.clients.get(i.0).unwrap());
        }
        return None;
    }

    pub fn get_client_mut(&mut self, id: &ClientActorId) -> Option<&mut ClientConnection> {
        let index = self.id_index.get_mut(id);
        if let Some(i) = index {
            return Some(self.clients.get_mut(i.0).unwrap());
        }
        return None;
    }

    pub fn drop_client(&mut self, id: &ClientActorId) {
        CONNECTED_GAME_CLIENTS.dec();

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

    pub fn new_client(
        &mut self,
        identity: Hash,
        module_identity: Hash,
        module_name: String,
        protocol: Protocol,
        ws: WebSocketStream<Upgraded>,
    ) -> ClientActorId {
        CONNECTED_GAME_CLIENTS.inc();

        let client_name = self.client_name_auto_increment_state;
        self.client_name_auto_increment_state += 1;

        let pointer = Pointer(self.clients.len());

        let client_id = ClientActorId {
            identity,
            name: client_name,
        };
        let mut game_client = ClientConnection::new(client_id, ws, module_identity, module_name.clone(), protocol);

        // NOTE: Begin receiving when we create a new client. This only really works
        // because authentication is provided in the headers of the request. That is to say,
        // by the time we're creating a client connection, we already know that this is
        // a valid client actor connection
        game_client.recv();
        self.clients.push(game_client);

        // Update id index
        self.id_index.insert(client_id, pointer);

        // Schedule module subscription for the future.
        // We do this since new_client can't really be async.
        spawn(async move {
            // Get the right module and add this client as a subscriber
            // TODO: Should maybe even do this before the upgrade and refuse connection
            // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
            // to connect to multiple modules
            // TODO: Right now this is connecting clients directly to an instance, but their requests should be
            // logically subscribed to the database, not any particular instance. We should handle failover for
            // them and stuff. Not right now though.
            let database = worker_db::get_database_by_address(&module_identity, &module_name).unwrap();
            let database_instance = worker_db::get_leader_database_instance_by_database(database.id);
            let instance_id = database_instance.unwrap().id;
            let host = host_controller::get_host();
            let module = host.get_module(instance_id).unwrap();
            module.add_subscriber(client_id).await.unwrap();
            module
                .call_identity_connected_disconnected(identity, true)
                .await
                .unwrap();
        });
        client_id
    }
}
