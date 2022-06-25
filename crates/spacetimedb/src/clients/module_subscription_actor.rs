use super::{
    client_connection::{ClientActorId, ClientConnectionSender},
    client_connection_index::CLIENT_ACTOR_INDEX,
};
use crate::db::{
    messages::transaction::Transaction,
    relational_db::{RelationalDB, ST_TABLES_ID},
};
use serde::{Deserialize, Serialize};
use spacetimedb_bindings::{TupleValue, TypeValue};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteJson {
    table_id: u32,
    op: String,
    row_pk: String,
    row: Vec<TypeValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageJson {
    writes: Vec<WriteJson>,
}

#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber { client_id: ClientActorId },
    PublishTransaction { transaction: Transaction },
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

    pub fn publish_transaction(&self, transaction: Transaction) -> Result<(), anyhow::Error> {
        self.tx
            .send(ModuleSubscriptionCommand::PublishTransaction { transaction })?;
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
            ModuleSubscriptionCommand::PublishTransaction { transaction } => self.publish_transaction(transaction),
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
        let mut message_json = MessageJson { writes: Vec::new() };
        let mut stdb = self.relational_db.lock().unwrap();
        let mut tx = stdb.begin_tx();
        let tables = stdb
            .iter(&mut tx, ST_TABLES_ID)
            .unwrap()
            .map(|row| *row.elements[0].as_u32().unwrap())
            .collect::<Vec<u32>>();
        for table_id in tables {
            for row in stdb.iter(&mut tx, table_id).unwrap() {
                let row_pk = stdb.pk_for_row(&row);
                let row_pk = base64::encode(row_pk.to_bytes());
                message_json.writes.push(WriteJson {
                    table_id,
                    op: "insert".to_string(),
                    row_pk,
                    row: row.elements,
                })
            }
        }
        stdb.rollback_tx(tx);

        let message = serde_json::to_string(&message_json).unwrap();
        let subscriber = subscriber.clone();
        Self::send_async(subscriber, message);
    }

    pub fn publish_transaction(&mut self, transaction: Transaction) -> bool {
        let mut message_json = MessageJson { writes: Vec::new() };

        for write in transaction.writes {
            let op_string = match write.operation {
                crate::db::messages::write::Operation::Delete => "delete".to_string(),
                crate::db::messages::write::Operation::Insert => "insert".to_string(),
            };

            let (row, row_pk) = {
                // TODO: probably awfully slow for very little reason
                let mut stdb = self.relational_db.lock().unwrap();
                let mut tx = stdb.begin_tx();
                let tuple_def = stdb.schema_for_table(&mut tx, write.set_id).unwrap();
                stdb.rollback_tx(tx);
                let tuple = stdb
                    .txdb
                    .from_data_key(&write.data_key, |data| {
                        let (tuple, _) = TupleValue::decode(&tuple_def, data);
                        tuple
                    })
                    .unwrap();
                (tuple, base64::encode(write.data_key.to_bytes()))
            };

            message_json.writes.push(WriteJson {
                table_id: write.set_id,
                op: op_string,
                row_pk,
                row: row.elements,
            })
        }

        self.broadcast_message_json(&message_json);

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
