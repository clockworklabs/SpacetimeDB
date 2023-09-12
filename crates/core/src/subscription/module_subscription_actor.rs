use std::sync::Arc;

use super::{
    query::compile_query,
    subscription::{QuerySet, Subscription},
};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::protobuf::client_api::Subscribe;
use crate::{
    client::{
        messages::{CachedMessage, SubscriptionUpdateMessage, TransactionUpdateMessage},
        ClientActorId, ClientConnectionSender,
    },
    host::NoSuchModule,
};
use crate::{db::relational_db::RelationalDB, error::DBError};
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use tokio::sync::mpsc;

#[derive(Debug)]
enum ModuleSubscriptionCommand {
    AddSubscriber {
        sender: ClientConnectionSender,
        subscription: Subscribe,
    },
    RemoveSubscriber {
        client_id: ClientActorId,
    },
}

#[derive(Debug)]
enum Command {
    Subscription(ModuleSubscriptionCommand),
    BroadcastCommitEvent { event: ModuleEvent },
}

#[derive(Clone, Debug)]
pub struct ModuleSubscriptionManager {
    tx: mpsc::UnboundedSender<ModuleSubscriptionCommand>,
}

#[derive(Clone)]
pub struct SubscriptionEventSender {
    commit_event_tx: mpsc::UnboundedSender<ModuleEvent>,
}

impl ModuleSubscriptionManager {
    pub fn spawn(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> (Self, SubscriptionEventSender) {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (commit_event_tx, mut commit_event_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut actor = ModuleSubscriptionActor::new(relational_db, owner_identity);
            loop {
                let command = tokio::select! {
                    event = commit_event_rx.recv() => match event {
                        Some(event) => Command::BroadcastCommitEvent { event },
                        // the module has exited
                        None => break,
                    },
                    Some(cmd) = rx.recv() => Command::Subscription(cmd),
                };
                if let Err(e) = actor.handle_message(command).await {
                    log::error!("error occurred in ModuleSubscriptionActor: {e}")
                }
            }
        });
        (Self { tx }, SubscriptionEventSender { commit_event_tx })
    }

    pub fn add_subscriber(&self, sender: ClientConnectionSender, subscription: Subscribe) -> Result<(), NoSuchModule> {
        self.tx
            .send(ModuleSubscriptionCommand::AddSubscriber { sender, subscription })
            .map_err(|_| NoSuchModule)
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), NoSuchModule> {
        self.tx
            .send(ModuleSubscriptionCommand::RemoveSubscriber { client_id })
            .map_err(|_| NoSuchModule)
    }
}

impl SubscriptionEventSender {
    pub async fn broadcast_event(&self, client: Option<&ClientConnectionSender>, mut event: ModuleEvent) {
        match event.status {
            EventStatus::Committed(_) => {
                self.commit_event_tx.send(event).expect("subscription actor panicked");
            }
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage {
                        event: &mut event,
                        database_update: Default::default(),
                    };
                    let _ = client.send_message(message).await;
                } else {
                    log::trace!("Reducer failed but there is no client to send the failure to!")
                }
            }
            EventStatus::OutOfEnergy => {} // ?
        }
    }

    pub fn broadcast_event_blocking(&self, client: Option<&ClientConnectionSender>, event: ModuleEvent) {
        tokio::runtime::Handle::current().block_on(self.broadcast_event(client, event))
    }
}

struct ModuleSubscriptionActor {
    relational_db: Arc<RelationalDB>,
    subscriptions: Vec<Subscription>,
    owner_identity: Identity,
}

impl ModuleSubscriptionActor {
    fn new(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> Self {
        Self {
            relational_db,
            subscriptions: Vec::new(),
            owner_identity,
        }
    }

    async fn handle_message(&mut self, command: Command) -> Result<(), DBError> {
        match command {
            Command::Subscription(ModuleSubscriptionCommand::AddSubscriber { sender, subscription }) => {
                self.add_subscription(sender, subscription).await?
            }
            Command::Subscription(ModuleSubscriptionCommand::RemoveSubscriber { client_id }) => {
                self.remove_subscriber(client_id)
            }
            Command::BroadcastCommitEvent { event } => self.broadcast_commit_event(event).await?,
        }
        Ok(())
    }

    async fn _add_subscription(
        &mut self,
        sender: ClientConnectionSender,
        subscription: Subscribe,
        tx: &mut MutTxId,
    ) -> Result<(), DBError> {
        self.remove_subscriber(sender.id);
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);

        let queries: QuerySet = subscription
            .query_strings
            .into_iter()
            .map(|query| compile_query(&self.relational_db, tx, &auth, &query))
            .collect::<Result<_, _>>()?;

        let sub = match self.subscriptions.iter_mut().find(|s| s.queries == queries) {
            Some(sub) => {
                sub.subscribers.push(sender);
                sub
            }
            None => {
                self.subscriptions.push(Subscription {
                    queries,
                    subscribers: vec![sender],
                });
                self.subscriptions.last_mut().unwrap()
            }
        };

        let database_update = sub.queries.eval(&self.relational_db, tx, auth)?;

        let sender = sub.subscribers.last().unwrap();

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionUpdateMessage { database_update }).await;

        Ok(())
    }

    async fn add_subscription(
        &mut self,
        sender: ClientConnectionSender,
        subscription: Subscribe,
    ) -> Result<(), DBError> {
        //Split logic to properly handle `Error` + `Tx`
        let mut tx = self.relational_db.begin_tx();
        let result = self._add_subscription(sender, subscription, &mut tx).await;
        self.relational_db.finish_tx(tx, result)
    }

    fn remove_subscriber(&mut self, client_id: ClientActorId) {
        self.subscriptions.retain_mut(|sub| {
            sub.remove_subscriber(client_id);
            !sub.subscribers.is_empty()
        })
    }

    async fn _broadcast_commit_event(&mut self, mut event: ModuleEvent, tx: &mut MutTxId) -> Result<(), DBError> {
        let futures = FuturesUnordered::new();
        let auth = AuthCtx::new(self.owner_identity, event.caller_identity);

        for subscription in &mut self.subscriptions {
            let database_update = event.status.database_update().unwrap();
            let incr = subscription
                .queries
                .eval_incr(&self.relational_db, tx, database_update, auth)?;

            if incr.tables.is_empty() {
                continue;
            }

            let message = TransactionUpdateMessage {
                event: &mut event,
                database_update: incr,
            };
            let mut message = CachedMessage::new(message);

            for subscriber in &subscription.subscribers {
                // rustc realllly doesn't like subscriber.send_message(message) here for weird
                // lifetime reasons, even though it would be sound
                let message = message.serialize(subscriber.protocol);
                futures.push(subscriber.send(message).map(drop))
            }
        }

        futures.collect::<()>().await;

        Ok(())
    }

    async fn broadcast_commit_event(&mut self, event: ModuleEvent) -> Result<(), DBError> {
        //Split logic to properly handle `Error` + `Tx`
        let mut tx = self.relational_db.begin_tx();
        let result = self._broadcast_commit_event(event, &mut tx).await;
        self.relational_db.finish_tx(tx, result)
    }
}
