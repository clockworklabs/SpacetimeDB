use super::{
    super::client::client_connection::{ClientConnectionSender, Protocol},
    super::client::client_connection_index::CLIENT_ACTOR_INDEX,
    query::compile_query,
    subscription::Subscription,
};
use crate::db::relational_db::RelationalDBWrapper;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::{client::ClientActorId, host::module_host::DatabaseUpdate};
use crate::{
    json::client_api::{EventJson, FunctionCallJson, MessageJson, TransactionUpdateJson},
    protobuf::client_api::{event, message, Event, FunctionCall, Message as MessageProtobuf, TransactionUpdate},
};
use prost::Message as ProstMessage;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber {
        client_id: ClientActorId,
        query_string: String,
    },
    RemoveSubscriber {
        client_id: ClientActorId,
    },
    BroadcastEvent {
        event: ModuleEvent,
    },
}

#[derive(Clone)]
pub struct ModuleSubscriptionManager {
    tx: mpsc::UnboundedSender<ModuleSubscriptionCommand>,
}

impl ModuleSubscriptionManager {
    pub fn spawn(relational_db: RelationalDBWrapper) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut actor = ModuleSubscriptionActor::new(relational_db);
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command).await {
                    break;
                }
            }
        });
        Self { tx }
    }

    pub fn add_subscriber(&self, client_id: ClientActorId, query_string: String) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::AddSubscriber {
            client_id,
            query_string,
        })?;
        Ok(())
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.tx
            .send(ModuleSubscriptionCommand::RemoveSubscriber { client_id })?;
        Ok(())
    }

    pub fn broadcast_event(&self, event: ModuleEvent) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::BroadcastEvent { event })?;
        Ok(())
    }
}

struct ModuleSubscriptionActor {
    relational_db: RelationalDBWrapper,
    subscriptions: Vec<Subscription>,
}

impl ModuleSubscriptionActor {
    pub fn new(relational_db: RelationalDBWrapper) -> Self {
        Self {
            relational_db,
            subscriptions: Vec::new(),
        }
    }

    pub async fn handle_message(&mut self, command: ModuleSubscriptionCommand) -> bool {
        // should_exit if true
        match command {
            ModuleSubscriptionCommand::AddSubscriber {
                client_id,
                query_string,
            } => self.add_subscription(client_id, query_string).await,
            ModuleSubscriptionCommand::RemoveSubscriber { client_id } => self.remove_subscriber(client_id),
            ModuleSubscriptionCommand::BroadcastEvent { event } => {
                self.broadcast_event(event).await;
                false
            }
        }
    }

    pub async fn add_subscription(&mut self, client_id: ClientActorId, query_string: String) -> bool {
        let (sender, protocol) = {
            let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
            let client = cai.get_client(&client_id).unwrap();
            (client.sender(), client.protocol)
        };

        // TODO(cloutiertyler): Currently using string equivalence as an easy
        // hack to dedup queries. This is not good and should eventually be
        // changed to check the semantic (or at least structure) equivalence of
        // the query.
        let mut s = DefaultHasher::new();
        query_string.hash(&mut s);
        let query_id = s.finish();

        let mut found = false;
        let mut database_update: Option<DatabaseUpdate> = None;
        for sub in &mut self.subscriptions {
            if sub.query_id == query_id {
                sub.add_subscriber(sender.clone(), protocol);
                database_update = Some(sub.eval_query(&mut self.relational_db));
                found = true;
                break;
            }
        }

        if !found {
            let query = compile_query(&mut self.relational_db, &query_string);
            let Ok(query) = query else {
                todo!("Handle this error");
            };
            let mut sub = Subscription {
                query_id,
                query,
                subscribers: vec![],
            };
            sub.add_subscriber(sender.clone(), protocol);
            database_update = Some(sub.eval_query(&mut self.relational_db));
            self.subscriptions.push(sub)
        }

        self.send_state(protocol, sender, database_update.unwrap()).await;
        false
    }

    pub fn _remove_subscription(&mut self, _client_id: ClientActorId, _query_id: u64) -> bool {
        todo!()
    }

    pub fn remove_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let mut i = 0;
        while i < self.subscriptions.len() {
            let sub = &mut self.subscriptions[i];
            sub.remove_subscriber(client_id);
            if sub.subscribers.is_empty() {
                // No more subscribers, remove the subscription
                self.subscriptions.swap_remove(i);
            } else {
                i += 1;
            }
        }
        false
    }

    async fn broadcast_event(&mut self, event: ModuleEvent) {
        let database_update = if let EventStatus::Committed(database_update) = &event.status {
            database_update.clone()
        } else {
            DatabaseUpdate { tables: vec![] }
        };

        for subscription in &mut self.subscriptions {
            let incr = subscription.eval_incr_query(&mut self.relational_db, database_update.clone());

            // NOTE: Currently we are sending all events to all clients even if
            // the query does not yield any rows. This provides support for
            // listening to empty events.  In the future, we probably want to
            // require that client explicitly subscribe to events they want to
            // hear about that have no rows in them.
            //
            // if incr.is_empty() {
            //     continue;
            // }

            let protobuf_event = Self::render_protobuf_event(&event, incr.clone());
            let mut protobuf_buf = Vec::new();
            protobuf_event.encode(&mut protobuf_buf).unwrap();

            let json_event = Self::render_json_event(&event, incr);
            let json_string = serde_json::to_string(&json_event).unwrap();

            for subscriber in &subscription.subscribers {
                let protocol = subscriber.protocol;
                match protocol {
                    Protocol::Text => {
                        let sender = subscriber.sender.clone();
                        Self::send_sync_text(sender, json_string.clone()).await;
                    }
                    Protocol::Binary => {
                        let sender = subscriber.sender.clone();
                        Self::send_sync_binary(sender, protobuf_buf.clone()).await;
                    }
                }
            }
        }
    }

    /// NOTE: It is important to send the state in this thread because if you spawn a new
    /// thread it's possible for messages to get sent to the client out of order. If you do
    /// spawn in another thread messages will need to be buffered until the state is sent out
    /// on the wire
    async fn send_state(
        &mut self,
        protocol: Protocol,
        sender: ClientConnectionSender,
        database_update: DatabaseUpdate,
    ) {
        match protocol {
            Protocol::Text => {
                let json_state = MessageJson::SubscriptionUpdate(database_update.into_json());
                let json_string = serde_json::to_string(&json_state).unwrap();
                Self::send_sync_text(sender, json_string).await;
            }
            Protocol::Binary => {
                let protobuf_state = MessageProtobuf {
                    r#type: Some(message::Type::SubscriptionUpdate(database_update.into_protobuf())),
                };
                let mut protobuf_buf = Vec::new();
                protobuf_state.encode(&mut protobuf_buf).unwrap();
                Self::send_sync_binary(sender, protobuf_buf.clone()).await;
            }
        }
    }

    pub fn render_protobuf_event(event: &ModuleEvent, database_update: DatabaseUpdate) -> MessageProtobuf {
        let (status, errmsg) = match &event.status {
            EventStatus::Committed(_) => (event::Status::Committed, String::new()),
            EventStatus::Failed(errmsg) => (event::Status::Failed, errmsg.clone()),
            EventStatus::OutOfEnergy => (event::Status::OutOfEnergy, String::new()),
        };

        let event = Event {
            timestamp: event.timestamp.0,
            status: status.into(),
            caller_identity: event.caller_identity.data.to_vec(),
            function_call: Some(FunctionCall {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.arg_bytes.to_owned(),
            }),
            message: errmsg,
            energy_quanta_used: event.energy_quanta_used,
            host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
        };

        let subscription_update = database_update.into_protobuf();

        let tx_update = TransactionUpdate {
            event: Some(event),
            subscription_update: Some(subscription_update),
        };

        MessageProtobuf {
            r#type: Some(message::Type::TransactionUpdate(tx_update)),
        }
    }

    pub fn render_json_event(event: &ModuleEvent, database_update: DatabaseUpdate) -> MessageJson {
        let (status_str, errmsg) = match &event.status {
            EventStatus::Committed(_) => ("committed", String::new()),
            EventStatus::Failed(errmsg) => ("failed", errmsg.clone()),
            EventStatus::OutOfEnergy => ("out_of_energy", String::new()),
        };

        let event = EventJson {
            timestamp: event.timestamp.0,
            status: status_str.to_string(),
            caller_identity: event.caller_identity.to_hex(),
            function_call: FunctionCallJson {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.arg_bytes.to_owned(),
            },
            energy_quanta_used: event.energy_quanta_used,
            message: errmsg,
        };

        let subscription_update = database_update.into_json();
        MessageJson::TransactionUpdate(TransactionUpdateJson {
            event,
            subscription_update,
        })
    }

    async fn send_sync_text(subscriber: ClientConnectionSender, message: String) {
        let message = Message::Text(message);
        let _ = subscriber.send(message).await;
    }

    async fn send_sync_binary(subscriber: ClientConnectionSender, message: impl AsRef<[u8]>) {
        let message = message.as_ref().to_owned();
        let message = Message::Binary(message);
        let _ = subscriber.send(message).await;
    }
}
