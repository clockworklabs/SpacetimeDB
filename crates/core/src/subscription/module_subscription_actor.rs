use std::sync::Arc;

use super::{
    query::compile_read_only_query,
    subscription::{QuerySet, Subscription},
};
use crate::client::{
    messages::{CachedMessage, SubscriptionUpdateMessage, TransactionUpdateMessage},
    ClientActorId, ClientConnectionSender,
};
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::protobuf::client_api::Subscribe;
use crate::worker_metrics::WORKER_METRICS;
use futures::Future;
use parking_lot::RwLock;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;

type Subscriptions = Arc<RwLock<Vec<Subscription>>>;
#[derive(Debug)]
pub struct ModuleSubscriptions {
    relational_db: Arc<RelationalDB>,
    pub subscriptions: Subscriptions,
    owner_identity: Identity,
}

impl ModuleSubscriptions {
    pub fn new(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> Self {
        Self {
            relational_db,
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            owner_identity,
        }
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    pub fn add_subscriber(&self, sender: ClientConnectionSender, subscription: Subscribe) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(), |tx| {
            let ctx = ExecutionContext::subscribe(self.relational_db.address());
            self.relational_db.release_tx(&ctx, tx);
        });

        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = QuerySet::new();
        for sql in subscription.query_strings {
            let qset = compile_read_only_query(&self.relational_db, &tx, &auth, &sql)?;
            queries.extend(qset);
        }

        let database_update = queries.eval(&self.relational_db, &tx, auth)?;
        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        drop(tx);
        self._remove_subscriber(sender.id, &mut subscriptions);
        let subscription = match subscriptions.iter_mut().find(|s| s.queries == queries) {
            Some(sub) => {
                sub.add_subscriber(sender);
                sub
            }
            None => {
                let n = queries.len();
                subscriptions.push(Subscription::new(queries, sender));
                WORKER_METRICS
                    .subscription_queries
                    .with_label_values(&self.relational_db.address())
                    .add(n as i64);
                subscriptions.last_mut().unwrap()
            }
        };

        WORKER_METRICS
            .initial_subscription_evals
            .with_label_values(&self.relational_db.address())
            .inc();

        let sender = subscription.subscribers().last().unwrap().clone();
        drop(subscriptions);
        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let fut = sender.send_message(SubscriptionUpdateMessage { database_update });
        let _ = tokio::runtime::Handle::current().block_on(fut);
        Ok(())
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) {
        let mut subscriptions = self.subscriptions.write();
        self._remove_subscriber(client_id, &mut subscriptions);
    }

    fn _remove_subscriber(&self, client_id: ClientActorId, subscriptions: &mut Vec<Subscription>) {
        subscriptions.retain_mut(|subscription| {
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

    /// Broadcast a ModuleEvent to all interested subscribers.
    ///
    /// It's recommended to take a read lock on `subscriptions` field *before* you commit
    /// the transaction that will give you the event you pass here, to prevent a race condition
    /// where a just-added subscriber receives the same update twice.
    pub async fn broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        subscriptions: &[Subscription],
        event: &ModuleEvent,
    ) {
        match event.status {
            EventStatus::Committed(_) => {
                tokio::task::block_in_place(|| self.broadcast_commit_event(subscriptions, event)).await;
            }
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage {
                        event,
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

    /// A blocking version of [`broadcast_event`][Self::broadcast_event].
    pub fn blocking_broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        subscriptions: &[Subscription],
        event: &ModuleEvent,
    ) {
        tokio::runtime::Handle::current().block_on(self.broadcast_event(client, subscriptions, event))
    }

    /// Broadcast the commit event to all interested subscribers.
    ///
    /// This function is blocking, even though it returns a future. The returned future resolves
    /// once all updates have been successfully added to the subscribers' send queues (i.e. after
    /// it resolves, it's guaranteed that if you call `subscriber.send(x)` the client will receive
    /// x after they receive this subscription update).
    fn broadcast_commit_event(
        &self,
        subscriptions: &[Subscription],
        event: &ModuleEvent,
    ) -> impl Future<Output = ()> + '_ {
        let database_update = event.status.database_update().unwrap();

        let auth = AuthCtx::new(self.owner_identity, event.caller_identity);

        let tokio_handle = &tokio::runtime::Handle::current();

        let tx = &*scopeguard::guard(self.relational_db.begin_tx(), |tx| {
            let ctx = ExecutionContext::incremental_update(self.relational_db.address());
            self.relational_db.release_tx(&ctx, tx);
        });

        let tasks = subscriptions
            .par_iter()
            .filter_map(|subscription| {
                let incr = subscription
                    .queries
                    .eval_incr(&self.relational_db, tx, database_update, auth);
                match incr {
                    Ok(incr) if incr.tables.is_empty() => None,
                    Ok(incr) => Some((subscription, incr)),
                    Err(err) => {
                        // TODO: log an id for the subscription somehow as well
                        tracing::error!(err = &err as &dyn std::error::Error, "subscription eval_incr failed");
                        None
                    }
                }
            })
            .flat_map_iter(|(subscription, database_update)| {
                let message = TransactionUpdateMessage { event, database_update };
                let mut message = CachedMessage::new(message);

                subscription.subscribers().iter().cloned().map(move |subscriber| {
                    let message = message.serialize(subscriber.protocol);
                    tokio_handle.spawn(async move {
                        let _ = subscriber.send(message).await;
                    })
                })
            })
            .collect::<Vec<_>>();

        async move {
            for task in tasks {
                let _ = task.await;
            }
        }
    }
}
