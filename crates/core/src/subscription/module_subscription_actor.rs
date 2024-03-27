use super::execution_unit::{ExecutionUnit, QueryHash};
use super::module_subscription_manager::SubscriptionManager;
use super::query::compile_read_only_query;
use super::subscription::ExecutionSet;
use crate::client::messages::{SubscriptionUpdate, SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_instance_context::DatabaseInstanceContext;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::energy::EnergyMonitor;
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::messages::control_db::Database;
use crate::protobuf::client_api::Subscribe;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use spacetimedb_sats::energy::QueryTimer;
use std::{fmt, sync::Arc, time::Instant};

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

pub struct ModuleSubscriptions {
    pub subscriptions: Subscriptions,
    owner_identity: Identity,
    relational_db: Arc<RelationalDB>,
    energy_monitor: Arc<dyn EnergyMonitor>,
    database: Database,
    pub database_instance_id: u64,
}

impl fmt::Debug for ModuleSubscriptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleSubscriptions")
            .field("subscriptions", &self.subscriptions)
            .field("owner_identity", &self.owner_identity)
            .field("ctx", &"Arc<dyn DatabaseInstanceContext>")
            .finish()
    }
}

type AssertTxFn = Arc<dyn Fn(&Tx)>;

impl ModuleSubscriptions {
    pub(crate) fn new(ctx: &DatabaseInstanceContext, owner_identity: Identity) -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::default())),
            owner_identity,
            database_instance_id: ctx.database_instance_id,
            database: ctx.database.clone(),
            relational_db: ctx.relational_db.clone(),
            energy_monitor: ctx.energy_monitor.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_testing(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::default())),
            owner_identity,
            database_instance_id: 0,
            database: Database::for_testing(),
            relational_db,
            energy_monitor: Arc::new(crate::energy::NullEnergyMonitor),
        }
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    #[tracing::instrument(skip_all)]
    pub fn add_subscriber(
        &self,
        sender: Arc<ClientConnectionSender>,
        subscription: Subscribe,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(), |tx| {
            let ctx = ExecutionContext::subscribe(self.relational_db.address());
            self.relational_db.release_tx(&ctx, tx);
        });
        // check for backward comp.
        let request_id = subscription.request_id;
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = vec![];

        let guard = self.subscriptions.read();
        let mut query_timer = QueryTimer::default();

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
                queries.push(Arc::new(ExecutionUnit::new(compiled.remove(0), hash)?));
            }
        }

        drop(guard);

        let execution_set: ExecutionSet = queries.into();
        let database_update = execution_set.eval(sender.protocol, &self.relational_db, &tx)?;

        query_timer.finish_execution();
        self.energy_monitor
            .record_query_energy(&self.database, self.database_instance_id, query_timer.total());

        WORKER_METRICS
            .initial_subscription_evals
            .with_label_values(&self.relational_db.address())
            .inc();

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();

        subscriptions.remove_subscription(&sender.id.identity);
        subscriptions.add_subscription(sender.clone(), execution_set.into_iter());
        let num_queries = subscriptions.num_queries();

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.address())
            .set(num_queries as i64);

        #[cfg(test)]
        if let Some(assert) = _assert {
            assert(&tx);
        }

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionUpdateMessage {
            subscription_update: SubscriptionUpdate {
                database_update,
                request_id: Some(request_id),
                timer: Some(timer),
            },
        });
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
                tokio::task::block_in_place(|| self.broadcast_commit_event(subscriptions, event))
            }
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage::<DatabaseUpdate> {
                        event: &event,
                        database_update: <_>::default(),
                    };
                    let _ = client.send_message(message);
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
    fn broadcast_commit_event(&self, subscriptions: &SubscriptionManager, event: Arc<ModuleEvent>) {
        subscriptions.eval_updates(&self.relational_db, &event)
    }
}

#[cfg(test)]
mod tests {
    use super::ModuleSubscriptions;
    use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::execution_context::ExecutionContext;
    use spacetimedb_client_api_messages::client_api::Subscribe;
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_sats::product;
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::{runtime::Builder, sync::mpsc};

    #[test]
    /// Asserts that a subscription holds a tx handle for the entire length of its evaluation.
    fn test_tx_subscription_ordering() -> ResultTest<()> {
        let runtime = Builder::new_multi_thread().enable_all().build().unwrap();

        // Create table with one row
        let db = Arc::new(make_test_db()?.0);
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            db.insert(tx, table_id, product!(1_u8))
        })?;

        let id = Identity::ZERO;
        let client = ClientActorId::for_test(id);
        let sender = Arc::new(ClientConnectionSender::dummy(client, Protocol::Binary));
        let module_subscriptions = ModuleSubscriptions::for_testing(db.clone(), id);

        let (send, mut recv) = mpsc::unbounded_channel();

        // Subscribing to T should return a single row
        let query_handle = runtime.spawn_blocking(move || {
            let db = module_subscriptions.relational_db.clone();
            let query_strings = vec!["select * from T".into()];
            module_subscriptions.add_subscriber(
                sender,
                Subscribe {
                    query_strings,
                    request_id: 0,
                },
                Instant::now(),
                Some(Arc::new(move |tx: &_| {
                    // Wake up writer thread after starting the reader tx
                    let _ = send.send(());
                    // Then go to sleep
                    std::thread::sleep(Duration::from_secs(1));
                    let ctx = ExecutionContext::default();
                    // Assuming subscription evaluation holds a lock on the db,
                    // any mutations to T will necessarily occur after,
                    // and therefore we should only see a single row returned.
                    assert_eq!(1, db.iter(&ctx, tx, table_id).unwrap().count());
                })),
            )
        });

        // Write a second row to T concurrently with the reader thread
        let write_handle = runtime.spawn(async move {
            let _ = recv.recv().await;
            db.with_auto_commit(&ExecutionContext::default(), |tx| {
                db.insert(tx, table_id, product!(2_u8))
            })
        });

        runtime.block_on(write_handle)??;
        runtime.block_on(query_handle)??;
        Ok(())
    }
}
