use super::{
    client::client_connection::{ClientConnectionSender, Protocol},
    client::client_connection_index::CLIENT_ACTOR_INDEX,
};
use crate::client::ClientActorId;
use crate::db::relational_db::RelationalDBWrapper;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::{
    db::relational_db::RelationalDB,
    json::client_api::{
        EventJson, FunctionCallJson, MessageJson, SubscriptionUpdateJson, TableRowOperationJson, TableUpdateJson,
        TransactionUpdateJson,
    },
    protobuf::client_api::{
        event, message, table_row_operation, Event, FunctionCall, Message as MessageProtobuf, SubscriptionUpdate,
        TableRowOperation, TableUpdate, TransactionUpdate,
    },
};
use prost::Message as ProstMessage;
use spacetimedb_lib::{TupleDef, TupleValue};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber { client_id: ClientActorId },
    RemoveSubscriber { client_id: ClientActorId },
    BroadcastEvent { event: ModuleEvent },
}

#[derive(Clone)]
pub struct ModuleSubscription {
    tx: mpsc::UnboundedSender<ModuleSubscriptionCommand>,
}

#[derive(Clone)]
pub struct Subscriber {
    sender: ClientConnectionSender,
    protocol: Protocol,
}

impl ModuleSubscription {
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

    pub fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::AddSubscriber { client_id })?;
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
    subscribers: Vec<Subscriber>,
}

impl ModuleSubscriptionActor {
    pub fn new(relational_db: RelationalDBWrapper) -> Self {
        Self {
            relational_db,
            subscribers: Vec::new(),
        }
    }

    pub async fn handle_message(&mut self, command: ModuleSubscriptionCommand) -> bool {
        // should_exit if true
        match command {
            ModuleSubscriptionCommand::AddSubscriber { client_id } => self.add_subscriber(client_id).await,
            ModuleSubscriptionCommand::RemoveSubscriber { client_id } => self.remove_subscriber(client_id),
            ModuleSubscriptionCommand::BroadcastEvent { event } => {
                self.broadcast_event(event).await;
                false
            }
        }
    }

    pub async fn add_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let (sender, protocol) = {
            let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
            let client = cai.get_client(&client_id).unwrap();
            (client.sender(), client.protocol)
        };
        self.subscribers.push(Subscriber {
            sender: sender.clone(),
            protocol,
        });

        self.send_state(protocol, sender).await;
        false
    }

    pub fn remove_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let position = self.subscribers.iter().position(|s| s.sender.id == client_id);
        match position {
            None => {
                log::warn!(
                    "Unable to find subscription for client_id: {} while attempting to unsubscribe",
                    client_id
                );
            }
            Some(position) => {
                self.subscribers.remove(position);
            }
        }
        false
    }

    async fn broadcast_event(&mut self, event: ModuleEvent) {
        // TODO: this is going to have to be rendered per client based on subscriptions
        let protobuf_event = self.render_protobuf_event(&event);
        let mut protobuf_buf = Vec::new();
        protobuf_event.encode(&mut protobuf_buf).unwrap();

        // TODO: this is going to have to be rendered per client based on subscriptions
        let json_event = self.render_json_event(&event);
        let json_string = serde_json::to_string(&json_event).unwrap();

        for subscriber in &self.subscribers {
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

    /// NOTE: It is important to send the state in this thread because if you spawn a new
    /// thread it's possible for messages to get sent to the client out of order. If you do
    /// spawn in another thread messages will need to be buffered until the state is sent out
    /// on the wire
    async fn send_state(&mut self, protocol: Protocol, sender: ClientConnectionSender) {
        match protocol {
            Protocol::Text => {
                let json_state = self.render_json_state();
                let json_string = serde_json::to_string(&json_state).unwrap();
                Self::send_sync_text(sender, json_string).await;
            }
            Protocol::Binary => {
                let protobuf_state = self.render_protobuf_state();
                let mut protobuf_buf = Vec::new();
                protobuf_state.encode(&mut protobuf_buf).unwrap();
                Self::send_sync_binary(sender, protobuf_buf.clone()).await;
            }
        }
    }

    pub fn render_protobuf_state(&mut self) -> MessageProtobuf {
        let mut subscription_update = SubscriptionUpdate {
            table_updates: Vec::new(),
        };
        let mut stdb = self.relational_db.lock().unwrap();
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();
        let tables = stdb.scan_table_names(tx).unwrap().collect::<Vec<_>>();
        for (table_id, table_name) in tables {
            let mut table_row_operations = Vec::new();
            for row in stdb.scan(tx, table_id).unwrap() {
                let row_pk = RelationalDB::pk_for_row(&row);
                let row_pk = row_pk.to_bytes();
                let mut row_bytes = Vec::new();
                row.encode(&mut row_bytes);
                table_row_operations.push(TableRowOperation {
                    op: table_row_operation::OperationType::Insert.into(),
                    row_pk,
                    row: row_bytes,
                });
            }
            subscription_update.table_updates.push(TableUpdate {
                table_id,
                table_name,
                table_row_operations,
            })
        }
        tx_.rollback();

        MessageProtobuf {
            r#type: Some(message::Type::SubscriptionUpdate(subscription_update)),
        }
    }

    pub fn render_protobuf_event(&mut self, event: &ModuleEvent) -> MessageProtobuf {
        let empty_writes = Vec::new();
        let (status, writes) = match &event.status {
            EventStatus::Committed(writes) => (event::Status::Committed, writes),
            EventStatus::Failed => (event::Status::Failed, &empty_writes),
            EventStatus::OutOfEnergy => (event::Status::OutOfEnergy, &empty_writes),
        };

        let event = Event {
            timestamp: event.timestamp.0,
            status: status.into(),
            caller_identity: event.caller_identity.data.to_vec(),
            function_call: Some(FunctionCall {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.arg_bytes.to_owned(),
            }),
            message: "TODO".to_owned(),
            energy_quanta_used: event.energy_quanta_used,
            host_execution_duration_micros: event.host_execution_duration.as_micros() as u64,
        };

        let mut schemas: HashMap<u32, TupleDef> = HashMap::new();
        let mut map: HashMap<u32, Vec<TableRowOperation>> = HashMap::new();
        for write in writes {
            let op = match write.operation {
                crate::db::messages::write::Operation::Delete => table_row_operation::OperationType::Delete,
                crate::db::messages::write::Operation::Insert => table_row_operation::OperationType::Insert,
            };

            let tuple_def = if let Some(tuple_def) = schemas.get(&write.set_id) {
                tuple_def
            } else {
                let mut stdb = self.relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let tuple_def = stdb.schema_for_table(tx, write.set_id).unwrap().unwrap();
                tx_.rollback();
                schemas.insert(write.set_id, tuple_def);
                schemas.get(&write.set_id).unwrap()
            };

            let vec = if let Some(vec) = map.get_mut(&write.set_id) {
                vec
            } else {
                map.insert(write.set_id, Vec::new());
                map.get_mut(&write.set_id).unwrap()
            };

            let (row, row_pk) = {
                let stdb = self.relational_db.lock().unwrap();
                let tuple = stdb
                    .txdb
                    .from_data_key(&write.data_key, |data| TupleValue::decode(tuple_def, &mut { data }))
                    .unwrap();
                let tuple = match tuple {
                    Ok(tuple) => tuple,
                    Err(e) => {
                        log::error!("render_protobuf_event: Failed to decode row: Err: {}", e);
                        continue;
                    }
                };

                (tuple, write.data_key.to_bytes())
            };

            let mut row_bytes = Vec::new();
            row.encode(&mut row_bytes);
            vec.push(TableRowOperation {
                op: op.into(),
                row_pk,
                row: row_bytes,
            });
        }

        let mut table_name_map: HashMap<u32, String> = HashMap::new();
        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            let table_name = if let Some(name) = table_name_map.get(&table_id) {
                name.clone()
            } else {
                let mut stdb = self.relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let table_name = stdb.table_name_from_id(tx, table_id).unwrap().unwrap();
                let table_name = table_name.to_string();
                tx_.rollback();
                table_name_map.insert(table_id, table_name.clone());
                table_name
            };
            table_updates.push(TableUpdate {
                table_id,
                table_name,
                table_row_operations,
            });
        }

        let subscription_update = SubscriptionUpdate { table_updates };

        let tx_update = TransactionUpdate {
            event: Some(event),
            subscription_update: Some(subscription_update),
        };

        MessageProtobuf {
            r#type: Some(message::Type::TransactionUpdate(tx_update)),
        }
    }

    pub fn render_json_state(&mut self) -> MessageJson {
        // For all tables, push all state
        // TODO: We need some way to namespace tables so we don't send all the internal tables and stuff
        let mut subscription_update = SubscriptionUpdateJson {
            table_updates: Vec::new(),
        };
        let mut stdb = self.relational_db.lock().unwrap();
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();
        let tables = stdb.scan_table_names(tx).unwrap().collect::<Vec<_>>();
        for (table_id, table_name) in tables {
            let mut table_row_operations = Vec::new();
            for row in stdb.scan(tx, table_id).unwrap() {
                let row_pk = RelationalDB::pk_for_row(&row);
                let row_pk = base64::encode(row_pk.to_bytes());
                table_row_operations.push(TableRowOperationJson {
                    op: "insert".to_string(),
                    row_pk,
                    row: row.elements.into(),
                });
            }
            subscription_update.table_updates.push(TableUpdateJson {
                table_id,
                table_name,
                table_row_operations,
            })
        }
        tx_.rollback();

        MessageJson::SubscriptionUpdate(subscription_update)
    }

    pub fn render_json_event(&self, event: &ModuleEvent) -> MessageJson {
        let empty_writes = Vec::new();
        let (status_str, writes) = match &event.status {
            EventStatus::Committed(writes) => ("committed", writes),
            EventStatus::Failed => ("failed", &empty_writes),
            EventStatus::OutOfEnergy => ("out_of_energy", &empty_writes),
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
        };

        let mut schemas: HashMap<u32, TupleDef> = HashMap::new();
        let mut map: HashMap<u32, Vec<TableRowOperationJson>> = HashMap::new();
        for write in writes {
            let op_string = match write.operation {
                crate::db::messages::write::Operation::Delete => "delete".to_string(),
                crate::db::messages::write::Operation::Insert => "insert".to_string(),
            };

            let tuple_def = if let Some(tuple_def) = schemas.get(&write.set_id) {
                tuple_def
            } else {
                let mut stdb = self.relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let tuple_def = stdb.schema_for_table(tx, write.set_id).unwrap().unwrap();
                tx_.rollback();
                schemas.insert(write.set_id, tuple_def);
                schemas.get(&write.set_id).unwrap()
            };

            let vec = if let Some(vec) = map.get_mut(&write.set_id) {
                vec
            } else {
                map.insert(write.set_id, Vec::new());
                map.get_mut(&write.set_id).unwrap()
            };

            let (row, row_pk) = {
                let stdb = self.relational_db.lock().unwrap();
                let tuple = stdb
                    .txdb
                    .from_data_key(&write.data_key, |data| TupleValue::decode(tuple_def, &mut { data }))
                    .unwrap();
                let tuple = match tuple {
                    Ok(tuple) => tuple,
                    Err(e) => {
                        log::error!("render_json_event: Failed to decode row: {}", e);
                        continue;
                    }
                };

                (tuple, base64::encode(write.data_key.to_bytes()))
            };

            vec.push(TableRowOperationJson {
                op: op_string,
                row_pk,
                row: row.elements.into_vec(),
            });
        }

        let mut table_name_map: HashMap<u32, String> = HashMap::new();
        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            let table_name = if let Some(name) = table_name_map.get(&table_id) {
                name.clone()
            } else {
                let mut stdb = self.relational_db.lock().unwrap();
                let mut tx_ = stdb.begin_tx();
                let (tx, stdb) = tx_.get();
                let table_name = stdb.table_name_from_id(tx, table_id).unwrap().unwrap();
                let table_name = table_name.to_string();
                tx_.rollback();
                table_name_map.insert(table_id, table_name.clone());
                table_name
            };
            table_updates.push(TableUpdateJson {
                table_id,
                table_name,
                table_row_operations,
            });
        }

        let subscription_update = SubscriptionUpdateJson { table_updates };

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
