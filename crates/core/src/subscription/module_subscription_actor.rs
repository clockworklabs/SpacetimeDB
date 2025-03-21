use super::execution_unit::QueryHash;
use super::module_subscription_manager::{Plan, SubscriptionManager};
use super::query::compile_read_only_query;
use super::tx::DeltaTx;
use super::{collect_table_update, record_exec_metrics, TableUpdateType};
use crate::client::messages::{
    SubscriptionData, SubscriptionError, SubscriptionMessage, SubscriptionResult, SubscriptionRows,
    SubscriptionUpdateMessage, TransactionUpdateMessage,
};
use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::execution_context::{Workload, WorkloadType};
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::messages::websocket::Subscribe;
use crate::sql::ast::SchemaViewer;
use crate::subscription::execute_plans;
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_client_api_messages::websocket::{
    self as ws, BsatnFormat, FormatSwitch, JsonFormat, SubscribeMulti, SubscribeSingle, TableUpdate, Unsubscribe,
    UnsubscribeMulti,
};
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_expr::check::parse_and_type_sub;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::Identity;
use std::{sync::Arc, time::Instant};

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

#[derive(Debug, Clone)]
pub struct ModuleSubscriptions {
    relational_db: Arc<RelationalDB>,
    /// If taking a lock (tx) on the db at the same time, ALWAYS lock the db first.
    /// You will deadlock otherwise.
    subscriptions: Subscriptions,
    owner_identity: Identity,
}

type AssertTxFn = Arc<dyn Fn(&Tx)>;
type SubscriptionUpdate = FormatSwitch<TableUpdate<BsatnFormat>, TableUpdate<JsonFormat>>;
type FullSubscriptionUpdate = FormatSwitch<ws::DatabaseUpdate<BsatnFormat>, ws::DatabaseUpdate<JsonFormat>>;

/// A utility for sending an error message to a client and returning early
macro_rules! return_on_err {
    ($expr:expr, $handler:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                // TODO: Handle errors sending messages.
                let _ = $handler(e.to_string().into());
                return Ok(());
            }
        }
    };
}

/// Hash a sql query, using the caller's identity if necessary
fn hash_query(sql: &str, tx: &TxId, auth: &AuthCtx) -> Result<QueryHash, DBError> {
    parse_and_type_sub(sql, &SchemaViewer::new(tx, auth), auth)
        .map_err(DBError::from)
        .map(|(_, has_param)| QueryHash::from_string(sql, auth.caller, has_param))
}

impl ModuleSubscriptions {
    pub fn new(relational_db: Arc<RelationalDB>, subscriptions: Subscriptions, owner_identity: Identity) -> Self {
        Self {
            relational_db,
            subscriptions,
            owner_identity,
        }
    }

    /// Run auth and row limit checks for a new subscriber, then compute the initial query results.
    fn evaluate_initial_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        query: Arc<Plan>,
        tx: &TxId,
        auth: &AuthCtx,
        update_type: TableUpdateType,
    ) -> Result<(SubscriptionUpdate, ExecutionMetrics), DBError> {
        check_row_limit(
            query.physical_plan(),
            &self.relational_db,
            tx,
            |plan, tx| estimate_rows_scanned(tx, plan),
            auth,
        )?;

        let comp = sender.config.compression;
        let table_id = query.subscribed_table_id();
        let table_name = query.subscribed_table_name();
        let plan = query.physical_plan().clone().optimize().map(PipelinedProject::from)?;
        let tx = DeltaTx::from(tx);

        Ok(match sender.config.protocol {
            Protocol::Binary => collect_table_update(&plan, table_id, table_name.into(), comp, &tx, update_type)
                .map(|(table_update, metrics)| (FormatSwitch::Bsatn(table_update), metrics))?,
            Protocol::Text => collect_table_update(&plan, table_id, table_name.into(), comp, &tx, update_type)
                .map(|(table_update, metrics)| (FormatSwitch::Json(table_update), metrics))?,
        })
    }

    fn evaluate_queries(
        &self,
        sender: Arc<ClientConnectionSender>,
        queries: &Vec<Arc<Plan>>,
        tx: &TxId,
        auth: &AuthCtx,
        update_type: TableUpdateType,
    ) -> Result<(FullSubscriptionUpdate, ExecutionMetrics), DBError> {
        fn rows_scanned(tx: &TxId, plans: &[Arc<Plan>]) -> u64 {
            plans
                .iter()
                .map(|plan| estimate_rows_scanned(tx, plan.physical_plan()))
                .fold(0, |acc, n| acc.saturating_add(n))
        }

        check_row_limit(
            &queries,
            &self.relational_db,
            tx,
            |plan, tx| rows_scanned(tx, plan),
            auth,
        )?;
        let comp = sender.config.compression;

        let tx = DeltaTx::from(tx);
        match sender.config.protocol {
            Protocol::Binary => {
                let (update, metrics) = execute_plans(queries, comp, &tx, update_type)?;
                Ok((FormatSwitch::Bsatn(update), metrics))
            }
            Protocol::Text => {
                let (update, metrics) = execute_plans(queries, comp, &tx, update_type)?;
                Ok((FormatSwitch::Json(update), metrics))
            }
        }
    }

    /// Add a subscription to a single query.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn add_single_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: SubscribeSingle,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        // Send an error message to the client
        let send_err_msg = |message| {
            sender.send_message(SubscriptionMessage {
                request_id: Some(request.request_id),
                query_id: Some(request.query_id),
                timer: Some(timer),
                result: SubscriptionResult::Error(SubscriptionError {
                    table_id: None,
                    message,
                }),
            })
        };

        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let query = super::query::WHITESPACE.replace_all(&request.query, " ");
        let sql = query.trim();

        let hash = return_on_err!(hash_query(sql, &tx, &auth), send_err_msg);

        let existing_query = {
            let guard = self.subscriptions.read();
            guard.query(&hash)
        };

        let query = return_on_err!(
            existing_query
                .map(Ok)
                .unwrap_or_else(|| compile_read_only_query(&auth, &tx, sql).map(Arc::new)),
            send_err_msg
        );

        let (table_rows, metrics) = return_on_err!(
            self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth, TableUpdateType::Subscribe),
            send_err_msg
        );

        record_exec_metrics(
            &WorkloadType::Subscribe,
            &self.relational_db.database_identity(),
            metrics,
        );

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.add_subscription(sender.clone(), query.clone(), request.query_id)?;

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);

        #[cfg(test)]
        if let Some(assert) = _assert {
            assert(&tx);
        }

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::Subscribe(SubscriptionRows {
                table_id: query.subscribed_table_id(),
                table_name: query.subscribed_table_name().into(),
                table_rows,
            }),
        });
        Ok(())
    }

    /// Remove a subscription for a single query.
    pub fn remove_single_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: Unsubscribe,
        timer: Instant,
    ) -> Result<(), DBError> {
        // Send an error message to the client
        let send_err_msg = |message| {
            sender.send_message(SubscriptionMessage {
                request_id: Some(request.request_id),
                query_id: Some(request.query_id),
                timer: Some(timer),
                result: SubscriptionResult::Error(SubscriptionError {
                    table_id: None,
                    message,
                }),
            })
        };

        let mut subscriptions = self.subscriptions.write();

        let query =
            match subscriptions.remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id) {
                Ok(queries) => {
                    // This is technically a bug, since this could be empty if the client has another duplicate subscription.
                    // This whole function should be removed soon, so I don't think we need to fix it.
                    if queries.len() == 1 {
                        queries[0].clone()
                    } else {
                        // Apparently we ignore errors sending messages.
                        let _ = send_err_msg("Internal error".into());
                        return Ok(());
                    }
                }
                Err(error) => {
                    // Apparently we ignore errors sending messages.
                    let _ = send_err_msg(error.to_string().into());
                    return Ok(());
                }
            };

        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Unsubscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let eval_result =
            self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth, TableUpdateType::Unsubscribe);

        // If execution error, send to client
        let (table_rows, metrics) = match eval_result {
            Ok(ok) => ok,
            Err(e) => {
                // Apparently we ignore errors sending messages.
                let _ = send_err_msg(e.to_string().into());
                return Ok(());
            }
        };

        record_exec_metrics(
            &WorkloadType::Subscribe,
            &self.relational_db.database_identity(),
            metrics,
        );

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);
        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::Unsubscribe(SubscriptionRows {
                table_id: query.subscribed_table_id(),
                table_name: query.subscribed_table_name().into(),
                table_rows,
            }),
        });
        Ok(())
    }

    /// Remove a client's subscription for a set of queries.
    pub fn remove_multi_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: UnsubscribeMulti,
        timer: Instant,
    ) -> Result<(), DBError> {
        // Send an error message to the client
        let send_err_msg = |message| {
            sender.send_message(SubscriptionMessage {
                request_id: Some(request.request_id),
                query_id: Some(request.query_id),
                timer: Some(timer),
                result: SubscriptionResult::Error(SubscriptionError {
                    table_id: None,
                    message,
                }),
            })
        };

        // Always lock the db before the subscription lock to avoid deadlocks.
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Unsubscribe), |tx| {
            self.relational_db.release_tx(tx);
        });

        let removed_queries = {
            let mut subscriptions = self.subscriptions.write();

            let queries = match subscriptions
                .remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id)
            {
                Ok(queries) => queries,
                Err(error) => {
                    // Apparently we ignore errors sending messages.
                    let _ = send_err_msg(error.to_string().into());
                    return Ok(());
                }
            };
            WORKER_METRICS
                .subscription_queries
                .with_label_values(&self.relational_db.database_identity())
                .set(subscriptions.num_unique_queries() as i64);
            queries
        };

        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let eval_result = self.evaluate_queries(
            sender.clone(),
            &removed_queries,
            &tx,
            &auth,
            TableUpdateType::Unsubscribe,
        );
        // If execution error, send to client
        let (update, metrics) = match eval_result {
            Ok(ok) => ok,
            Err(e) => {
                // Apparently we ignore errors sending messages.
                let _ = send_err_msg(e.to_string().into());
                return Ok(());
            }
        };

        record_exec_metrics(
            &WorkloadType::Unsubscribe,
            &self.relational_db.database_identity(),
            metrics,
        );

        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::UnsubscribeMulti(SubscriptionData { data: update }),
        });
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn add_multi_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: SubscribeMulti,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        // Send an error message to the client
        let send_err_msg = |message| {
            let _ = sender.send_message(SubscriptionMessage {
                request_id: Some(request.request_id),
                query_id: Some(request.query_id),
                timer: Some(timer),
                result: SubscriptionResult::Error(SubscriptionError {
                    table_id: None,
                    message,
                }),
            });
        };

        // We always get the db lock before the subscription lock to avoid deadlocks.
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = vec![];
        let guard = self.subscriptions.read();
        for sql in request
            .query_strings
            .iter()
            .map(|sql| super::query::WHITESPACE.replace_all(sql, " "))
        {
            let sql = sql.trim();
            if sql == super::query::SUBSCRIBE_TO_ALL_QUERY {
                queries.extend(
                    super::subscription::get_all(&self.relational_db, &tx, &auth)?
                        .into_iter()
                        .map(Arc::new),
                );
                continue;
            }

            let hash = return_on_err!(hash_query(sql, &tx, &auth), send_err_msg);

            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let compiled = return_on_err!(compile_read_only_query(&auth, &tx, sql), send_err_msg);
                queries.push(Arc::new(compiled));
            }
        }

        drop(guard);

        // We minimize locking so that other clients can add subscriptions concurrently.
        // We are protected from race conditions with broadcasts, because we have the db lock,
        // an `commit_and_broadcast_event` grabs a read lock on `subscriptions` while it still has a
        // write lock on the db.
        let queries = {
            let mut subscriptions = self.subscriptions.write();
            let new_queries = subscriptions.add_subscription_multi(sender.clone(), queries, request.query_id)?;

            WORKER_METRICS
                .subscription_queries
                .with_label_values(&self.relational_db.database_identity())
                .set(subscriptions.num_unique_queries() as i64);
            new_queries
        };

        let Ok((update, metrics)) =
            self.evaluate_queries(sender.clone(), &queries, &tx, &auth, TableUpdateType::Subscribe)
        else {
            // If we fail the query, we need to remove the subscription.
            let mut subscriptions = self.subscriptions.write();
            subscriptions.remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id)?;
            send_err_msg("Internal error evaluating queries".into());
            return Ok(());
        };

        record_exec_metrics(
            &WorkloadType::Subscribe,
            &self.relational_db.database_identity(),
            metrics,
        );

        #[cfg(test)]
        if let Some(assert) = _assert {
            assert(&tx);
        }

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::SubscribeMulti(SubscriptionData { data: update }),
        });
        Ok(())
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    /// This is used for the legacy subscription API which uses a set of queries.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn add_legacy_subscriber(
        &self,
        sender: Arc<ClientConnectionSender>,
        subscription: Subscribe,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            self.relational_db.release_tx(tx);
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
                    super::subscription::get_all(&self.relational_db, &tx, &auth)?
                        .into_iter()
                        .map(Arc::new),
                );
                continue;
            }

            let hash = hash_query(sql, &tx, &auth)?;
            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let compiled = compile_read_only_query(&auth, &tx, sql)?;
                queries.push(Arc::new(compiled));
            }
        }

        drop(guard);

        let comp = sender.config.compression;

        fn rows_scanned(tx: &TxId, plans: &[Arc<Plan>]) -> u64 {
            plans
                .iter()
                .map(|plan| estimate_rows_scanned(tx, plan.physical_plan()))
                .fold(0, |acc, n| acc.saturating_add(n))
        }

        check_row_limit(
            &queries,
            &self.relational_db,
            &tx,
            |plan, tx| rows_scanned(tx, plan),
            &auth,
        )?;

        let tx = DeltaTx::from(&*tx);
        let (database_update, metrics) = match sender.config.protocol {
            Protocol::Binary => execute_plans(&queries, comp, &tx, TableUpdateType::Subscribe)
                .map(|(table_update, metrics)| (FormatSwitch::Bsatn(table_update), metrics))?,
            Protocol::Text => execute_plans(&queries, comp, &tx, TableUpdateType::Subscribe)
                .map(|(table_update, metrics)| (FormatSwitch::Json(table_update), metrics))?,
        };

        record_exec_metrics(
            &WorkloadType::Subscribe,
            &self.relational_db.database_identity(),
            metrics,
        );

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.set_legacy_subscription(sender.clone(), queries.into_iter());
        let num_queries = subscriptions.num_unique_queries();

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
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
            database_update,
            request_id: Some(request_id),
            timer: Some(timer),
        });
        Ok(())
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) {
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_all_subscriptions(&(client_id.identity, client_id.connection_id));
        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);
    }

    /// Commit a transaction and broadcast its ModuleEvent to all interested subscribers.
    pub fn commit_and_broadcast_event(
        &self,
        caller: Option<&ClientConnectionSender>,
        mut event: ModuleEvent,
        tx: MutTx,
    ) -> Result<Result<Arc<ModuleEvent>, WriteConflict>, DBError> {
        // Take a read lock on `subscriptions` before committing tx
        // else it can result in subscriber receiving duplicate updates.
        let subscriptions = self.subscriptions.read();
        let stdb = &self.relational_db;
        // Downgrade mutable tx.
        // Ensure tx is released/cleaned up once out of scope.
        let (read_tx, tx_data) = match &mut event.status {
            EventStatus::Committed(db_update) => {
                let Some((tx_data, read_tx)) = stdb.commit_tx_downgrade(tx, Workload::Update)? else {
                    return Ok(Err(WriteConflict));
                };
                *db_update = DatabaseUpdate::from_writes(&tx_data);
                (read_tx, Some(tx_data))
            }
            EventStatus::Failed(_) | EventStatus::OutOfEnergy => {
                (stdb.rollback_mut_tx_downgrade(tx, Workload::Update), None)
            }
        };

        let read_tx = scopeguard::guard(read_tx, |tx| {
            self.relational_db.release_tx(tx);
        });

        let read_tx = tx_data
            .as_ref()
            .map(|tx_data| DeltaTx::new(&read_tx, tx_data))
            .unwrap_or_else(|| DeltaTx::from(&*read_tx));

        let event = Arc::new(event);

        match &event.status {
            EventStatus::Committed(_) => {
                subscriptions.eval_updates(&read_tx, event.clone(), caller, &self.relational_db.database_identity())
            }
            EventStatus::Failed(_) => {
                if let Some(client) = caller {
                    let message = TransactionUpdateMessage {
                        event: Some(event.clone()),
                        database_update: SubscriptionUpdateMessage::default_for_protocol(client.config.protocol, None),
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
    use crate::client::messages::{
        SerializableMessage, SubscriptionMessage, SubscriptionResult, SubscriptionUpdateMessage,
        TransactionUpdateMessage,
    };
    use crate::client::{ClientActorId, ClientConfig, ClientConnectionSender, ClientName, Protocol};
    use crate::db::datastore::traits::IsolationLevel;
    use crate::db::relational_db::tests_utils::{insert, TestDB};
    use crate::db::relational_db::RelationalDB;
    use crate::error::DBError;
    use crate::execution_context::Workload;
    use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
    use crate::messages::websocket as ws;
    use crate::subscription::module_subscription_manager::SubscriptionManager;
    use crate::subscription::query::compile_read_only_query;
    use crate::subscription::TableUpdateType;
    use parking_lot::RwLock;
    use spacetimedb_client_api_messages::energy::EnergyQuanta;
    use spacetimedb_client_api_messages::websocket::{
        CompressableQueryUpdate, Compression, FormatSwitch, QueryId, RowListLen, Subscribe, SubscribeMulti,
        SubscribeSingle, Unsubscribe,
    };
    use spacetimedb_lib::db::auth::StAccess;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::{bsatn, ConnectionId, ProductType, ProductValue, Timestamp};
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_primitives::{IndexId, TableId};
    use spacetimedb_sats::{product, u256};
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::mpsc::{self, Receiver};

    fn add_subscriber(db: Arc<RelationalDB>, sql: &str, assert: Option<AssertTxFn>) -> Result<(), DBError> {
        let owner = Identity::from_byte_array([1; 32]);
        let client = ClientActorId::for_test(Identity::ZERO);
        let config = ClientConfig::for_test();
        let sender = Arc::new(ClientConnectionSender::dummy(client, config));
        let module_subscriptions =
            ModuleSubscriptions::new(db.clone(), Arc::new(RwLock::new(SubscriptionManager::default())), owner);

        let subscribe = Subscribe {
            query_strings: [sql.into()].into(),
            request_id: 0,
        };
        module_subscriptions.add_legacy_subscriber(sender, subscribe, Instant::now(), assert)
    }

    /// An in-memory `RelationalDB` for testing
    fn relational_db() -> anyhow::Result<Arc<RelationalDB>> {
        let TestDB { db, .. } = TestDB::in_memory()?;
        Ok(Arc::new(db))
    }

    /// Initialize a module [SubscriptionManager]
    fn module_subscriptions(db: Arc<RelationalDB>) -> ModuleSubscriptions {
        ModuleSubscriptions::new(
            db,
            Arc::new(RwLock::new(SubscriptionManager::default())),
            Identity::ZERO,
        )
    }

    /// Return a client connection for testing
    fn sender_with_rx() -> (Arc<ClientConnectionSender>, mpsc::Receiver<SerializableMessage>) {
        let client = ClientActorId::for_test(Identity::ZERO);
        let config = ClientConfig::for_test();
        let (sender, rx) = ClientConnectionSender::dummy_with_channel(client, config);
        (Arc::new(sender), rx)
    }

    /// A [SubscribeSingle] message for testing
    fn single_subscribe(sql: &str, query_id: u32) -> SubscribeSingle {
        SubscribeSingle {
            query: sql.into(),
            request_id: 0,
            query_id: QueryId::new(query_id),
        }
    }

    /// A [SubscribeMulti] message for testing
    fn multi_subscribe(query_strings: &[&'static str], query_id: u32) -> SubscribeMulti {
        SubscribeMulti {
            query_strings: query_strings
                .iter()
                .map(|sql| String::from(*sql).into_boxed_str())
                .collect(),
            request_id: 0,
            query_id: QueryId::new(query_id),
        }
    }

    /// An [Unsubscribe] message for testing
    fn single_unsubscribe(query_id: u32) -> Unsubscribe {
        Unsubscribe {
            request_id: 0,
            query_id: QueryId::new(query_id),
        }
    }

    /// A dummy [ModuleEvent] for testing
    fn module_event() -> ModuleEvent {
        ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: Identity::ZERO,
            caller_connection_id: None,
            function_call: ModuleFunctionCall::default(),
            status: EventStatus::Committed(DatabaseUpdate::default()),
            energy_quanta_used: EnergyQuanta { quanta: 0 },
            host_execution_duration: Duration::from_millis(0),
            request_id: None,
            timer: None,
        }
    }

    /// Creates a single row, single column table with an index
    fn create_table_with_index(db: &RelationalDB, name: &str) -> anyhow::Result<(TableId, IndexId)> {
        let table_id = db.create_table_for_test(name, &[("id", AlgebraicType::U64)], &[0.into()])?;
        let index_id = db.with_read_only(Workload::ForTests, |tx| {
            db.schema_for_table(tx, table_id)?
                .indexes
                .iter()
                .find(|schema| {
                    schema
                        .index_algorithm
                        .columns()
                        .as_singleton()
                        .is_some_and(|col_id| col_id.idx() == 0)
                })
                .map(|schema| schema.index_id)
                .ok_or_else(|| anyhow::anyhow!("Index not found for ColId `{}`", 0))
        })?;
        db.with_auto_commit(Workload::ForTests, |tx| {
            db.insert(tx, table_id, &bsatn::to_vec(&product![1_u64])?)?;
            Ok((table_id, index_id))
        })
    }

    #[test]
    fn test_subscribe_metrics() -> anyhow::Result<()> {
        let (sender, _) = sender_with_rx();
        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with index on `id`
        create_table_with_index(&db, "t")?;

        let auth = AuthCtx::for_testing();
        let sql = "select * from t where id = 1";
        let tx = db.begin_tx(Workload::ForTests);
        let plan = compile_read_only_query(&auth, &tx, sql)?;
        let plan = Arc::new(plan);

        let (_, metrics) = subs.evaluate_queries(sender, &vec![plan], &tx, &auth, TableUpdateType::Subscribe)?;

        // We only probe the index once
        assert_eq!(metrics.index_seeks, 1);
        // We scan a single u64 when serializing the result
        assert_eq!(metrics.bytes_scanned, 8);
        // Subscriptions are read-only
        assert_eq!(metrics.bytes_written, 0);
        // Bytes scanned and bytes sent will always be the same for an initial subscription,
        // because a subscription is initiated by a single client.
        assert_eq!(metrics.bytes_sent_to_clients, 8);

        // Note, rows scanned may be greater than one.
        // It depends on the number of operators used to answer the query.
        assert!(metrics.rows_scanned > 0);
        Ok(())
    }

    #[tokio::test]
    async fn subscribe_error() -> anyhow::Result<()> {
        let (sender, mut rx) = sender_with_rx();
        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with index on `id`
        create_table_with_index(&db, "t")?;

        let subscribe = || -> anyhow::Result<()> {
            // Invalid query: t does not have a field x
            let sql = "select * from t where x = 1";
            subs.add_single_subscription(sender.clone(), single_subscribe(sql, 0), Instant::now(), None)?;
            Ok(())
        };

        subscribe()?;

        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Error(..),
                ..
            }))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn unsubscribe_error() -> anyhow::Result<()> {
        let (sender, mut rx) = sender_with_rx();
        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with an index on `id`
        let (_, index_id) = create_table_with_index(&db, "t")?;

        let subscribe = || -> anyhow::Result<()> {
            let sql = "select * from t where id = 1";
            subs.add_single_subscription(sender.clone(), single_subscribe(sql, 0), Instant::now(), None)?;
            Ok(())
        };

        let unsubscribe = || -> anyhow::Result<()> {
            subs.remove_single_subscription(sender.clone(), single_unsubscribe(0), Instant::now())?;
            Ok(())
        };

        subscribe()?;

        // The initial subscription should succeed
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Subscribe(..),
                ..
            }))
        ));

        // Remove the index from `id`
        db.with_auto_commit(Workload::ForTests, |tx| db.drop_index(tx, index_id))?;

        unsubscribe()?;

        // Why does the unsubscribe fail?
        // This relies on some knowledge of the underlying implementation.
        // Specifically that we do not recompile queries on unsubscribe.
        // We execute the cached plan which in this case is an index scan.
        // The index no longer exists, and therefore it fails.
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Error(..),
                ..
            }))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn tx_update_error() -> anyhow::Result<()> {
        let (sender, mut rx) = sender_with_rx();
        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create two tables `t` and `s` with indexes on their `id` columns
        let (table_id, _index_id) = create_table_with_index(&db, "t")?;
        let (_table_id, index_id) = create_table_with_index(&db, "s")?;

        let subscribe = || -> anyhow::Result<()> {
            let sql = "select t.* from t join s on t.id = s.id";
            subs.add_single_subscription(sender.clone(), single_subscribe(sql, 0), Instant::now(), None)?;
            Ok(())
        };

        subscribe()?;

        // The initial subscription should succeed
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Subscribe(..),
                ..
            }))
        ));

        // Remove the index from `s`
        db.with_auto_commit(Workload::ForTests, |tx| db.drop_index(tx, index_id))?;

        // Start a new transaction and insert a new row into `t`
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        db.insert(&mut tx, table_id, &bsatn::to_vec(&product![2_u64])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        // Why does the update fail?
        // This relies on some knowledge of the underlying implementation.
        // Specifically, plans are cached on the initial subscribe.
        // Hence we execute a cached plan which happens to be an index join.
        // We've removed the index on `s`, and therefore it fails.
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Error(..),
                ..
            }))
        ));
        Ok(())
    }

    /// In this test we have two clients issue parameterized subscriptions.
    /// These subscriptions are identical syntactically but not semantically,
    /// because they are parameterized by `:sender` - the caller's identity.
    #[tokio::test]
    async fn test_parameterized_subscription() -> anyhow::Result<()> {
        let client_0_identity = Identity::from_u256(u256::MAX);
        let client_1_identity = Identity::from_u256(u256::ONE);
        let client_0_config = ClientConfig {
            protocol: Protocol::Binary,
            compression: Compression::None,
            tx_update_full: true,
        };
        let client_1_config = ClientConfig {
            protocol: Protocol::Binary,
            compression: Compression::None,
            tx_update_full: true,
        };
        let client_0 = ClientActorId {
            identity: client_0_identity,
            connection_id: ConnectionId::from_u128(0),
            name: ClientName(0),
        };
        let client_1 = ClientActorId {
            identity: client_1_identity,
            connection_id: ConnectionId::from_u128(1),
            name: ClientName(1),
        };
        let (sender_0, mut rx_0) = ClientConnectionSender::dummy_with_channel(client_0, client_0_config);
        let (sender_1, mut rx_1) = ClientConnectionSender::dummy_with_channel(client_1, client_1_config);
        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create an empty table with an `Identity` column
        let table_id = db.create_table_for_test("t", &[("identity", AlgebraicType::identity())], &[])?;

        let subscribe = |sender, query_id| -> anyhow::Result<()> {
            let sql = "select * from t where identity = :sender";
            subs.add_multi_subscription(sender, multi_subscribe(&[sql], query_id), Instant::now(), None)?;
            Ok(())
        };

        let client_0_query_id = 1;
        let client_1_query_id = 2;

        subscribe(Arc::new(sender_0), client_0_query_id)?;
        subscribe(Arc::new(sender_1), client_1_query_id)?;

        /// Wait for the initial subscription
        async fn wait(rx: &mut Receiver<SerializableMessage>) {
            assert!(matches!(rx.recv().await, Some(SerializableMessage::Subscription(_))))
        }

        // Wait for both subscriptions
        wait(&mut rx_0).await;
        wait(&mut rx_1).await;

        // Insert two identities - one for each caller - into the table
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        db.insert(&mut tx, table_id, &bsatn::to_vec(&product![client_0_identity])?)?;
        db.insert(&mut tx, table_id, &bsatn::to_vec(&product![client_1_identity])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        /// Assert that we get the expected identity from the receiver
        async fn assert_identity(table_id: TableId, identity: Identity, rx: &mut Receiver<SerializableMessage>) {
            match rx.recv().await {
                Some(SerializableMessage::TxUpdate(TransactionUpdateMessage {
                    database_update:
                        SubscriptionUpdateMessage {
                            database_update: FormatSwitch::Bsatn(ws::DatabaseUpdate { mut tables }),
                            ..
                        },
                    ..
                })) => {
                    assert_eq!(tables.len(), 1);
                    let mut table_update = tables.pop().unwrap();

                    assert_eq!(table_update.table_id, table_id);
                    assert_eq!(table_update.num_rows, 1);
                    assert_eq!(table_update.updates.len(), 1);

                    let CompressableQueryUpdate::Uncompressed(table_update) = table_update.updates.pop().unwrap()
                    else {
                        panic!("expected an uncompressed table update")
                    };

                    assert!(table_update.deletes.is_empty());
                    assert_eq!(table_update.inserts.len(), 1);

                    let typ = ProductType::from([AlgebraicType::identity()]);
                    let raw = table_update.inserts.into_iter().next().unwrap();
                    let row = ProductValue::decode(&typ, &mut &*raw).unwrap();

                    assert_eq!(row, product![identity]);
                }
                _ => panic!("expected a transaction update"),
            }
        }

        // Assert that each connection receives the correct update
        assert_identity(table_id, client_0_identity, &mut rx_0).await;
        assert_identity(table_id, client_1_identity, &mut rx_1).await;
        Ok(())
    }

    /// Asserts that a subscription holds a tx handle for the entire length of its evaluation.
    #[test]
    fn test_tx_subscription_ordering() -> ResultTest<()> {
        let test_db = TestDB::durable()?;

        let runtime = test_db.runtime().cloned().unwrap();
        let db = Arc::new(test_db.db.clone());

        // Create table with one row
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(Workload::ForTests, |tx| {
            insert(&db, tx, table_id, &product!(1_u8)).map(drop)
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
                    // Assuming subscription evaluation holds a lock on the db,
                    // any mutations to T will necessarily occur after,
                    // and therefore we should only see a single row returned.
                    assert_eq!(1, db.iter(tx, table_id).unwrap().count());
                })),
            )
        });

        // Write a second row to T concurrently with the reader thread
        let write_handle = runtime.spawn(async move {
            let _ = recv.recv().await;
            db2.with_auto_commit(Workload::ForTests, |tx| {
                insert(&db2, tx, table_id, &product!(2_u8)).map(drop)
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
        let indexes = &[0.into()];
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
            "SELECT * FROM private WHERE false",
            "SELECT private.* FROM private",
            "SELECT public.* FROM public JOIN private ON public.a = private.a WHERE private.a = 1",
            "SELECT private.* FROM private JOIN public ON private.a = public.a WHERE public.a = 1",
        ] {
            assert!(subscribe(sql).is_err(),);
        }

        Ok(())
    }
}
