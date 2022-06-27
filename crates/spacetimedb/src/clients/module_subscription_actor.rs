use super::{
    client_connection::{ClientActorId, ClientConnectionSender, Protocol},
    client_connection_index::CLIENT_ACTOR_INDEX,
};
use crate::{
    db::relational_db::{RelationalDB, ST_COLUMNS_ID, ST_TABLES_ID},
    json::websocket::{
        EventJson, FunctionCallJson, MessageJson, SubscriptionUpdateJson, TableRowOperationJson, TableUpdateJson,
        TransactionUpdateJson,
    },
    protobuf::websocket::{
        event, message, table_row_operation, Event, FunctionCall, Message as MessageProtobuf, SubscriptionUpdate,
        TableRowOperation, TableUpdate, TransactionUpdate,
    },
    wasm_host::module_host::ModuleEvent,
};
use prost::Message as ProstMessage;
use spacetimedb_bindings::{TupleDef, TupleValue};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber { client_id: ClientActorId },
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
    pub fn spawn(relational_db: Arc<Mutex<RelationalDB>>) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut actor = ModuleSubscriptionActor::new(relational_db);
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
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

    pub fn broadcast_event(&self, event: ModuleEvent) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::BroadcastEvent { event })?;
        Ok(())
    }
}

struct ModuleSubscriptionActor {
    relational_db: Arc<Mutex<RelationalDB>>,
    subscribers: Vec<Subscriber>,
}

impl ModuleSubscriptionActor {
    pub fn new(relational_db: Arc<Mutex<RelationalDB>>) -> Self {
        Self {
            relational_db,
            subscribers: Vec::new(),
        }
    }

    pub fn handle_message(&mut self, command: ModuleSubscriptionCommand) -> bool {
        match command {
            ModuleSubscriptionCommand::AddSubscriber { client_id } => {
                let should_exit = self.add_subscriber(client_id);
                should_exit
            }
            ModuleSubscriptionCommand::BroadcastEvent { event } => {
                self.broadcast_event(event);
                false
            }
        }
    }

    pub fn add_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
        let client = cai.get_client(&client_id).unwrap();
        let sender = client.sender();
        let protocol = client.protocol;
        self.subscribers.push(Subscriber {
            sender: sender.clone(),
            protocol,
        });

        self.send_state(protocol, sender);
        false
    }

    fn broadcast_event(&mut self, event: ModuleEvent) {
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
                    Self::send_async_text(sender, json_string.clone());
                }
                Protocol::Binary => {
                    let sender = subscriber.sender.clone();
                    Self::send_async_binary(sender, protobuf_buf.clone());
                }
            }
        }
    }

    fn send_state(&mut self, protocol: Protocol, sender: ClientConnectionSender) {
        match protocol {
            Protocol::Text => {
                let json_state = self.render_json_state();
                let json_string = serde_json::to_string(&json_state).unwrap();
                Self::send_async_text(sender, json_string.clone());
            }
            Protocol::Binary => {
                let protobuf_state = self.render_protobuf_state();
                let mut protobuf_buf = Vec::new();
                protobuf_state.encode(&mut protobuf_buf).unwrap();
                Self::send_async_binary(sender, protobuf_buf.clone());
            }
        }
    }

    pub fn render_protobuf_state(&mut self) -> MessageProtobuf {
        let mut subscription_update = SubscriptionUpdate {
            table_updates: Vec::new(),
        };
        let mut stdb = self.relational_db.lock().unwrap();
        let mut tx = stdb.begin_tx();
        let tables = stdb
            .iter(&mut tx, ST_TABLES_ID)
            .unwrap()
            .map(|row| *row.elements[0].as_u32().unwrap())
            .collect::<Vec<u32>>();
        for table_id in tables {
            if table_id == ST_TABLES_ID || table_id == ST_COLUMNS_ID {
                continue;
            }
            let mut table_row_operations = Vec::new();
            for row in stdb.iter(&mut tx, table_id).unwrap() {
                let row_pk = stdb.pk_for_row(&row);
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
                table_row_operations,
            })
        }
        stdb.rollback_tx(tx);

        MessageProtobuf {
            r#type: Some(message::Type::SubscriptionUpdate(subscription_update)),
        }
    }

    pub fn render_protobuf_event(&mut self, event: &ModuleEvent) -> MessageProtobuf {
        let empty_writes = Vec::new();
        let (status, writes) = match &event.status {
            crate::wasm_host::module_host::EventStatus::Committed(writes) => (event::Status::Committed, writes),
            crate::wasm_host::module_host::EventStatus::Failed => (event::Status::Failed, &empty_writes),
        };

        let event = Event {
            timestamp: event.timestamp,
            status: status.into(),
            caller_identity: event.caller_identity.to_vec(),
            function_call: Some(FunctionCall {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.arg_bytes.to_owned(),
            }),
            message: "TODO".to_owned(),
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
                let mut tx = stdb.begin_tx();
                let tuple_def = stdb.schema_for_table(&mut tx, write.set_id).unwrap();
                stdb.rollback_tx(tx);
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
                    .from_data_key(&write.data_key, |data| {
                        let (tuple, _) = TupleValue::decode(&tuple_def, data);
                        tuple
                    })
                    .unwrap();
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

        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            table_updates.push(TableUpdate {
                table_id,
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
        let mut tx = stdb.begin_tx();
        let tables = stdb
            .iter(&mut tx, ST_TABLES_ID)
            .unwrap()
            .map(|row| *row.elements[0].as_u32().unwrap())
            .collect::<Vec<u32>>();
        for table_id in tables {
            if table_id == ST_TABLES_ID || table_id == ST_COLUMNS_ID {
                continue;
            }
            let mut table_row_operations = Vec::new();
            for row in stdb.iter(&mut tx, table_id).unwrap() {
                let row_pk = stdb.pk_for_row(&row);
                let row_pk = base64::encode(row_pk.to_bytes());
                table_row_operations.push(TableRowOperationJson {
                    op: "insert".to_string(),
                    row_pk,
                    row: row.elements,
                });
            }
            subscription_update.table_updates.push(TableUpdateJson {
                table_id,
                table_row_operations,
            })
        }
        stdb.rollback_tx(tx);

        MessageJson::SubscriptionUpdate(subscription_update)
    }

    pub fn render_json_event(&self, event: &ModuleEvent) -> TransactionUpdateJson {
        let empty_writes = Vec::new();
        let (status_str, writes) = match &event.status {
            crate::wasm_host::module_host::EventStatus::Committed(writes) => ("committed", writes),
            crate::wasm_host::module_host::EventStatus::Failed => ("failed", &empty_writes),
        };

        let event = EventJson {
            timestamp: event.timestamp,
            status: status_str.to_string(),
            caller_identity: hex::encode(event.caller_identity),
            function_call: FunctionCallJson {
                reducer: event.function_call.reducer.to_owned(),
                arg_bytes: event.function_call.arg_bytes.to_owned(),
            },
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
                let mut tx = stdb.begin_tx();
                let tuple_def = stdb.schema_for_table(&mut tx, write.set_id).unwrap();
                stdb.rollback_tx(tx);
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
                    .from_data_key(&write.data_key, |data| {
                        let (tuple, _) = TupleValue::decode(&tuple_def, data);
                        tuple
                    })
                    .unwrap();
                (tuple, base64::encode(write.data_key.to_bytes()))
            };

            vec.push(TableRowOperationJson {
                op: op_string,
                row_pk,
                row: row.elements,
            });
        }

        let mut table_updates = Vec::new();
        for (table_id, table_row_operations) in map.drain() {
            table_updates.push(TableUpdateJson {
                table_id,
                table_row_operations,
            });
        }

        let subscription_update = SubscriptionUpdateJson { table_updates };

        let tx_update = TransactionUpdateJson {
            event,
            subscription_update,
        };

        tx_update
    }

    fn send_async_text(subscriber: ClientConnectionSender, message: String) {
        tokio::spawn(async move {
            let message = Message::Text(message);
            subscriber.send(message).await.unwrap();
        });
    }

    fn send_async_binary(subscriber: ClientConnectionSender, message: impl AsRef<[u8]>) {
        let message = message.as_ref().to_owned();
        tokio::spawn(async move {
            let message = Message::Binary(message);
            subscriber.send(message).await.unwrap();
        });
    }
}
