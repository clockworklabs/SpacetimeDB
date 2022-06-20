use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use crate::db::messages::transaction::{Transaction, self};
use super::{client_connection::{ClientConnectionSender, ClientActorId}, client_connection_index::CLIENT_ACTOR_INDEX};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteJson {
    table_id: u32, 
    op: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageJson {
    writes: Vec<WriteJson>
}


#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber {
        client_id: ClientActorId,
    },
    PublishTransaction {
        transaction: Transaction,
    },
}

#[derive(Clone)]
pub struct ModuleSubscription {
    tx: mpsc::UnboundedSender<ModuleSubscriptionCommand>,
}

impl ModuleSubscription {
    pub fn spawn() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut actor = ModuleSubscriptionActor::new();
            while let Some(command) = rx.recv().await {
                if actor.handle_message(command) {
                    break;
                }
            }
        });
        Self { tx }
    }

    pub fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.tx
            .send(ModuleSubscriptionCommand::AddSubscriber {
                client_id,
            })?;
        Ok(())
    }

    pub fn publish_transaction(&self, transaction: Transaction) -> Result<(), anyhow::Error> {
        self.tx.send(ModuleSubscriptionCommand::PublishTransaction { transaction })?;
        Ok(())
    }

}


struct ModuleSubscriptionActor {
    subscribers: Vec<ClientConnectionSender>
}

impl ModuleSubscriptionActor {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new()
        }
    }

    pub fn handle_message(&mut self, command: ModuleSubscriptionCommand) -> bool {
        match command {
            ModuleSubscriptionCommand::AddSubscriber {
                client_id,
            } => {
                let should_exit = self.add_subscriber(client_id);
                should_exit
            },
            ModuleSubscriptionCommand::PublishTransaction {
                transaction
            } => {
                self.publish_transaction(transaction)
            },
        }
    }

    pub fn add_subscriber(&mut self, client_id: ClientActorId) -> bool {
        let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
        let sender = cai.get_client(&client_id).unwrap().sender();
        self.subscribers.push(sender);
        false
    }

    pub fn publish_transaction(&mut self, transaction: Transaction) -> bool {
        let mut message_json = MessageJson {
            writes: Vec::new(),
        };
        for write in transaction.writes {
            let op_string = match write.operation {
                crate::db::messages::write::Operation::Delete => "delete".to_string(),
                crate::db::messages::write::Operation::Insert => "insert".to_string(),
            };
            let value_string = match write.value {
                spacetimedb_bindings::Value::Data { len, buf } => {
                    base64::encode(&buf[0..len as usize])
                },
                spacetimedb_bindings::Value::Hash(hash) => base64::encode(hash),
            };
            message_json.writes.push(WriteJson { table_id: write.set_id, op: op_string, value: value_string })
        }
        let message = serde_json::to_string(&message_json).unwrap();
        for subscriber in &self.subscribers {
            let message = message.clone();
            let subscriber = subscriber.clone();
            tokio::spawn(async move {
                let message = Message::Text(message);
                subscriber.send(message).await.unwrap();
            });
        }
        false
    }
}