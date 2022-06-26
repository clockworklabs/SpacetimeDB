use super::{
    client_connection::{ClientActorId, ClientConnectionSender},
    client_connection_index::CLIENT_ACTOR_INDEX,
};
use crate::{
    db::relational_db::{RelationalDB, ST_TABLES_ID},
    json::websocket::{
        EventJson, FunctionCallJson, MessageJson, SubscriptionUpdateJson, TableRowOperationJson, TableUpdateJson,
        TransactionUpdateJson,
    },
    wasm_host::module_host::ModuleEvent,
};
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
    PublishEvent { event: ModuleEvent },
}

#[derive(Clone)]
pub struct ModuleSubscription {
    tx: mpsc::UnboundedSender<ModuleSubscriptionCommand>,
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

    pub fn publish_event(&self, event: ModuleEvent) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::PublishEvent { event })?;
        Ok(())
    }
}

struct ModuleSubscriptionActor {
    relational_db: Arc<Mutex<RelationalDB>>,
    subscribers: Vec<ClientConnectionSender>,
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
            ModuleSubscriptionCommand::PublishEvent { event } => self.publish_event(event),
        }
    }

    pub fn add_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
        let sender = cai.get_client(&client_id).unwrap().sender();
        self.subscribers.push(sender.clone());

        self.publish_state(sender);
        false
    }

    fn publish_state(&mut self, subscriber: ClientConnectionSender) {
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

        let message_json = MessageJson::SubscriptionUpdate(subscription_update);
        let message = serde_json::to_string(&message_json).unwrap();
        let subscriber = subscriber.clone();
        Self::send_async(subscriber, message);
    }

    pub fn publish_event(&mut self, event: ModuleEvent) -> bool {
        let (status_str, writes) = match event.status {
            crate::wasm_host::module_host::EventStatus::Committed(writes) => ("committed", writes),
            crate::wasm_host::module_host::EventStatus::Failed => ("failed", Vec::new()),
        };

        let event = EventJson {
            timestamp: event.timestamp,
            status: status_str.to_string(),
            caller_identity: event.caller_identity,
            function_call: FunctionCallJson {
                reducer: event.function_call.reducer,
                arg_bytes: event.function_call.arg_bytes,
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

        self.broadcast_message_json(&MessageJson::TransactionUpdate(tx_update));

        false
    }

    fn broadcast_message_json(&self, message_json: &MessageJson) {
        let message = serde_json::to_string(message_json).unwrap();
        for subscriber in &self.subscribers {
            let message = message.clone();
            let subscriber = subscriber.clone();
            Self::send_async(subscriber, message);
        }
    }

    fn send_async(subscriber: ClientConnectionSender, message: String) {
        tokio::spawn(async move {
            let message = Message::Text(message);
            subscriber.send(message).await.unwrap();
        });
    }
}
