use super::{client_connection::ClientActorId, client_connection_index::CLIENT_ACTOR_INDEX};
use crate::db::messages::transaction::Transaction;
use tokio::{spawn, sync::mpsc};
use tokio_tungstenite::tungstenite::Message;

pub struct SubscriptionManager {
    subscribers: Vec<ClientActorId>,
}

impl SubscriptionManager {
    pub fn spawn(mut rx: mpsc::Receiver<Transaction>) {
        spawn(async move {
            let mut subscription_manager = SubscriptionManager::new();
            while let Some(transaction) = rx.recv().await {
                subscription_manager.broadcast(transaction).await;
            }
        });
    }

    fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    async fn broadcast(&mut self, transaction: Transaction) {
        let senders = {
            let cai = CLIENT_ACTOR_INDEX.lock().unwrap();
            self.subscribers
                .iter()
                .map(|id| cai.get_client(id).unwrap().sender())
                .collect::<Vec<_>>()
        };
        let mut bytes = Vec::new();
        transaction.encode(&mut bytes);
        for client in senders {
            client.send_message_warn_fail(Message::Binary(bytes.clone())).await;
        }
    }
}
