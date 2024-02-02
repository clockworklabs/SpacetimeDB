use std::sync::Arc;

use super::{
    query::compile_read_only_query,
    subscription::{QuerySet, Subscription},
};
use crate::execution_context::ExecutionContext;
use crate::protobuf::client_api::Subscribe;
use crate::worker_metrics::WORKER_METRICS;
use crate::{
    client::{
        messages::{CachedMessage, SubscriptionUpdateMessage, TransactionUpdateMessage},
        ClientActorId, ClientConnectionSender,
    },
    host::NoSuchModule,
};
use crate::{db::relational_db::RelationalDB, error::DBError};
use crate::{
    db::relational_db::Tx,
    host::module_host::{EventStatus, ModuleEvent},
};
use futures::{stream::FuturesUnordered, Future, StreamExt};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use tokio::sync::{mpsc, oneshot};

/// All of these commands (and likely any future ones) should all be in the same enum with the
/// same queue so that e.g. db updates from commit events and db updates from subscription
/// modifications don't get sent to the client out of order.
#[derive(Debug)]
enum Command {
    AddSubscriber {
        sender: ClientConnectionSender,
        subscription: Subscribe,
    },
    RemoveSubscriber {
        client_id: ClientActorId,
    },
    BroadcastCommitEvent {
        event: ModuleEvent,
        // channel for signaling that all subscriptions have been evaluated.
        feedback_tx: oneshot::Sender<()>,
    },
}

#[derive(Clone, Debug)]
pub struct ModuleSubscriptionManager {
    tx: mpsc::UnboundedSender<Command>,
}

impl ModuleSubscriptionManager {
    pub fn spawn(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut actor = ModuleSubscriptionActor::new(relational_db, owner_identity);
            while let Some(command) = rx.recv().await {
                if let Err(e) = actor.handle_message(command).await {
                    log::error!("error occurred in ModuleSubscriptionActor: {e}")
                }
            }
        });
        Self { tx }
    }

    pub fn add_subscriber(&self, sender: ClientConnectionSender, subscription: Subscribe) -> Result<(), NoSuchModule> {
        self.tx
            .send(Command::AddSubscriber { sender, subscription })
            .map_err(|_| NoSuchModule)
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), NoSuchModule> {
        self.tx
            .send(Command::RemoveSubscriber { client_id })
            .map_err(|_| NoSuchModule)
    }

    pub async fn broadcast_event(&self, client: Option<&ClientConnectionSender>, mut event: ModuleEvent) {
        match event.status {
            EventStatus::Committed(_) => {
                let (tx, rx) = oneshot::channel();
                self.tx
                    .send(Command::BroadcastCommitEvent { event, feedback_tx: tx })
                    .expect("subscription actor panicked");
                if (rx.await).is_err() {
                    log::error!("failed to receive acknowledgement of successful evaluation from subscription actor")
                }
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
            Command::AddSubscriber { sender, subscription } => self.add_subscription(sender, subscription).await?,
            Command::RemoveSubscriber { client_id } => self.remove_subscriber(client_id),
            Command::BroadcastCommitEvent { event, feedback_tx } => {
                self.broadcast_commit_event(event).await;
                let _ = feedback_tx.send(());
            }
        }
        Ok(())
    }

    async fn _add_subscription(
        &mut self,
        sender: ClientConnectionSender,
        subscription: Subscribe,
        tx: &mut Tx,
    ) -> Result<(), DBError> {
        self.remove_subscriber(sender.id);
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = QuerySet::new();
        for sql in subscription.query_strings {
            let qset = compile_read_only_query(&self.relational_db, tx, &auth, &sql)?;
            queries.extend(qset);
        }

        let subscription = match self.subscriptions.iter_mut().find(|s| s.queries == queries) {
            Some(sub) => {
                sub.add_subscriber(sender);
                sub
            }
            None => {
                let n = queries.len();
                self.subscriptions.push(Subscription::new(queries, sender));
                WORKER_METRICS
                    .subscription_queries
                    .with_label_values(&self.relational_db.address())
                    .add(n as i64);
                self.subscriptions.last_mut().unwrap()
            }
        };

        let database_update = subscription.queries.eval(&self.relational_db, tx, auth)?;

        let sender = subscription.subscribers().last().unwrap();

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
        // Split logic to properly handle `Error` + `Tx`
        let mut tx = self.relational_db.begin_tx();

        let result = self._add_subscription(sender, subscription, &mut tx).await;

        // Note: the missing QueryDebugInfo here is only used for finishing the transaction;
        // all of the relevant queries already executed, with debug info, in _add_subscription
        let ctx = ExecutionContext::subscribe(self.relational_db.address());
        self.relational_db.release_tx(&ctx, tx);
        result
    }

    fn remove_subscriber(&mut self, client_id: ClientActorId) {
        self.subscriptions.retain_mut(|subscription| {
            subscription.remove_subscriber(client_id);
            if subscription.subscribers().is_empty() {
                WORKER_METRICS
                    .subscription_queries
                    .with_label_values(&self.relational_db.address())
                    .sub(subscription.queries.len() as i64);
            }
            !subscription.subscribers().is_empty()
        })
    }

    fn broadcast_commit_event(&mut self, event: ModuleEvent) -> impl Future<Output = ()> + '_ {
        let auth = AuthCtx::new(self.owner_identity, event.caller_identity);

        let tokio_handle = tokio::runtime::Handle::current();
        let futures = FuturesUnordered::new();

        let iter = self.subscriptions.par_iter_mut().map_init(
            || {
                scopeguard::guard(self.relational_db.begin_tx(), |tx| {
                    let ctx = ExecutionContext::incremental_update(self.relational_db.address());
                    self.relational_db.release_tx(&ctx, tx);
                })
            },
            |tx, subscription| {
                let database_update = event.status.database_update().unwrap();
                let incr = subscription
                    .queries
                    .eval_incr(&self.relational_db, tx, database_update, auth)
                    .map_err(|err| {
                        // TODO: log an id for the subscription somehow as well
                        tracing::error!(err = &err as &dyn std::error::Error, "subscription eval_incr failed")
                    })
                    .ok()?;
                if incr.tables.is_empty() {
                    return None;
                }
                Some((subscription, incr))
            },
        );
        let iter = iter.filter_map(|x| x);

        let iter = iter.map(|(subscription, incr)| {
            let message = TransactionUpdateMessage {
                event: &event,
                database_update: incr,
            };
            let mut message = CachedMessage::new(message);

            for subscriber in subscription.subscribers() {
                // rustc realllly doesn't like subscriber.send_message(message) here for weird
                // lifetime reasons, even though it would be sound
                let message = message.serialize(subscriber.protocol);
                let subscriber = subscriber.clone();
                futures.push(tokio_handle.spawn(async move {
                    let _ = subscriber.send(message).await;
                }))
            }
        });

        tokio::task::block_in_place(|| iter.collect::<()>());

        futures.map(drop).collect::<()>()
    }
}
