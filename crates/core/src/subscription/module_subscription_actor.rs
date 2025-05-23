use super::execution_unit::QueryHash;
use super::module_subscription_manager::{Plan, SubscriptionGaugeStats, SubscriptionManager};
use super::query::compile_query_with_hashes;
use super::tx::DeltaTx;
use super::{collect_table_update, TableUpdateType};
use crate::client::messages::{
    SubscriptionData, SubscriptionError, SubscriptionMessage, SubscriptionResult, SubscriptionRows,
    SubscriptionUpdateMessage, TransactionUpdateMessage,
};
use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
use crate::db::datastore::locking_tx_datastore::datastore::report_tx_metricses;
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::db_metrics::DB_METRICS;
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::execution_context::Workload;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::messages::websocket::Subscribe;
use crate::subscription::execute_plans;
use crate::subscription::query::is_subscribe_to_all_tables;
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use prometheus::IntGauge;
use spacetimedb_client_api_messages::websocket::{
    self as ws, BsatnFormat, FormatSwitch, JsonFormat, SubscribeMulti, SubscribeSingle, TableUpdate, Unsubscribe,
    UnsubscribeMulti,
};
use spacetimedb_execution::pipelined::PipelinedProject;
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
    stats: Arc<SubscriptionGauges>,
}

#[derive(Debug, Clone)]
pub struct SubscriptionGauges {
    db_identity: Identity,
    num_queries: IntGauge,
    num_connections: IntGauge,
    num_subscription_sets: IntGauge,
    num_query_subscriptions: IntGauge,
    num_legacy_subscriptions: IntGauge,
}

impl SubscriptionGauges {
    fn new(db_identity: &Identity) -> Self {
        let num_queries = WORKER_METRICS.subscription_queries.with_label_values(db_identity);
        let num_connections = DB_METRICS.subscription_connections.with_label_values(db_identity);
        let num_subscription_sets = DB_METRICS.subscription_sets.with_label_values(db_identity);
        let num_query_subscriptions = DB_METRICS.total_query_subscriptions.with_label_values(db_identity);
        let num_legacy_subscriptions = DB_METRICS.num_legacy_subscriptions.with_label_values(db_identity);
        Self {
            db_identity: *db_identity,
            num_queries,
            num_connections,
            num_subscription_sets,
            num_query_subscriptions,
            num_legacy_subscriptions,
        }
    }

    // Clear the subscription gauges for this database.
    fn unregister(&self) {
        let _ = WORKER_METRICS
            .subscription_queries
            .remove_label_values(&self.db_identity);
        let _ = DB_METRICS
            .subscription_connections
            .remove_label_values(&self.db_identity);
        let _ = DB_METRICS.subscription_sets.remove_label_values(&self.db_identity);
        let _ = DB_METRICS
            .total_query_subscriptions
            .remove_label_values(&self.db_identity);
        let _ = DB_METRICS
            .num_legacy_subscriptions
            .remove_label_values(&self.db_identity);
    }

    fn report(&self, stats: &SubscriptionGaugeStats) {
        self.num_queries.set(stats.num_queries as i64);
        self.num_connections.set(stats.num_connections as i64);
        self.num_subscription_sets.set(stats.num_subscription_sets as i64);
        self.num_query_subscriptions.set(stats.num_query_subscriptions as i64);
        self.num_legacy_subscriptions.set(stats.num_legacy_subscriptions as i64);
    }
}

type AssertTxFn = Arc<dyn Fn(&Tx)>;
type SubscriptionUpdate = FormatSwitch<TableUpdate<BsatnFormat>, TableUpdate<JsonFormat>>;
type FullSubscriptionUpdate = FormatSwitch<ws::DatabaseUpdate<BsatnFormat>, ws::DatabaseUpdate<JsonFormat>>;

/// A utility for sending an error message to a client and returning early
macro_rules! return_on_err {
    ($expr:expr, $handler:expr, $metrics:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                // TODO: Handle errors sending messages.
                let _ = $handler(e.to_string().into());
                return Ok($metrics);
            }
        }
    };
}

/// A utility for sending an error message to a client and returning early
macro_rules! return_on_err_with_sql {
    ($expr:expr, $sql:expr, $handler:expr) => {
        match $expr.map_err(|err| DBError::WithSql {
            sql: $sql.into(),
            error: Box::new(DBError::Other(err.into())),
        }) {
            Ok(val) => val,
            Err(e) => {
                // TODO: Handle errors sending messages.
                let _ = $handler(e.to_string().into());
                return Ok(None);
            }
        }
    };
}

impl ModuleSubscriptions {
    pub fn new(relational_db: Arc<RelationalDB>, subscriptions: Subscriptions, owner_identity: Identity) -> Self {
        let db = &relational_db.database_identity();
        let stats = Arc::new(SubscriptionGauges::new(db));

        Self {
            relational_db,
            subscriptions,
            owner_identity,
            stats,
        }
    }

    // Recompute gauges to update metrics.
    pub fn update_gauges(&self) {
        let num_queries = self.subscriptions.read().calculate_gauge_stats();
        self.stats.report(&num_queries);
    }

    // Remove the subscription gauges for this database.
    // TODO: This should be called when the database is shut down.
    pub fn remove_gauges(&self) {
        self.stats.unregister();
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
            &[&query],
            &self.relational_db,
            tx,
            |plan, tx| {
                plan.plans_fragments()
                    .map(|plan_fragment| estimate_rows_scanned(tx, plan_fragment.physical_plan()))
                    .fold(0, |acc, rows_scanned| acc.saturating_add(rows_scanned))
            },
            auth,
        )?;

        let table_id = query.subscribed_table_id();
        let table_name = query.subscribed_table_name();

        let plans = query
            .plans_fragments()
            .map(|fragment| fragment.physical_plan())
            .cloned()
            .map(|plan| plan.optimize())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(PipelinedProject::from)
            .collect::<Vec<_>>();

        let tx = DeltaTx::from(tx);

        Ok(match sender.config.protocol {
            Protocol::Binary => collect_table_update(&plans, table_id, table_name.into(), &tx, update_type)
                .map(|(table_update, metrics)| (FormatSwitch::Bsatn(table_update), metrics)),
            Protocol::Text => collect_table_update(&plans, table_id, table_name.into(), &tx, update_type)
                .map(|(table_update, metrics)| (FormatSwitch::Json(table_update), metrics)),
        }?)
    }

    fn evaluate_queries(
        &self,
        sender: Arc<ClientConnectionSender>,
        queries: &[Arc<Plan>],
        tx: &TxId,
        auth: &AuthCtx,
        update_type: TableUpdateType,
    ) -> Result<(FullSubscriptionUpdate, ExecutionMetrics), DBError> {
        check_row_limit(
            queries,
            &self.relational_db,
            tx,
            |plan, tx| {
                plan.plans_fragments()
                    .map(|plan_fragment| estimate_rows_scanned(tx, plan_fragment.physical_plan()))
                    .fold(0, |acc, rows_scanned| acc.saturating_add(rows_scanned))
            },
            auth,
        )?;

        let tx = DeltaTx::from(tx);
        match sender.config.protocol {
            Protocol::Binary => {
                let (update, metrics) = execute_plans(queries, &tx, update_type)?;
                Ok((FormatSwitch::Bsatn(update), metrics))
            }
            Protocol::Text => {
                let (update, metrics) = execute_plans(queries, &tx, update_type)?;
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
    ) -> Result<Option<ExecutionMetrics>, DBError> {
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

        let sql = request.query;
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let hash = QueryHash::from_string(&sql, auth.caller, false);
        let hash_with_param = QueryHash::from_string(&sql, auth.caller, true);

        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });

        let existing_query = {
            let guard = self.subscriptions.read();
            guard.query(&hash)
        };

        let query = return_on_err_with_sql!(
            existing_query.map(Ok).unwrap_or_else(|| compile_query_with_hashes(
                &auth,
                &tx,
                &sql,
                hash,
                hash_with_param
            )
            .map(Arc::new)),
            sql,
            send_err_msg
        );

        let (table_rows, metrics) = return_on_err_with_sql!(
            self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth, TableUpdateType::Subscribe),
            query.sql(),
            send_err_msg
        );

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.add_subscription(sender.clone(), query.clone(), request.query_id)?;

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
        Ok(Some(metrics))
    }

    /// Remove a subscription for a single query.
    pub fn remove_single_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: Unsubscribe,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
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

        let queries = return_on_err!(
            subscriptions.remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id),
            // Apparently we ignore errors sending messages.
            send_err_msg,
            None
        );
        // This is technically a bug, since this could be empty if the client has another duplicate subscription.
        // This whole function should be removed soon, so I don't think we need to fix it.
        let [query] = &*queries else {
            // Apparently we ignore errors sending messages.
            let _ = send_err_msg("Internal error".into());
            return Ok(None);
        };

        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Unsubscribe), |tx| {
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let (table_rows, metrics) = return_on_err_with_sql!(
            self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth, TableUpdateType::Unsubscribe),
            query.sql(),
            send_err_msg
        );

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
        Ok(Some(metrics))
    }

    /// Remove a client's subscription for a set of queries.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn remove_multi_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: UnsubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
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
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });

        let removed_queries = {
            let mut subscriptions = self.subscriptions.write();

            return_on_err!(
                subscriptions.remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id),
                send_err_msg,
                None
            )
        };

        let (update, metrics) = return_on_err!(
            self.evaluate_queries(
                sender.clone(),
                &removed_queries,
                &tx,
                &AuthCtx::new(self.owner_identity, sender.id.identity),
                TableUpdateType::Unsubscribe,
            ),
            send_err_msg,
            None
        );

        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::UnsubscribeMulti(SubscriptionData { data: update }),
        });

        Ok(Some(metrics))
    }

    /// Compiles the queries in a [Subscribe] or [SubscribeMulti] message.
    ///
    /// Note, we hash queries to avoid recompilation,
    /// but we need to know if a query is parameterized in order to hash it correctly.
    /// This requires that we type check which in turn requires that we start a tx.
    ///
    /// Unfortunately parsing with sqlparser is quite expensive,
    /// so we'd like to avoid that cost while holding the tx lock,
    /// especially since all we're trying to do is generate a hash.
    ///
    /// Instead we generate two hashes and outside of the tx lock.
    /// If either one is currently tracked, we can avoid recompilation.
    fn compile_queries(
        &self,
        sender: Identity,
        queries: impl IntoIterator<Item = Box<str>>,
        num_queries: usize,
    ) -> Result<(Vec<Arc<Plan>>, AuthCtx, TxId), DBError> {
        let mut subscribe_to_all_tables = false;
        let mut plans = Vec::with_capacity(num_queries);
        let mut query_hashes = Vec::with_capacity(num_queries);

        for sql in queries {
            if is_subscribe_to_all_tables(&sql) {
                subscribe_to_all_tables = true;
                continue;
            }
            let hash = QueryHash::from_string(&sql, sender, false);
            let hash_with_param = QueryHash::from_string(&sql, sender, true);
            query_hashes.push((sql, hash, hash_with_param));
        }

        let auth = AuthCtx::new(self.owner_identity, sender);

        // We always get the db lock before the subscription lock to avoid deadlocks.
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });
        let guard = self.subscriptions.read();

        if subscribe_to_all_tables {
            plans.extend(
                super::subscription::get_all(&self.relational_db, &tx, &auth)?
                    .into_iter()
                    .map(Arc::new),
            );
        }

        for (sql, hash, hash_with_param) in query_hashes {
            if let Some(unit) = guard.query(&hash) {
                plans.push(unit);
            } else if let Some(unit) = guard.query(&hash_with_param) {
                plans.push(unit);
            } else {
                plans.push(Arc::new(
                    compile_query_with_hashes(&auth, &tx, &sql, hash, hash_with_param).map_err(|err| {
                        DBError::WithSql {
                            error: Box::new(DBError::Other(err.into())),
                            sql,
                        }
                    })?,
                ));
            }
        }

        Ok((plans, auth, scopeguard::ScopeGuard::into_inner(tx)))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn add_multi_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: SubscribeMulti,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
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

        let num_queries = request.query_strings.len();
        let (queries, auth, tx) = return_on_err!(
            self.compile_queries(sender.id.identity, request.query_strings, num_queries),
            send_err_msg,
            None
        );
        let tx = scopeguard::guard(tx, |tx| {
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });

        // We minimize locking so that other clients can add subscriptions concurrently.
        // We are protected from race conditions with broadcasts, because we have the db lock,
        // an `commit_and_broadcast_event` grabs a read lock on `subscriptions` while it still has a
        // write lock on the db.
        let queries = {
            let mut subscriptions = self.subscriptions.write();

            subscriptions.add_subscription_multi(sender.clone(), queries, request.query_id)?
        };

        let Ok((update, metrics)) =
            self.evaluate_queries(sender.clone(), &queries, &tx, &auth, TableUpdateType::Subscribe)
        else {
            // If we fail the query, we need to remove the subscription.
            let mut subscriptions = self.subscriptions.write();
            subscriptions.remove_subscription((sender.id.identity, sender.id.connection_id), request.query_id)?;
            send_err_msg("Internal error evaluating queries".into());
            return Ok(None);
        };

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

        Ok(Some(metrics))
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
    ) -> Result<ExecutionMetrics, DBError> {
        let num_queries = subscription.query_strings.len();
        let (queries, auth, tx) = self.compile_queries(sender.id.identity, subscription.query_strings, num_queries)?;
        let tx = scopeguard::guard(tx, |tx| {
            let (tx_metrics, reducer) = self.relational_db.release_tx(tx);
            tx_metrics.report_with_db(&reducer, &self.relational_db, None);
        });

        check_row_limit(
            &queries,
            &self.relational_db,
            &tx,
            |plan, tx| {
                plan.plans_fragments()
                    .map(|plan_fragment| estimate_rows_scanned(tx, plan_fragment.physical_plan()))
                    .fold(0, |acc, rows_scanned| acc.saturating_add(rows_scanned))
            },
            &auth,
        )?;

        let tx = DeltaTx::from(&*tx);
        let (database_update, metrics) = match sender.config.protocol {
            Protocol::Binary => execute_plans(&queries, &tx, TableUpdateType::Subscribe)
                .map(|(table_update, metrics)| (FormatSwitch::Bsatn(table_update), metrics))?,
            Protocol::Text => execute_plans(&queries, &tx, TableUpdateType::Subscribe)
                .map(|(table_update, metrics)| (FormatSwitch::Json(table_update), metrics))?,
        };

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.set_legacy_subscription(sender.clone(), queries.into_iter());

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
            request_id: Some(subscription.request_id),
            timer: Some(timer),
        });

        Ok(metrics)
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) {
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_all_subscriptions(&(client_id.identity, client_id.connection_id));
    }

    /// Commit a transaction and broadcast its ModuleEvent to all interested subscribers.
    ///
    /// The returned [`ExecutionMetrics`] are reported in this method via `report_tx_metrics`.
    /// They are returned for testing purposes but should not be reported separately.
    pub fn commit_and_broadcast_event(
        &self,
        caller: Option<&ClientConnectionSender>,
        mut event: ModuleEvent,
        tx: MutTx,
    ) -> Result<Result<(Arc<ModuleEvent>, ExecutionMetrics), WriteConflict>, DBError> {
        // Take a read lock on `subscriptions` before committing tx
        // else it can result in subscriber receiving duplicate updates.
        let subscriptions = self.subscriptions.read();
        let stdb = &self.relational_db;
        // Downgrade mutable tx.
        // We'll later ensure tx is released/cleaned up once out of scope.
        let (read_tx, tx_data, tx_metrics_mut) = match &mut event.status {
            EventStatus::Committed(db_update) => {
                let Some((tx_data, tx_metrics, read_tx)) = stdb.commit_tx_downgrade(tx, Workload::Update)? else {
                    return Ok(Err(WriteConflict));
                };
                *db_update = DatabaseUpdate::from_writes(&tx_data);
                (read_tx, Some(tx_data), tx_metrics)
            }
            EventStatus::Failed(_) | EventStatus::OutOfEnergy => {
                let (tx_metrics, tx) = stdb.rollback_mut_tx_downgrade(tx, Workload::Update);
                (tx, None, tx_metrics)
            }
        };

        // When we're done with this method, release the tx and report metrics.
        let mut read_tx = scopeguard::guard(read_tx, |tx| {
            let (tx_metrics_read, reducer) = self.relational_db.release_tx(tx);
            report_tx_metricses(
                &reducer,
                stdb,
                tx_data.as_ref(),
                Some(&tx_metrics_mut),
                &tx_metrics_read,
            );
        });
        // Create the delta transaction we'll use to eval updates against.
        let delta_read_tx = tx_data
            .as_ref()
            .map(|tx_data| DeltaTx::new(&read_tx, tx_data))
            .unwrap_or_else(|| DeltaTx::from(&*read_tx));

        let event = Arc::new(event);
        let mut update_metrics: ExecutionMetrics = ExecutionMetrics::default();

        match &event.status {
            EventStatus::Committed(_) => {
                update_metrics = subscriptions.eval_updates_sequential(&delta_read_tx, event.clone(), caller);
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

        // Merge in the subscription evaluation metrics.
        read_tx.metrics.merge(update_metrics);

        Ok(Ok((event, update_metrics)))
    }
}

pub struct WriteConflict;

#[cfg(test)]
mod tests {
    use super::{AssertTxFn, ModuleSubscriptions};
    use crate::client::messages::{
        SerializableMessage, SubscriptionData, SubscriptionError, SubscriptionMessage, SubscriptionResult,
        SubscriptionUpdateMessage, TransactionUpdateMessage,
    };
    use crate::client::{ClientActorId, ClientConfig, ClientConnectionSender, ClientName, Protocol};
    use crate::db::datastore::system_tables::{StRowLevelSecurityRow, ST_ROW_LEVEL_SECURITY_ID};
    use crate::db::relational_db::tests_utils::{
        begin_mut_tx, begin_tx, insert, with_auto_commit, with_read_only, TestDB,
    };
    use crate::db::relational_db::RelationalDB;
    use crate::error::DBError;
    use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
    use crate::messages::websocket as ws;
    use crate::subscription::query::compile_read_only_query;
    use crate::subscription::TableUpdateType;
    use hashbrown::HashMap;
    use itertools::Itertools;
    use pretty_assertions::assert_matches;
    use spacetimedb_client_api_messages::energy::EnergyQuanta;
    use spacetimedb_client_api_messages::websocket::{
        CompressableQueryUpdate, Compression, FormatSwitch, QueryId, Subscribe, SubscribeMulti, SubscribeSingle,
        TableUpdate, Unsubscribe, UnsubscribeMulti,
    };
    use spacetimedb_execution::dml::MutDatastore;
    use spacetimedb_lib::bsatn::ToBsatn;
    use spacetimedb_lib::db::auth::StAccess;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::metrics::ExecutionMetrics;
    use spacetimedb_lib::{bsatn, ConnectionId, ProductType, ProductValue, Timestamp};
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_primitives::TableId;
    use spacetimedb_sats::product;
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::mpsc::{self, Receiver};

    fn add_subscriber(db: Arc<RelationalDB>, sql: &str, assert: Option<AssertTxFn>) -> Result<(), DBError> {
        let owner = Identity::from_byte_array([1; 32]);
        let client = ClientActorId::for_test(Identity::ZERO);
        let config = ClientConfig::for_test();
        let sender = Arc::new(ClientConnectionSender::dummy(client, config));
        let module_subscriptions = ModuleSubscriptions::new(db.clone(), <_>::default(), owner);

        let subscribe = Subscribe {
            query_strings: [sql.into()].into(),
            request_id: 0,
        };
        module_subscriptions.add_legacy_subscriber(sender, subscribe, Instant::now(), assert)?;
        Ok(())
    }

    /// An in-memory `RelationalDB` for testing
    fn relational_db() -> anyhow::Result<Arc<RelationalDB>> {
        let TestDB { db, .. } = TestDB::in_memory()?;
        Ok(Arc::new(db))
    }

    /// Initialize a module [SubscriptionManager]
    fn module_subscriptions(db: Arc<RelationalDB>) -> ModuleSubscriptions {
        ModuleSubscriptions::new(db, <_>::default(), Identity::ZERO)
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

    /// A [SubscribeMulti] message for testing
    fn multi_unsubscribe(query_id: u32) -> UnsubscribeMulti {
        UnsubscribeMulti {
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

    /// Create an [Identity] from a [u8]
    fn identity_from_u8(v: u8) -> Identity {
        Identity::from_byte_array([v; 32])
    }

    /// Create an [ConnectionId] from a [u8]
    fn connection_id_from_u8(v: u8) -> ConnectionId {
        ConnectionId::from_be_byte_array([v; 16])
    }

    /// Create an [ClientActorId] from a [u8].
    /// Calls [identity_from_u8] internally with the passed value.
    fn client_id_from_u8(v: u8) -> ClientActorId {
        ClientActorId {
            identity: identity_from_u8(v),
            connection_id: connection_id_from_u8(v),
            name: ClientName(v as u64),
        }
    }

    /// Instantiate a client connection with compression
    fn client_connection_with_compression(
        client_id: ClientActorId,
        compression: Compression,
    ) -> (Arc<ClientConnectionSender>, Receiver<SerializableMessage>) {
        let (sender, rx) = ClientConnectionSender::dummy_with_channel(
            client_id,
            ClientConfig {
                protocol: Protocol::Binary,
                compression,
                tx_update_full: true,
            },
        );
        (Arc::new(sender), rx)
    }

    /// Instantiate a client connection
    fn client_connection(client_id: ClientActorId) -> (Arc<ClientConnectionSender>, Receiver<SerializableMessage>) {
        client_connection_with_compression(client_id, Compression::None)
    }

    /// Insert rules into the RLS system table
    fn insert_rls_rules(
        db: &RelationalDB,
        table_ids: impl IntoIterator<Item = TableId>,
        rules: impl IntoIterator<Item = &'static str>,
    ) -> anyhow::Result<()> {
        with_auto_commit(db, |tx| {
            for (table_id, sql) in table_ids.into_iter().zip(rules) {
                db.insert(
                    tx,
                    ST_ROW_LEVEL_SECURITY_ID,
                    &ProductValue::from(StRowLevelSecurityRow {
                        table_id,
                        sql: sql.into(),
                    })
                    .to_bsatn_vec()?,
                )?;
            }
            Ok(())
        })
    }

    /// Subscribe to a query as a client
    fn subscribe_single(
        subs: &ModuleSubscriptions,
        sql: &'static str,
        sender: Arc<ClientConnectionSender>,
        counter: &mut u32,
    ) -> anyhow::Result<()> {
        *counter += 1;
        subs.add_single_subscription(sender, single_subscribe(sql, *counter), Instant::now(), None)?;
        Ok(())
    }

    /// Subscribe to a set of queries as a client
    fn subscribe_multi(
        subs: &ModuleSubscriptions,
        queries: &[&'static str],
        sender: Arc<ClientConnectionSender>,
        counter: &mut u32,
    ) -> anyhow::Result<ExecutionMetrics> {
        *counter += 1;
        let metrics = subs
            .add_multi_subscription(sender, multi_subscribe(queries, *counter), Instant::now(), None)
            .map(|metrics| metrics.unwrap_or_default())?;
        Ok(metrics)
    }

    /// Unsubscribe from a single query
    fn unsubscribe_single(
        subs: &ModuleSubscriptions,
        sender: Arc<ClientConnectionSender>,
        query_id: u32,
    ) -> anyhow::Result<()> {
        subs.remove_single_subscription(sender, single_unsubscribe(query_id), Instant::now())?;
        Ok(())
    }

    /// Unsubscribe from a set of queries
    fn unsubscribe_multi(
        subs: &ModuleSubscriptions,
        sender: Arc<ClientConnectionSender>,
        query_id: u32,
    ) -> anyhow::Result<()> {
        subs.remove_multi_subscription(sender, multi_unsubscribe(query_id), Instant::now())?;
        Ok(())
    }

    /// Pull a message from receiver and assert that it is a `TxUpdate` with the expected rows
    async fn assert_tx_update_for_table(
        rx: &mut Receiver<SerializableMessage>,
        table_id: TableId,
        schema: &ProductType,
        inserts: impl IntoIterator<Item = ProductValue>,
        deletes: impl IntoIterator<Item = ProductValue>,
    ) {
        match rx.recv().await {
            Some(SerializableMessage::TxUpdate(TransactionUpdateMessage {
                database_update:
                    SubscriptionUpdateMessage {
                        database_update: FormatSwitch::Bsatn(ws::DatabaseUpdate { mut tables }),
                        ..
                    },
                ..
            })) => {
                // Assume an update for only one table
                assert_eq!(tables.len(), 1);

                let table_update = tables.pop().unwrap();

                // We should not be sending empty updates to clients
                assert_ne!(table_update.num_rows, 0);

                // It should be the table we expect
                assert_eq!(table_update.table_id, table_id);

                let mut rows_received: HashMap<ProductValue, i32> = HashMap::new();

                for uncompressed in table_update.updates {
                    let CompressableQueryUpdate::Uncompressed(table_update) = uncompressed else {
                        panic!("expected an uncompressed table update")
                    };

                    for row in table_update
                        .inserts
                        .into_iter()
                        .map(|bytes| ProductValue::decode(schema, &mut &*bytes).unwrap())
                    {
                        *rows_received.entry(row).or_insert(0) += 1;
                    }

                    for row in table_update
                        .deletes
                        .into_iter()
                        .map(|bytes| ProductValue::decode(schema, &mut &*bytes).unwrap())
                    {
                        *rows_received.entry(row).or_insert(0) -= 1;
                    }
                }

                assert_eq!(
                    rows_received
                        .iter()
                        .filter(|(_, n)| n > &&0)
                        .map(|(row, _)| row)
                        .cloned()
                        .sorted()
                        .collect::<Vec<_>>(),
                    inserts.into_iter().sorted().collect::<Vec<_>>()
                );
                assert_eq!(
                    rows_received
                        .iter()
                        .filter(|(_, n)| n < &&0)
                        .map(|(row, _)| row)
                        .cloned()
                        .sorted()
                        .collect::<Vec<_>>(),
                    deletes.into_iter().sorted().collect::<Vec<_>>()
                );
            }
            Some(msg) => panic!("expected a TxUpdate, but got {:#?}", msg),
            None => panic!("The receiver closed due to an error"),
        }
    }

    /// Commit a set of row updates and broadcast to subscribers
    fn commit_tx(
        db: &RelationalDB,
        subs: &ModuleSubscriptions,
        deletes: impl IntoIterator<Item = (TableId, ProductValue)>,
        inserts: impl IntoIterator<Item = (TableId, ProductValue)>,
    ) -> anyhow::Result<ExecutionMetrics> {
        let mut tx = begin_mut_tx(db);
        for (table_id, row) in deletes {
            tx.delete_product_value(table_id, &row)?;
        }
        for (table_id, row) in inserts {
            db.insert(&mut tx, table_id, &bsatn::to_vec(&row)?)?;
        }

        let Ok(Ok((_, metrics))) = subs.commit_and_broadcast_event(None, module_event(), tx) else {
            panic!("Encountered an error in `commit_and_broadcast_event`");
        };
        Ok(metrics)
    }

    #[test]
    fn test_subscribe_metrics() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (sender, _) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with index on `id`
        let table_id = db.create_table_for_test("t", &[("id", AlgebraicType::U64)], &[0.into()])?;
        with_auto_commit(&db, |tx| -> anyhow::Result<_> {
            db.insert(tx, table_id, &bsatn::to_vec(&product![1_u64])?)?;
            Ok(())
        })?;

        let auth = AuthCtx::for_testing();
        let sql = "select * from t where id = 1";
        let tx = begin_tx(&db);
        let plan = compile_read_only_query(&auth, &tx, sql)?;
        let plan = Arc::new(plan);

        let (_, metrics) = subs.evaluate_queries(sender, &[plan], &tx, &auth, TableUpdateType::Subscribe)?;

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

    fn check_subscription_err(sql: &str, result: Option<SerializableMessage>) {
        if let Some(SerializableMessage::Subscription(SubscriptionMessage {
            result: SubscriptionResult::Error(SubscriptionError { message, .. }),
            ..
        })) = result
        {
            assert!(
                message.contains(sql),
                "Expected error message to contain the SQL query: {sql}, but got: {message}",
            );
            return;
        }
        panic!("Expected a subscription error message, but got: {:?}", result);
    }

    /// Test that clients receive error messages on subscribe
    #[tokio::test]
    async fn subscribe_single_error() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (tx, mut rx) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        db.create_table_for_test("t", &[("x", AlgebraicType::U8)], &[])?;

        // Subscribe to an invalid query (r is not in scope)
        let sql = "select r.* from t";
        subscribe_single(&subs, sql, tx, &mut 0)?;

        check_subscription_err(sql, rx.recv().await);

        Ok(())
    }

    /// Test that clients receive error messages on subscribe
    #[tokio::test]
    async fn subscribe_multi_error() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (tx, mut rx) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        db.create_table_for_test("t", &[("x", AlgebraicType::U8)], &[])?;

        // Subscribe to an invalid query (r is not in scope)
        let sql = "select r.* from t";
        subscribe_multi(&subs, &[sql], tx, &mut 0)?;

        check_subscription_err(sql, rx.recv().await);

        Ok(())
    }

    /// Test that clients receive error messages on unsubscribe
    #[tokio::test]
    async fn unsubscribe_single_error() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (tx, mut rx) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with an index on `id`
        let table_id = db.create_table_for_test("t", &[("id", AlgebraicType::U8)], &[0.into()])?;
        let index_id = with_read_only(&db, |tx| {
            db.schema_for_table(&*tx, table_id).map(|schema| {
                schema
                    .indexes
                    .first()
                    .map(|index_schema| index_schema.index_id)
                    .unwrap()
            })
        })?;

        let mut query_id = 0;

        // Subscribe to `t`
        let sql = "select * from t where id = 1";
        subscribe_single(&subs, sql, tx.clone(), &mut query_id)?;

        // The initial subscription should succeed
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Subscribe(..),
                ..
            }))
        ));

        // Remove the index from `id`
        with_auto_commit(&db, |tx| db.drop_index(tx, index_id))?;

        // Unsubscribe from `t`
        unsubscribe_single(&subs, tx, query_id)?;

        // Why does the unsubscribe fail?
        // This relies on some knowledge of the underlying implementation.
        // Specifically that we do not recompile queries on unsubscribe.
        // We execute the cached plan which in this case is an index scan.
        // The index no longer exists, and therefore it fails.
        check_subscription_err(sql, rx.recv().await);

        Ok(())
    }

    /// Test that clients receive error messages on unsubscribe
    #[tokio::test]
    async fn unsubscribe_multi_error() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (tx, mut rx) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create a table `t` with an index on `id`
        let table_id = db.create_table_for_test("t", &[("id", AlgebraicType::U8)], &[0.into()])?;
        let index_id = with_read_only(&db, |tx| {
            db.schema_for_table(&*tx, table_id).map(|schema| {
                schema
                    .indexes
                    .first()
                    .map(|index_schema| index_schema.index_id)
                    .unwrap()
            })
        })?;

        commit_tx(&db, &subs, [], [(table_id, product![0_u8])])?;

        let mut query_id = 0;

        // Subscribe to `t`
        let sql = "select * from t where id = 1";
        subscribe_multi(&subs, &[sql], tx.clone(), &mut query_id)?;

        // The initial subscription should succeed
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::SubscribeMulti(..),
                ..
            }))
        ));

        // Remove the index from `id`
        with_auto_commit(&db, |tx| db.drop_index(tx, index_id))?;

        // Unsubscribe from `t`
        unsubscribe_multi(&subs, tx, query_id)?;

        // Why does the unsubscribe fail?
        // This relies on some knowledge of the underlying implementation.
        // Specifically that we do not recompile queries on unsubscribe.
        // We execute the cached plan which in this case is an index scan.
        // The index no longer exists, and therefore it fails.
        check_subscription_err(sql, rx.recv().await);

        Ok(())
    }

    /// Test that clients receieve error messages on tx updates
    #[tokio::test]
    async fn tx_update_error() -> anyhow::Result<()> {
        let client_id = client_id_from_u8(1);
        let (tx, mut rx) = client_connection(client_id);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        // Create two tables `t` and `s` with indexes on their `id` columns
        let t_id = db.create_table_for_test("t", &[("id", AlgebraicType::U8)], &[0.into()])?;
        let s_id = db.create_table_for_test("s", &[("id", AlgebraicType::U8)], &[0.into()])?;
        let index_id = with_read_only(&db, |tx| {
            db.schema_for_table(&*tx, s_id).map(|schema| {
                schema
                    .indexes
                    .first()
                    .map(|index_schema| index_schema.index_id)
                    .unwrap()
            })
        })?;
        let sql = "select t.* from t join s on t.id = s.id";
        subscribe_single(&subs, sql, tx, &mut 0)?;

        // The initial subscription should succeed
        assert!(matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::Subscribe(..),
                ..
            }))
        ));

        // Remove the index from `s`
        with_auto_commit(&db, |tx| db.drop_index(tx, index_id))?;

        // Start a new transaction and insert a new row into `t`
        let mut tx = begin_mut_tx(&db);
        db.insert(&mut tx, t_id, &bsatn::to_vec(&product![2_u8])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        // Why does the update fail?
        // This relies on some knowledge of the underlying implementation.
        // Specifically, plans are cached on the initial subscribe.
        // Hence we execute a cached plan which happens to be an index join.
        // We've removed the index on `s`, and therefore it fails.
        check_subscription_err(sql, rx.recv().await);

        Ok(())
    }

    /// Test that two clients can subscribe to a parameterized query and get the correct rows.
    #[tokio::test]
    async fn test_parameterized_subscription() -> anyhow::Result<()> {
        // Create identities for two different clients
        let id_for_a = identity_from_u8(1);
        let id_for_b = identity_from_u8(2);

        let client_id_for_a = client_id_from_u8(1);
        let client_id_for_b = client_id_from_u8(2);

        // Establish a connection for each client
        let (tx_for_a, mut rx_for_a) = client_connection(client_id_for_a);
        let (tx_for_b, mut rx_for_b) = client_connection(client_id_for_b);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let schema = [("identity", AlgebraicType::identity())];

        let table_id = db.create_table_for_test("t", &schema, &[])?;

        let mut query_ids = 0;

        // Have each client subscribe to the same parameterized query.
        // Each client should receive different rows.
        subscribe_multi(
            &subs,
            &["select * from t where identity = :sender"],
            tx_for_a,
            &mut query_ids,
        )?;
        subscribe_multi(
            &subs,
            &["select * from t where identity = :sender"],
            tx_for_b,
            &mut query_ids,
        )?;

        // Wait for both subscriptions
        assert!(matches!(
            rx_for_a.recv().await,
            Some(SerializableMessage::Subscription(_))
        ));
        assert!(matches!(
            rx_for_b.recv().await,
            Some(SerializableMessage::Subscription(_))
        ));

        // Insert two identities - one for each caller - into the table
        let mut tx = begin_mut_tx(&db);
        db.insert(&mut tx, table_id, &bsatn::to_vec(&product![id_for_a])?)?;
        db.insert(&mut tx, table_id, &bsatn::to_vec(&product![id_for_b])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        let schema = ProductType::from([AlgebraicType::identity()]);

        // Both clients should only receive their identities and not the other's.
        assert_tx_update_for_table(&mut rx_for_a, table_id, &schema, [product![id_for_a]], []).await;
        assert_tx_update_for_table(&mut rx_for_b, table_id, &schema, [product![id_for_b]], []).await;
        Ok(())
    }

    /// Test that two clients can subscribe to a table with RLS rules and get the correct rows
    #[tokio::test]
    async fn test_rls_subscription() -> anyhow::Result<()> {
        // Create identities for two different clients
        let id_for_a = identity_from_u8(1);
        let id_for_b = identity_from_u8(2);

        let client_id_for_a = client_id_from_u8(1);
        let client_id_for_b = client_id_from_u8(2);

        // Establish a connection for each client
        let (tx_for_a, mut rx_for_a) = client_connection(client_id_for_a);
        let (tx_for_b, mut rx_for_b) = client_connection(client_id_for_b);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let schema = [("id", AlgebraicType::identity())];

        let u_id = db.create_table_for_test("u", &schema, &[0.into()])?;
        let v_id = db.create_table_for_test("v", &schema, &[0.into()])?;
        let w_id = db.create_table_for_test("w", &schema, &[0.into()])?;

        insert_rls_rules(
            &db,
            [u_id, v_id, w_id, w_id],
            [
                "select * from u where id = :sender",
                "select * from v where id = :sender",
                "select w.* from u join w on u.id = w.id",
                "select w.* from v join w on v.id = w.id",
            ],
        )?;

        let mut query_ids = 0;

        // Have each client subscribe to `w`.
        // Because `w` is gated using parameterized RLS rules,
        // each client should receive different rows.
        subscribe_multi(&subs, &["select * from w"], tx_for_a, &mut query_ids)?;
        subscribe_multi(&subs, &["select * from w"], tx_for_b, &mut query_ids)?;

        // Wait for both subscriptions
        assert!(matches!(
            rx_for_a.recv().await,
            Some(SerializableMessage::Subscription(_))
        ));
        assert!(matches!(
            rx_for_b.recv().await,
            Some(SerializableMessage::Subscription(_))
        ));

        // Insert a row into `u` for client "a".
        // Insert a row into `v` for client "b".
        // Insert a row into `w` for both.
        let mut tx = begin_mut_tx(&db);
        db.insert(&mut tx, u_id, &bsatn::to_vec(&product![id_for_a])?)?;
        db.insert(&mut tx, v_id, &bsatn::to_vec(&product![id_for_b])?)?;
        db.insert(&mut tx, w_id, &bsatn::to_vec(&product![id_for_a])?)?;
        db.insert(&mut tx, w_id, &bsatn::to_vec(&product![id_for_b])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        let schema = ProductType::from([AlgebraicType::identity()]);

        // Both clients should only receive their identities and not the other's.
        assert_tx_update_for_table(&mut rx_for_a, w_id, &schema, [product![id_for_a]], []).await;
        assert_tx_update_for_table(&mut rx_for_b, w_id, &schema, [product![id_for_b]], []).await;
        Ok(())
    }

    /// Test that we do not send empty updates to clients
    #[tokio::test]
    async fn test_no_empty_updates() -> anyhow::Result<()> {
        // Establish a client connection
        let (tx, mut rx) = client_connection(client_id_from_u8(1));

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let schema = [("x", AlgebraicType::U8)];

        let t_id = db.create_table_for_test("t", &schema, &[])?;

        // Subscribe to rows of `t` where `x` is 0
        subscribe_multi(&subs, &["select * from t where x = 0"], tx, &mut 0)?;

        // Wait to receive the initial subscription message
        assert!(matches!(rx.recv().await, Some(SerializableMessage::Subscription(_))));

        // Insert a row that does not match the query
        let mut tx = begin_mut_tx(&db);
        db.insert(&mut tx, t_id, &bsatn::to_vec(&product![1_u8])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        // Insert a row that does match the query
        let mut tx = begin_mut_tx(&db);
        db.insert(&mut tx, t_id, &bsatn::to_vec(&product![0_u8])?)?;

        assert!(matches!(
            subs.commit_and_broadcast_event(None, module_event(), tx),
            Ok(Ok(_))
        ));

        let schema = ProductType::from([AlgebraicType::U8]);

        // If the server sends empty updates, this assertion will fail,
        // because we will receive one for the first transaction.
        assert_tx_update_for_table(&mut rx, t_id, &schema, [product![0_u8]], []).await;
        Ok(())
    }

    /// Test that we do not compress within a [SubscriptionMessage].
    /// The message itself is compressed before being sent over the wire,
    /// but we don't care about that for this test.
    #[tokio::test]
    async fn test_no_compression_for_subscribe() -> anyhow::Result<()> {
        // Establish a client connection with compression
        let (tx, mut rx) = client_connection_with_compression(client_id_from_u8(1), Compression::Brotli);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let table_id = db.create_table_for_test("t", &[("x", AlgebraicType::U64)], &[])?;

        let mut inserts = vec![];

        for i in 0..16_000u64 {
            inserts.push((table_id, product![i]));
        }

        // Insert a lot of rows into `t`.
        // We want to insert enough to cross any threshold there might be for compression.
        commit_tx(&db, &subs, [], inserts)?;

        // Subscribe to the entire table
        subscribe_multi(&subs, &["select * from t"], tx, &mut 0)?;

        // Assert the table updates within this message are all be uncompressed
        match rx.recv().await {
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result:
                    SubscriptionResult::SubscribeMulti(SubscriptionData {
                        data: FormatSwitch::Bsatn(ws::DatabaseUpdate { tables }),
                    }),
                ..
            })) => {
                assert!(tables.iter().all(|TableUpdate { updates, .. }| updates
                    .iter()
                    .all(|query_update| matches!(query_update, CompressableQueryUpdate::Uncompressed(_)))));
            }
            Some(_) => panic!("unexpected message from subscription"),
            None => panic!("channel unexpectedly closed"),
        };

        Ok(())
    }

    /// Test that we do not compress within a [TransactionUpdateMessage].
    /// The message itself is compressed before being sent over the wire,
    /// but we don't care about that for this test.
    #[tokio::test]
    async fn test_no_compression_for_update() -> anyhow::Result<()> {
        // Establish a client connection with compression
        let (tx, mut rx) = client_connection_with_compression(client_id_from_u8(1), Compression::Brotli);

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let table_id = db.create_table_for_test("t", &[("x", AlgebraicType::U64)], &[])?;

        let mut inserts = vec![];

        for i in 0..16_000u64 {
            inserts.push((table_id, product![i]));
        }

        // Subscribe to the entire table
        subscribe_multi(&subs, &["select * from t"], tx, &mut 0)?;

        // Wait to receive the initial subscription message
        assert!(matches!(rx.recv().await, Some(SerializableMessage::Subscription(_))));

        // Insert a lot of rows into `t`.
        // We want to insert enough to cross any threshold there might be for compression.
        commit_tx(&db, &subs, [], inserts)?;

        // Assert the table updates within this message are all be uncompressed
        match rx.recv().await {
            Some(SerializableMessage::TxUpdate(TransactionUpdateMessage {
                database_update:
                    SubscriptionUpdateMessage {
                        database_update: FormatSwitch::Bsatn(ws::DatabaseUpdate { tables }),
                        ..
                    },
                ..
            })) => {
                assert!(tables.iter().all(|TableUpdate { updates, .. }| updates
                    .iter()
                    .all(|query_update| matches!(query_update, CompressableQueryUpdate::Uncompressed(_)))));
            }
            Some(_) => panic!("unexpected message from subscription"),
            None => panic!("channel unexpectedly closed"),
        };

        Ok(())
    }

    /// In this test we subscribe to a join query, update the lhs table,
    /// and assert that the server sends the correct delta to the client.
    #[tokio::test]
    async fn test_update_for_join() -> anyhow::Result<()> {
        async fn test_subscription_updates(queries: &[&'static str]) -> anyhow::Result<()> {
            // Establish a client connection
            let (sender, mut rx) = client_connection(client_id_from_u8(1));

            let db = relational_db()?;
            let subs = module_subscriptions(db.clone());

            let p_schema = [("id", AlgebraicType::U64), ("signed_in", AlgebraicType::Bool)];
            let l_schema = [
                ("id", AlgebraicType::U64),
                ("x", AlgebraicType::U64),
                ("z", AlgebraicType::U64),
            ];

            let p_id = db.create_table_for_test("p", &p_schema, &[0.into()])?;
            let l_id = db.create_table_for_test("l", &l_schema, &[0.into()])?;

            subscribe_multi(&subs, queries, sender, &mut 0)?;

            assert!(matches!(rx.recv().await, Some(SerializableMessage::Subscription(_))));

            // Insert two matching player rows
            commit_tx(
                &db,
                &subs,
                [],
                [
                    (p_id, product![1_u64, true]),
                    (p_id, product![2_u64, true]),
                    (l_id, product![1_u64, 2_u64, 2_u64]),
                    (l_id, product![2_u64, 3_u64, 3_u64]),
                ],
            )?;

            let schema = ProductType::from(p_schema);

            // We should receive both matching player rows
            assert_tx_update_for_table(
                &mut rx,
                p_id,
                &schema,
                [product![1_u64, true], product![2_u64, true]],
                [],
            )
            .await;

            // Update one of the matching player rows
            commit_tx(
                &db,
                &subs,
                [(p_id, product![2_u64, true])],
                [(p_id, product![2_u64, false])],
            )?;

            // We should receive an update for it because it is still matching
            assert_tx_update_for_table(
                &mut rx,
                p_id,
                &schema,
                [product![2_u64, false]],
                [product![2_u64, true]],
            )
            .await;

            // Update the the same matching player row
            commit_tx(
                &db,
                &subs,
                [(p_id, product![2_u64, false])],
                [(p_id, product![2_u64, true])],
            )?;

            // We should receive an update for it because it is still matching
            assert_tx_update_for_table(
                &mut rx,
                p_id,
                &schema,
                [product![2_u64, true]],
                [product![2_u64, false]],
            )
            .await;

            Ok(())
        }

        test_subscription_updates(&[
            "select * from p where id = 1",
            "select p.* from p join l on p.id = l.id where l.x > 0 and l.x < 5 and l.z > 0 and l.z < 5",
        ])
        .await?;
        test_subscription_updates(&[
            "select * from p where id = 1",
            "select p.* from p join l on p.id = l.id where 0 < l.x and l.x < 5 and 0 < l.z and l.z < 5",
        ])
        .await?;
        test_subscription_updates(&[
            "select * from p where id = 1",
            "select p.* from p join l on p.id = l.id where l.x > 0 and l.x < 5 and l.x > 0 and l.z < 5 and l.id != 1",
        ])
        .await?;
        test_subscription_updates(&[
            "select * from p where id = 1",
            "select p.* from p join l on p.id = l.id where 0 < l.x and l.x < 5 and 0 < l.z and l.z < 5 and l.id != 1",
        ])
        .await?;

        Ok(())
    }

    /// Test that we do not evaluate queries that we know will not match table update rows
    #[tokio::test]
    async fn test_query_pruning() -> anyhow::Result<()> {
        // Establish a connection for each client
        let (tx_for_a, mut rx_for_a) = client_connection(client_id_from_u8(1));
        let (tx_for_b, mut rx_for_b) = client_connection(client_id_from_u8(2));

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let u_id = db.create_table_for_test(
            "u",
            &[
                ("i", AlgebraicType::U64),
                ("a", AlgebraicType::U64),
                ("b", AlgebraicType::U64),
            ],
            &[0.into()],
        )?;
        let v_id = db.create_table_for_test(
            "v",
            &[
                ("i", AlgebraicType::U64),
                ("x", AlgebraicType::U64),
                ("y", AlgebraicType::U64),
            ],
            &[0.into(), 1.into()],
        )?;

        commit_tx(
            &db,
            &subs,
            [],
            [
                (u_id, product![0u64, 1u64, 1u64]),
                (u_id, product![1u64, 2u64, 2u64]),
                (u_id, product![2u64, 3u64, 3u64]),
                (v_id, product![0u64, 4u64, 4u64]),
                (v_id, product![1u64, 5u64, 5u64]),
            ],
        )?;

        let mut query_ids = 0;

        // Returns (i: 0, a: 1, b: 1)
        subscribe_multi(
            &subs,
            &[
                "select u.* from u join v on u.i = v.i where v.x = 4",
                "select u.* from u join v on u.i = v.i where v.x = 6",
            ],
            tx_for_a,
            &mut query_ids,
        )?;

        // Returns (i: 1, a: 2, b: 2)
        subscribe_multi(
            &subs,
            &[
                "select u.* from u join v on u.i = v.i where v.x = 5",
                "select u.* from u join v on u.i = v.i where v.x = 7",
            ],
            tx_for_b,
            &mut query_ids,
        )?;

        // Wait for both subscriptions
        assert!(matches!(
            rx_for_a.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::SubscribeMulti(_),
                ..
            }))
        ));
        assert!(matches!(
            rx_for_b.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::SubscribeMulti(_),
                ..
            }))
        ));

        // Modify a single row in `v`
        let metrics = commit_tx(
            &db,
            &subs,
            [(v_id, product![1u64, 5u64, 5u64])],
            [(v_id, product![1u64, 5u64, 6u64])],
        )?;

        // We should only have evaluated a single query
        assert_eq!(metrics.delta_queries_evaluated, 1);
        assert_eq!(metrics.delta_queries_matched, 0);

        // Insert a new row into `v`
        let metrics = commit_tx(&db, &subs, [], [(v_id, product![2u64, 6u64, 6u64])])?;

        assert_tx_update_for_table(
            &mut rx_for_a,
            u_id,
            &ProductType::from([AlgebraicType::U64, AlgebraicType::U64, AlgebraicType::U64]),
            [product![2u64, 3u64, 3u64]],
            [],
        )
        .await;

        // We should only have evaluated a single query
        assert_eq!(metrics.delta_queries_evaluated, 1);
        assert_eq!(metrics.delta_queries_matched, 1);

        // Modify a matching row in `u`
        let metrics = commit_tx(
            &db,
            &subs,
            [(u_id, product![1u64, 2u64, 2u64])],
            [(u_id, product![1u64, 2u64, 3u64])],
        )?;

        assert_tx_update_for_table(
            &mut rx_for_b,
            u_id,
            &ProductType::from([AlgebraicType::U64, AlgebraicType::U64, AlgebraicType::U64]),
            [product![1u64, 2u64, 3u64]],
            [product![1u64, 2u64, 2u64]],
        )
        .await;

        // We should have evaluated all of the queries
        assert_eq!(metrics.delta_queries_evaluated, 4);
        assert_eq!(metrics.delta_queries_matched, 1);

        // Insert a non-matching row in `u`
        let metrics = commit_tx(&db, &subs, [], [(u_id, product![3u64, 0u64, 0u64])])?;

        // We should have evaluated all of the queries
        assert_eq!(metrics.delta_queries_evaluated, 4);
        assert_eq!(metrics.delta_queries_matched, 0);

        Ok(())
    }

    /// Test that we do not evaluate queries that return trivially empty results
    #[tokio::test]
    async fn test_query_pruning_for_empty_tables() -> anyhow::Result<()> {
        // Establish a client connection
        let (tx, mut rx) = client_connection(client_id_from_u8(1));

        let db = relational_db()?;
        let subs = module_subscriptions(db.clone());

        let schema = &[("id", AlgebraicType::U64), ("a", AlgebraicType::U64)];
        let indices = &[0.into()];
        // Create tables `t` and `s` with `(i: u64, a: u64)`.
        db.create_table_for_test("t", schema, indices)?;
        let s_id = db.create_table_for_test("s", schema, indices)?;

        // Insert one row into `s`, but leave `t` empty.
        commit_tx(&db, &subs, [], [(s_id, product![0u64, 0u64])])?;

        // Subscribe to queries that return empty results
        let metrics = subscribe_multi(
            &subs,
            &[
                "select t.* from t where a = 0",
                "select t.* from t join s on t.id = s.id where s.a = 0",
                "select s.* from t join s on t.id = s.id where t.a = 0",
            ],
            tx,
            &mut 0,
        )?;

        assert_matches!(
            rx.recv().await,
            Some(SerializableMessage::Subscription(SubscriptionMessage {
                result: SubscriptionResult::SubscribeMulti(_),
                ..
            }))
        );

        assert_eq!(metrics.rows_scanned, 0);
        assert_eq!(metrics.index_seeks, 0);

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
        with_auto_commit(&db, |tx| insert(&db, tx, table_id, &product!(1_u8)).map(drop))?;

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
            with_auto_commit(&db2, |tx| insert(&db2, tx, table_id, &product!(2_u8)).map(drop))
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
