use super::execution_unit::{ExecutionUnit, QueryHash};
use super::module_subscription_manager::SubscriptionManager;
use super::query::compile_read_only_query;
use super::subscription::ExecutionSet;
use crate::client::messages::{SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::db::relational_db::RelationalDB;
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{EventStatus, ModuleEvent};
use crate::protobuf::client_api::Subscribe;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use std::sync::Arc;

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

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
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::default())),
            owner_identity,
        }
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    #[tracing::instrument(skip_all)]
    pub fn add_subscriber(&self, sender: ClientConnectionSender, subscription: Subscribe) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(), |tx| {
            let ctx = ExecutionContext::subscribe(self.relational_db.address());
            self.relational_db.release_tx(&ctx, tx);
        });

        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = vec![];

        let guard = self.subscriptions.read();

        for sql in subscription.query_strings {
            let hash = QueryHash::from_string(&sql);
            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let mut compiled = compile_read_only_query(&self.relational_db, &tx, &auth, &sql)?;
                if compiled.len() > 1 {
                    return Result::Err(
                        SubscriptionError::Unsupported(String::from("Multiple statements in subscription query"))
                            .into(),
                    );
                }
                queries.push(Arc::new(ExecutionUnit::new(compiled.remove(0), hash)));
            }
        }

        drop(guard);

        let execution_set: ExecutionSet = queries.into();
        let database_update = execution_set.eval(&self.relational_db, &tx, auth)?;

        WORKER_METRICS
            .initial_subscription_evals
            .with_label_values(&self.relational_db.address())
            .inc();

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let sender = Arc::new(sender);
        let mut subscriptions = self.subscriptions.write();
        drop(tx);
        subscriptions.remove_subscription(&sender.id.identity);
        subscriptions.add_subscription(sender.clone(), execution_set.into_iter());
        let num_queries = subscriptions.num_queries();
        drop(subscriptions);

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.address())
            .set(num_queries as i64);

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
        subscriptions.remove_subscription(&client_id.identity);
        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.address())
            .set(subscriptions.num_queries() as i64);
    }

    /// Broadcast a ModuleEvent to all interested subscribers.
    ///
    /// It's recommended to take a read lock on `subscriptions` field *before* you commit
    /// the transaction that will give you the event you pass here, to prevent a race condition
    /// where a just-added subscriber receives the same update twice.
    pub async fn broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        subscriptions: &SubscriptionManager,
        event: Arc<ModuleEvent>,
    ) {
        match event.status {
            EventStatus::Committed(_) => {
                if let Err(err) =
                    tokio::task::block_in_place(|| self.broadcast_commit_event(subscriptions, event)).await
                {
                    // TODO: log an id for the subscription somehow as well
                    tracing::error!(err = &err as &dyn std::error::Error, "subscription eval_incr failed");
                }
            }
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage {
                        event: &event,
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
        subscriptions: &SubscriptionManager,
        event: Arc<ModuleEvent>,
    ) {
        tokio::runtime::Handle::current().block_on(self.broadcast_event(client, subscriptions, event))
    }

    /// Broadcast the commit event to all interested subscribers.
    ///
    /// This function is blocking, even though it returns a future. The returned future resolves
    /// once all updates have been successfully added to the subscribers' send queues (i.e. after
    /// it resolves, it's guaranteed that if you call `subscriber.send(x)` the client will receive
    /// x after they receive this subscription update).
    async fn broadcast_commit_event(
        &self,
        subscriptions: &SubscriptionManager,
        event: Arc<ModuleEvent>,
    ) -> Result<(), DBError> {
        let auth = AuthCtx::new(self.owner_identity, event.caller_identity);
        subscriptions.eval_updates(&self.relational_db, auth, event).await
    }
}
