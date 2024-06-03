use super::execution_unit::{ExecutionUnit, QueryHash};
use super::module_subscription_manager::SubscriptionManager;
use super::query::compile_read_only_query;
use super::subscription::ExecutionSet;
use crate::client::messages::{SubscriptionUpdate, SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::db::engine::{MutTx, DatabaseEngine, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::protobuf::client_api::Subscribe;
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use spacetimedb_vm::errors::ErrorVm;
use spacetimedb_vm::expr::AuthAccess;
use std::{sync::Arc, time::Instant};

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

#[derive(Debug, Clone)]
pub struct ModuleSubscriptions {
    db_engine: Arc<DatabaseEngine>,
    /// If taking a lock (tx) on the db at the same time, ALWAYS lock the db first.
    /// You will deadlock otherwise.
    subscriptions: Subscriptions,
    owner_identity: Identity,
}

type AssertTxFn = Arc<dyn Fn(&Tx)>;

impl ModuleSubscriptions {
    pub fn new(db_engine: Arc<DatabaseEngine>, owner_identity: Identity) -> Self {
        Self {
            db_engine,
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::default())),
            owner_identity,
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
        let config = self.db_engine.read_config();
        let ctx = ExecutionContext::subscribe(self.db_engine.address(), config.slow_query);
        let tx = scopeguard::guard(self.db_engine.begin_tx(), |tx| {
            self.db_engine.release_tx(&ctx, tx);
        });
        let request_id = subscription.request_id;
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = vec![];

        let guard = self.subscriptions.read();

        for sql in subscription
            .query_strings
            .iter()
            .map(|sql| super::query::WHITESPACE.replace_all(sql, " "))
        {
            let sql = sql.trim();
            if sql == super::query::SUBSCRIBE_TO_ALL_QUERY {
                queries.extend(
                    super::subscription::get_all(&self.db_engine, &tx, &auth)?
                        .into_iter()
                        .map(|query| {
                            let hash = QueryHash::from_string(&query.sql);
                            ExecutionUnit::new(query, hash).map(Arc::new)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
                continue;
            }
            let hash = QueryHash::from_string(sql);
            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let mut compiled = compile_read_only_query(&self.db_engine, &tx, sql)?;
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

        execution_set
            .check_auth(auth.owner, auth.caller)
            .map_err(ErrorVm::Auth)?;

        check_row_limit(
            &execution_set,
            &tx,
            |execution_set, tx| execution_set.row_estimate(tx),
            &auth,
            &config,
        )?;

        let database_update = execution_set.eval(&ctx, sender.protocol, &self.db_engine, &tx)?;

        WORKER_METRICS
            .initial_subscription_evals
            .with_label_values(&self.db_engine.address())
            .inc();

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_subscription(&(sender.id.identity, sender.id.address));
        subscriptions.add_subscription(sender.clone(), execution_set.into_iter());
        let num_queries = subscriptions.num_queries();

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.db_engine.address())
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
        subscriptions.remove_subscription(&(client_id.identity, client_id.address));
        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.db_engine.address())
            .set(subscriptions.num_queries() as i64);
    }

    /// Commit a transaction and broadcast its ModuleEvent to all interested subscribers.
    pub fn commit_and_broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        mut event: ModuleEvent,
        ctx: &ExecutionContext,
        tx: MutTx,
    ) -> Result<Result<Arc<ModuleEvent>, WriteConflict>, DBError> {
        // Take a read lock on `subscriptions` before committing tx
        // else it can result in subscriber receiving duplicate updates.
        let subscriptions = self.subscriptions.read();
        let db_engine = &self.db_engine;

        let read_tx = match &mut event.status {
            EventStatus::Committed(db_update) => {
                let Some((tx_data, read_tx)) = db_engine.commit_tx_downgrade(ctx, tx)? else {
                    return Ok(Err(WriteConflict));
                };
                *db_update = DatabaseUpdate::from_writes(&tx_data);
                read_tx
            }
            EventStatus::Failed(_) | EventStatus::OutOfEnergy => db_engine.rollback_mut_tx_downgrade(ctx, tx),
        };
        let event = Arc::new(event);

        match &event.status {
            EventStatus::Committed(_) => subscriptions.eval_updates(db_engine, &read_tx, event.clone(), client),
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage::<DatabaseUpdate> {
                        event: event.clone(),
                        database_update: <_>::default(),
                    };
                    let _ = client.send_message(message);
                } else {
                    log::trace!("Reducer failed but there is no client to send the failure to!")
                }
            }
            EventStatus::OutOfEnergy => {} // ?
        }

        Ok(Ok(event))
    }
}

pub struct WriteConflict;

#[cfg(test)]
mod tests {
    use super::{AssertTxFn, ModuleSubscriptions};
    use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
    use crate::db::engine::tests_utils::TestDB;
    use crate::db::engine::DatabaseEngine;
    use crate::error::DBError;
    use crate::execution_context::ExecutionContext;
    use spacetimedb_client_api_messages::client_api::Subscribe;
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_sats::db::auth::StAccess;
    use spacetimedb_sats::db::error::AuthError;
    use spacetimedb_sats::product;
    use spacetimedb_vm::errors::ErrorVm;
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::mpsc;

    fn add_subscriber(db: Arc<DatabaseEngine>, sql: &str, assert: Option<AssertTxFn>) -> Result<(), DBError> {
        let owner = Identity::from_byte_array([1; 32]);
        let client = ClientActorId::for_test(Identity::ZERO);
        let sender = Arc::new(ClientConnectionSender::dummy(client, Protocol::Binary));
        let module_subscriptions = ModuleSubscriptions::new(db.clone(), owner);

        let subscribe = Subscribe {
            query_strings: [sql.into()].into(),
            request_id: 0,
        };
        module_subscriptions.add_subscriber(sender, subscribe, Instant::now(), assert)
    }

    /// Asserts that a subscription holds a tx handle for the entire length of its evaluation.
    #[test]
    fn test_tx_subscription_ordering() -> ResultTest<()> {
        let test_db = TestDB::durable()?;

        let runtime = test_db.runtime().cloned().unwrap();
        let db = Arc::new(test_db.db.clone());

        // Create table with one row
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            db.insert(tx, table_id, product!(1_u8))
        })?;

        let (send, mut recv) = mpsc::unbounded_channel();

        // Subscribing to T should return a single row.
        let db2 = db.clone();
        let query_handle = runtime.spawn_blocking(move || {
            add_subscriber(
                db.clone(),
                "select * from T",
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
            db2.with_auto_commit(&ExecutionContext::default(), |tx| {
                db2.insert(tx, table_id, product!(2_u8))
            })
        });

        runtime.block_on(write_handle)??;
        runtime.block_on(query_handle)??;

        test_db.close()?;

        Ok(())
    }

    #[test]
    fn subs_cannot_access_private_tables() -> ResultTest<()> {
        let test_db = TestDB::durable()?;
        let db = Arc::new(test_db.db.clone());

        // Create a public table.
        let indexes = &[(0.into(), "a")];
        let cols = &[("a", AlgebraicType::U8)];
        let _ = db.create_table_for_test("public", cols, indexes)?;

        // Create a private table.
        let _ = db.create_table_for_test_with_access("private", cols, indexes, StAccess::Private)?;

        // We can subscribe to a public table.
        let subscribe = |sql| add_subscriber(db.clone(), sql, None);
        assert!(subscribe("SELECT * FROM public").is_ok());

        // We cannot subscribe when a private table is mentioned,
        // not even when in a join where the projection doesn't mention the table,
        // as the mere fact of joining can leak information from the private table.
        for sql in [
            "SELECT * FROM private",
            // Even if the query will return no rows, we still reject it.
            "SELECT * FROM private WHERE 1 = 0",
            "SELECT private.* FROM private",
            "SELECT public.* FROM public JOIN private ON public.a = private.a WHERE private.a = 1",
            "SELECT private.* FROM private JOIN public ON private.a = public.a WHERE public.a = 1",
        ] {
            assert!(matches!(
                subscribe(sql).unwrap_err(),
                DBError::Vm(ErrorVm::Auth(AuthError::TablePrivate { .. }))
            ));
        }

        Ok(())
    }
}
