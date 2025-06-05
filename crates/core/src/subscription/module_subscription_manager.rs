use super::execution_unit::QueryHash;
use super::tx::DeltaTx;
use crate::client::messages::{
    SerializableMessage, SubscriptionError, SubscriptionMessage, SubscriptionResult, SubscriptionUpdateMessage,
    TransactionUpdateMessage,
};
use crate::client::{ClientConnectionSender, Protocol};
use crate::error::DBError;
use crate::host::module_host::{DatabaseTableUpdate, ModuleEvent, UpdatesRelValue};
use crate::messages::websocket::{self as ws, TableUpdate};
use crate::subscription::delta::eval_delta;
use crate::worker_metrics::WORKER_METRICS;
use hashbrown::hash_map::OccupiedError;
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use parking_lot::RwLock;
use prometheus::IntGauge;
use scopeguard::ScopeGuard;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CompressableQueryUpdate, FormatSwitch, JsonFormat, QueryId, QueryUpdate, SingleQueryUpdate,
    WebsocketFormat,
};
use spacetimedb_data_structures::map::{Entry, IntMap};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::{AlgebraicValue, ConnectionId, Identity, ProductValue};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_subscription::{SubscriptionPlan, TableName};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Clients are uniquely identified by their Identity and ConnectionId.
/// Identity is insufficient because different ConnectionIds can use the same Identity.
/// TODO: Determine if ConnectionId is sufficient for uniquely identifying a client.
type ClientId = (Identity, ConnectionId);
type Query = Arc<Plan>;
type Client = Arc<ClientConnectionSender>;
type SwitchedTableUpdate = FormatSwitch<TableUpdate<BsatnFormat>, TableUpdate<JsonFormat>>;
type SwitchedDbUpdate = FormatSwitch<ws::DatabaseUpdate<BsatnFormat>, ws::DatabaseUpdate<JsonFormat>>;

/// ClientQueryId is an identifier for a query set by the client.
type ClientQueryId = QueryId;
/// SubscriptionId is a globally unique identifier for a subscription.
type SubscriptionId = (ClientId, ClientQueryId);

// Type aliases that oddly aren't included in parking_lot...
type ArcRwLockReadGuard<T> = parking_lot::ArcRwLockReadGuard<parking_lot::RawRwLock, T>;
type ArcRwLockWriteGuard<T> = parking_lot::ArcRwLockWriteGuard<parking_lot::RawRwLock, T>;

#[derive(Debug)]
pub struct Plan {
    hash: QueryHash,
    sql: String,
    plans: Vec<SubscriptionPlan>,
}

impl Plan {
    /// Create a new subscription plan to be cached
    pub fn new(plans: Vec<SubscriptionPlan>, hash: QueryHash, text: String) -> Self {
        Self { plans, hash, sql: text }
    }

    /// Returns the query hash for this subscription
    pub fn hash(&self) -> QueryHash {
        self.hash
    }

    /// A subscription query return rows from a single table.
    /// This method returns the id of that table.
    pub fn subscribed_table_id(&self) -> TableId {
        self.plans[0].subscribed_table_id()
    }

    /// A subscription query return rows from a single table.
    /// This method returns the name of that table.
    pub fn subscribed_table_name(&self) -> &str {
        self.plans[0].subscribed_table_name()
    }

    /// Returns the index ids from which this subscription reads
    pub fn index_ids(&self) -> impl Iterator<Item = (TableId, IndexId)> {
        self.plans
            .iter()
            .flat_map(|plan| plan.index_ids())
            .collect::<HashSet<_>>()
            .into_iter()
    }

    /// Returns the table ids from which this subscription reads
    pub fn table_ids(&self) -> impl Iterator<Item = TableId> + '_ {
        self.plans
            .iter()
            .flat_map(|plan| plan.table_ids())
            .collect::<HashSet<_>>()
            .into_iter()
    }

    /// Returns the plan fragments that comprise this subscription.
    /// Will only return one element unless there is a table with multiple RLS rules.
    pub fn plans_fragments(&self) -> impl Iterator<Item = &SubscriptionPlan> + '_ {
        self.plans.iter()
    }

    /// The `SQL` text of this subscription.
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

/// For each client, we hold a handle for sending messages, and we track the queries they are subscribed to.
#[derive(Debug)]
struct ClientInfo {
    outbound_ref: Client,
    subscriptions: HashMap<SubscriptionId, HashSet<QueryHash>>,
    subscription_ref_count: HashMap<QueryHash, usize>,
    // This should be removed when we migrate to SubscribeSingle.
    legacy_subscriptions: HashSet<QueryHash>,
    // This flag is set if an error occurs during a tx update.
    // It will be cleaned up async or on resubscribe.
    dropped: AtomicBool,
}

impl ClientInfo {
    fn new(outbound_ref: Client) -> Self {
        Self {
            outbound_ref,
            subscriptions: HashMap::default(),
            subscription_ref_count: HashMap::default(),
            legacy_subscriptions: HashSet::default(),
            dropped: AtomicBool::new(false),
        }
    }

    /// Check that the subscription ref count matches the actual number of subscriptions.
    #[cfg(test)]
    fn assert_ref_count_consistency(&self) {
        let mut expected_ref_count = HashMap::new();
        for query_hashes in self.subscriptions.values() {
            for query_hash in query_hashes {
                assert!(
                    self.subscription_ref_count.contains_key(query_hash),
                    "Query hash not found: {:?}",
                    query_hash
                );
                expected_ref_count
                    .entry(*query_hash)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
        }
        assert_eq!(
            self.subscription_ref_count, expected_ref_count,
            "Checking the reference totals failed"
        );
    }
}

/// For each query that has subscribers, we track a set of legacy subscribers and individual subscriptions.
#[derive(Debug)]
struct QueryState {
    query: Query,
    // For legacy clients that subscribe to a set of queries, we track them here.
    legacy_subscribers: HashSet<ClientId>,
    // For clients that subscribe to a single query, we track them here.
    subscriptions: HashSet<ClientId>,
}

impl QueryState {
    fn new(query: Query) -> Self {
        Self {
            query,
            legacy_subscribers: HashSet::default(),
            subscriptions: HashSet::default(),
        }
    }
    fn has_subscribers(&self) -> bool {
        !self.subscriptions.is_empty() || !self.legacy_subscribers.is_empty()
    }

    // This returns all of the clients listening to a query. If a client has multiple subscriptions for this query, it will appear twice.
    fn all_clients(&self) -> impl Iterator<Item = &ClientId> {
        itertools::chain(&self.legacy_subscribers, &self.subscriptions)
    }

    /// Return the [`Query`] for this [`QueryState`]
    pub fn query(&self) -> &Query {
        &self.query
    }

    /// Return the search arguments for this query
    fn search_args(&self) -> impl Iterator<Item = (TableId, ColId, AlgebraicValue)> {
        let mut args = HashSet::new();
        for arg in self
            .query
            .plans
            .iter()
            .flat_map(|subscription| subscription.physical_plan().search_args())
        {
            args.insert(arg);
        }
        args.into_iter()
    }
}

/// In this container, we keep track of parameterized subscription queries.
/// This is used to prune unnecessary queries during subscription evaluation.
///
/// TODO: This container is populated on initial subscription.
/// Ideally this information would be stored in the datastore,
/// but because subscriptions are evaluated using a read only tx,
/// we have to manage this memory separately.
///
/// If we stored this information in the datastore,
/// we could encode pruning logic in the execution plan itself.
#[derive(Debug, Default)]
pub struct SearchArguments {
    /// We parameterize subscriptions if they have an equality selection.
    /// In this case a parameter is a [TableId], [ColId] pair.
    ///
    /// Ex.
    ///
    /// ```sql
    /// SELECT * FROM t WHERE id = <value>
    /// ```
    ///
    /// This query is parameterized by `t.id`.
    ///
    /// Ex.
    ///
    /// ```sql
    /// SELECT t.* FROM t JOIN s ON t.id = s.id WHERE s.x = <value>
    /// ```
    ///
    /// This query is parameterized by `s.x`.
    params: BTreeSet<(TableId, ColId)>,
    /// For each parameter we keep track of its possible values or arguments.
    /// These arguments are the different values that clients subscribe with.
    ///
    /// Ex.
    ///
    /// ```sql
    /// SELECT * FROM t WHERE id = 3
    /// SELECT * FROM t WHERE id = 5
    /// ```
    ///
    /// These queries will get parameterized by `t.id`,
    /// and we will record the args `3` and `5` in this map.
    args: BTreeSet<(TableId, ColId, AlgebraicValue, QueryHash)>,
}

impl SearchArguments {
    /// Return the column ids by which a table is parameterized
    fn search_params_for_table(&self, table_id: TableId) -> impl Iterator<Item = ColId> + '_ {
        let lower_bound = (table_id, 0.into());
        let upper_bound = (table_id, u16::MAX.into());
        self.params
            .range(lower_bound..=upper_bound)
            .map(|(_, col_id)| col_id)
            .cloned()
    }

    /// Are there queries parameterized by this table and column?
    /// If so, do we have a subscriber for this `search_arg`?
    fn queries_for_search_arg(
        &self,
        table_id: TableId,
        col_id: ColId,
        search_arg: AlgebraicValue,
    ) -> impl Iterator<Item = &QueryHash> {
        let lower_bound = (table_id, col_id, search_arg.clone(), QueryHash::MIN);
        let upper_bound = (table_id, col_id, search_arg, QueryHash::MAX);
        self.args.range(lower_bound..upper_bound).map(|(_, _, _, hash)| hash)
    }

    /// Find the queries that need to be evaluated for this row.
    fn queries_for_row<'a>(&'a self, table_id: TableId, row: &'a ProductValue) -> impl Iterator<Item = &'a QueryHash> {
        self.search_params_for_table(table_id)
            .filter_map(|col_id| row.get_field(col_id.idx(), None).ok().map(|arg| (col_id, arg.clone())))
            .flat_map(move |(col_id, arg)| self.queries_for_search_arg(table_id, col_id, arg))
    }

    /// Remove a query hash and its associated data from this container.
    /// Note, a query hash may be associated with multiple column ids.
    fn remove_query(&mut self, query: &QueryHash) {
        // Collect the column parameters for this query
        let mut params = self
            .args
            .iter()
            .filter(|(_, _, _, hash)| hash == query)
            .map(|(table_id, col_id, _, _)| (*table_id, *col_id))
            .dedup()
            .collect::<HashSet<_>>();

        // Remove the search argument entries for this query
        self.args.retain(|(_, _, _, hash)| hash != query);

        // Remove column parameters that no longer have any search arguments associated to them
        params.retain(|(table_id, col_id)| {
            self.args
                .range(
                    (*table_id, *col_id, AlgebraicValue::Min, QueryHash::MIN)
                        ..=(*table_id, *col_id, AlgebraicValue::Max, QueryHash::MAX),
                )
                .next()
                .is_none()
        });

        self.params
            .retain(|(table_id, col_id)| !params.contains(&(*table_id, *col_id)));
    }

    /// Add a new mapping from search argument to query hash
    fn insert_query(&mut self, table_id: TableId, col_id: ColId, arg: AlgebraicValue, query: QueryHash) {
        self.args.insert((table_id, col_id, arg, query));
        self.params.insert((table_id, col_id));
    }
}

type ClientsMap = HashMap<ClientId, ClientInfo>;

/// Keeps track of the indexes that are used in subscriptions.
#[derive(Debug, Default)]
pub struct QueriedTableIndexIds {
    ids: HashMap<TableId, HashMap<IndexId, usize>>,
}

impl FromIterator<(TableId, IndexId)> for QueriedTableIndexIds {
    fn from_iter<T: IntoIterator<Item = (TableId, IndexId)>>(iter: T) -> Self {
        let mut index_ids = Self::default();
        for (table_id, index_id) in iter {
            index_ids.insert_index_id(table_id, index_id);
        }
        index_ids
    }
}

impl QueriedTableIndexIds {
    /// Returns the index ids that are used in subscriptions for this table.
    /// Note, it does not return all of the index ids that are defined on this table.
    /// Only those that are used by at least one subscription query.
    pub fn index_ids_for_table(&self, table_id: TableId) -> impl Iterator<Item = IndexId> + '_ {
        self.ids
            .get(&table_id)
            .into_iter()
            .flat_map(|index_ids| index_ids.keys())
            .copied()
    }

    /// Insert a new `table_id` `index_id` pair into this container.
    /// Note, different queries may read from the same index.
    /// Hence we may already be tracking this index, in which case we bump its ref count.
    pub fn insert_index_id(&mut self, table_id: TableId, index_id: IndexId) {
        *self.ids.entry(table_id).or_default().entry(index_id).or_default() += 1;
    }

    /// Remove a `table_id` `index_id` pair from this container.
    /// Note, different queries may read from the same index.
    /// Hence we only remove this key from the map if its ref count goes to zero.
    pub fn delete_index_id(&mut self, table_id: TableId, index_id: IndexId) {
        if let Some(ids) = self.ids.get_mut(&table_id) {
            if let Some(n) = ids.get_mut(&index_id) {
                *n -= 1;

                if *n == 0 {
                    ids.remove(&index_id);

                    if ids.is_empty() {
                        self.ids.remove(&table_id);
                    }
                }
            }
        }
    }

    /// Insert the index ids from which a query reads into this mapping.
    /// Note, an index may already be tracked if another query is already using it.
    /// In this case we just bump its ref count.
    pub fn insert_index_ids_for_query(&mut self, query: &Query) {
        for (table_id, index_id) in query.index_ids() {
            self.insert_index_id(table_id, index_id);
        }
    }

    /// Delete the index ids from which a query reads from this mapping
    /// Note, we will not remove an index id from this mapping if another query is using it.
    /// Instead we decrement its ref count.
    pub fn delete_index_ids_for_query(&mut self, query: &Query) {
        for (table_id, index_id) in query.index_ids() {
            self.delete_index_id(table_id, index_id);
        }
    }
}

/// Responsible for the efficient evaluation of subscriptions.
/// It performs basic multi-query optimization,
/// in that if a query has N subscribers,
/// it is only executed once,
/// with the results copied to the N receivers.
#[derive(Debug)]
pub struct SubscriptionManager {
    /// State for each client.
    ///
    /// Protected by an `Arc<RwLock>` because the [`Self::send_worker`] needs to read from it
    /// in order to dispatch messages to clients.
    clients: Arc<RwLock<ClientsMap>>,

    /// Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, QueryState>,

    /// If a query reads from a table,
    /// but does not have a simple equality filter on that table,
    /// we map the table to the query in this inverted index.
    tables: IntMap<TableId, HashSet<QueryHash>>,

    /// Tracks the indices used across all subscriptions
    /// to enable building the appropriate indexes for row updates.
    indexes: QueriedTableIndexIds,

    /// If a query reads from a table,
    /// and has a simple equality filter on that table,
    /// we map the filter values to the query in this lookup table.
    search_args: SearchArguments,

    /// Transmit side of a channel to the manager's [`Self::send_worker`] task.
    ///
    /// The send worker runs in parallel and pops [`ComputedQueries`]es out in order,
    /// aggregates each client's full set of updates,
    /// then passes them to the clients' websocket workers.
    /// This allows transaction processing to proceed on the main thread
    /// ahead of post-processing and broadcasting updates
    /// while still ensuring that those updates are sent in the correct serial order.
    /// Additionally, it avoids starving the next reducer request of Tokio workers,
    /// as it imposes a delay between unlocking the datastore
    /// and waking the many per-client sender Tokio tasks.
    send_worker_tx: mpsc::UnboundedSender<ComputedQueries>,

    /// Metric handle for the `subscription_send_queue_length` metric labeled with this database's [`Identity`],
    /// or `None` when not running for a particular database, i.e. in tests.
    send_queue_length_metric: Option<IntGauge>,
}

/// A single update for one client and one query.
struct ClientUpdate {
    id: ClientId,
    table_id: TableId,
    table_name: TableName,
    update: FormatSwitch<SingleQueryUpdate<BsatnFormat>, SingleQueryUpdate<JsonFormat>>,
}

/// The computed incremental update queries with sufficient information
/// to not depend on the transaction lock so that further work can be
/// done in a separate worker: [`SubscriptionManager::send_worker`].
/// The queries in this structure have not been aggregated yet
/// but will be in the worker.
struct ComputedQueries {
    updates: Vec<ClientUpdate>,
    errs: Vec<(ClientId, Box<str>)>,
    event: Arc<ModuleEvent>,
    caller: Option<Arc<ClientConnectionSender>>,
    clients: ArcRwLockReadGuard<ClientsMap>,
}

// Tracks some gauges related to subscriptions.
pub struct SubscriptionGaugeStats {
    // The number of unique queries with at least one subscriber.
    pub num_queries: usize,
    // The number of unique connections with at least one subscription.
    pub num_connections: usize,
    // The number of subscription sets across all clients.
    pub num_subscription_sets: usize,
    // The total number of subscriptions across all clients and queries.
    pub num_query_subscriptions: usize,
    // The total number of subscriptions across all clients and queries.
    pub num_legacy_subscriptions: usize,
}

impl SubscriptionManager {
    pub fn for_database(database_identity: Identity) -> Self {
        let metric = WORKER_METRICS
            .subscription_send_queue_length
            .with_label_values(&database_identity);

        // The `Self::send_worker` will drop this `ScopeGuard` when it exits, thus cleaning up the metric.
        let clean_up_metric = scopeguard::guard((), move |_| {
            let _ = WORKER_METRICS
                .subscription_send_queue_length
                .remove_label_values(&database_identity);
        });

        Self::with_metric(Some(metric), clean_up_metric)
    }

    pub fn for_test_without_metrics_arc_rwlock() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self::for_test_without_metrics()))
    }

    pub fn for_test_without_metrics() -> Self {
        let clean_up_metric = scopeguard::guard((), |_| {});
        Self::with_metric(None, clean_up_metric)
    }

    fn with_metric(
        metric: Option<IntGauge>,
        clean_up_metric: ScopeGuard<(), impl FnOnce(()) + Send + 'static>,
    ) -> Self {
        let (send_worker_tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::send_worker(rx, metric.clone(), clean_up_metric));
        Self {
            clients: Default::default(),
            queries: Default::default(),
            indexes: Default::default(),
            tables: Default::default(),
            search_args: Default::default(),
            send_worker_tx,
            send_queue_length_metric: metric,
        }
    }

    pub fn client(&self, id: &ClientId) -> Client {
        self.clients.read()[id].outbound_ref.clone()
    }

    pub fn query(&self, hash: &QueryHash) -> Option<Query> {
        self.queries.get(hash).map(|state| state.query.clone())
    }

    pub fn calculate_gauge_stats(&self) -> SubscriptionGaugeStats {
        let clients = self.clients.read();
        let num_queries = self.queries.len();
        let num_connections = clients.len();
        let num_query_subscriptions = self.queries.values().map(|state| state.subscriptions.len()).sum();
        let num_subscription_sets = clients.values().map(|ci| ci.subscriptions.len()).sum();
        let num_legacy_subscriptions = clients
            .values()
            .filter(|ci| !ci.legacy_subscriptions.is_empty())
            .count();

        SubscriptionGaugeStats {
            num_queries,
            num_connections,
            num_query_subscriptions,
            num_subscription_sets,
            num_legacy_subscriptions,
        }
    }

    pub fn num_unique_queries(&self) -> usize {
        self.queries.len()
    }

    #[cfg(test)]
    fn contains_query(&self, hash: &QueryHash) -> bool {
        self.queries.contains_key(hash)
    }

    #[cfg(test)]
    fn contains_client(&self, subscriber: &ClientId) -> bool {
        self.clients.read().contains_key(subscriber)
    }

    #[cfg(test)]
    fn contains_legacy_subscription(&self, subscriber: &ClientId, query: &QueryHash) -> bool {
        self.queries
            .get(query)
            .is_some_and(|state| state.legacy_subscribers.contains(subscriber))
    }

    #[cfg(test)]
    fn query_reads_from_table(&self, query: &QueryHash, table: &TableId) -> bool {
        self.tables.get(table).is_some_and(|queries| queries.contains(query))
    }

    #[cfg(test)]
    fn query_has_search_arg(&self, query: QueryHash, table_id: TableId, col_id: ColId, arg: AlgebraicValue) -> bool {
        self.search_args
            .queries_for_search_arg(table_id, col_id, arg)
            .any(|hash| *hash == query)
    }

    #[cfg(test)]
    fn table_has_search_param(&self, table_id: TableId, col_id: ColId) -> bool {
        self.search_args
            .search_params_for_table(table_id)
            .any(|id| id == col_id)
    }

    fn remove_legacy_subscriptions(&mut self, clients: &mut ArcRwLockWriteGuard<ClientsMap>, client: &ClientId) {
        if let Some(ci) = clients.get_mut(client) {
            let mut queries_to_remove = Vec::new();
            for query_hash in ci.legacy_subscriptions.iter() {
                let Some(query_state) = self.queries.get_mut(query_hash) else {
                    tracing::warn!("Query state not found for query hash: {:?}", query_hash);
                    continue;
                };

                query_state.legacy_subscribers.remove(client);
                if !query_state.has_subscribers() {
                    SubscriptionManager::remove_query_from_tables(
                        &mut self.tables,
                        &mut self.indexes,
                        &mut self.search_args,
                        &query_state.query,
                    );
                    queries_to_remove.push(*query_hash);
                }
            }
            ci.legacy_subscriptions.clear();
            for query_hash in queries_to_remove {
                self.queries.remove(&query_hash);
            }
        }
    }

    /// Remove any clients that have been marked for removal
    pub fn remove_dropped_clients(&mut self) {
        let mut clients = self.clients.write_arc();
        for id in clients.keys().copied().collect::<Vec<_>>() {
            if let Some(client) = clients.get(&id) {
                if client.dropped.load(Ordering::Relaxed) {
                    self.remove_all_subscriptions_inner(&mut clients, &id);
                }
            }
        }
    }

    /// Remove a single subscription for a client.
    /// This will return an error if the client does not have a subscription with the given query id.
    pub fn remove_subscription(&mut self, client_id: ClientId, query_id: ClientQueryId) -> Result<Vec<Query>, DBError> {
        let mut clients = self.clients.write();
        let subscription_id = (client_id, query_id);
        let Some(ci) = clients
            .get_mut(&client_id)
            .filter(|ci| !ci.dropped.load(Ordering::Acquire))
        else {
            return Err(anyhow::anyhow!("Client not found: {:?}", client_id).into());
        };

        #[cfg(test)]
        ci.assert_ref_count_consistency();

        let Some(query_hashes) = ci.subscriptions.remove(&subscription_id) else {
            return Err(anyhow::anyhow!("Subscription not found: {:?}", subscription_id).into());
        };
        let mut queries_to_return = Vec::new();
        for hash in query_hashes {
            let remaining_refs = {
                let Some(count) = ci.subscription_ref_count.get_mut(&hash) else {
                    return Err(anyhow::anyhow!("Query count not found for query hash: {:?}", hash).into());
                };
                *count -= 1;
                *count
            };
            if remaining_refs > 0 {
                // The client is still subscribed to this query, so we are done for now.
                continue;
            }
            // The client is no longer subscribed to this query.
            ci.subscription_ref_count.remove(&hash);
            let Some(query_state) = self.queries.get_mut(&hash) else {
                return Err(anyhow::anyhow!("Query state not found for query hash: {:?}", hash).into());
            };
            queries_to_return.push(query_state.query.clone());
            query_state.subscriptions.remove(&client_id);
            if !query_state.has_subscribers() {
                SubscriptionManager::remove_query_from_tables(
                    &mut self.tables,
                    &mut self.indexes,
                    &mut self.search_args,
                    &query_state.query,
                );
                self.queries.remove(&hash);
            }
        }

        #[cfg(test)]
        ci.assert_ref_count_consistency();

        Ok(queries_to_return)
    }

    /// Adds a single subscription for a client.
    pub fn add_subscription(&mut self, client: Client, query: Query, query_id: ClientQueryId) -> Result<(), DBError> {
        self.add_subscription_multi(client, vec![query], query_id).map(|_| ())
    }

    pub fn add_subscription_multi(
        &mut self,
        client: Client,
        queries: Vec<Query>,
        query_id: ClientQueryId,
    ) -> Result<Vec<Query>, DBError> {
        let mut clients = self.clients.write_arc();

        let client_id = (client.id.identity, client.id.connection_id);

        // Clean up any dropped subscriptions
        if clients
            .get(&client_id)
            .is_some_and(|ci| ci.dropped.load(Ordering::Acquire))
        {
            self.remove_all_subscriptions_inner(&mut clients, &client_id);
        }

        let ci = clients
            .entry(client_id)
            .or_insert_with(|| ClientInfo::new(client.clone()));
        #[cfg(test)]
        ci.assert_ref_count_consistency();
        let subscription_id = (client_id, query_id);
        let hash_set = match ci.subscriptions.try_insert(subscription_id, HashSet::new()) {
            Err(OccupiedError { .. }) => {
                return Err(anyhow::anyhow!(
                    "Subscription with id {:?} already exists for client: {:?}",
                    query_id,
                    client_id
                )
                .into());
            }
            Ok(hash_set) => hash_set,
        };
        // We track the queries that are being added for this client.
        let mut new_queries = Vec::new();

        for query in &queries {
            let hash = query.hash();
            // Deduping queries within this single call.
            if !hash_set.insert(hash) {
                continue;
            }
            let query_state = self
                .queries
                .entry(hash)
                .or_insert_with(|| QueryState::new(query.clone()));

            Self::insert_query(&mut self.tables, &mut self.indexes, &mut self.search_args, query_state);

            let entry = ci.subscription_ref_count.entry(hash).or_insert(0);
            *entry += 1;
            let is_new_entry = *entry == 1;

            let inserted = query_state.subscriptions.insert(client_id);
            // This should arguably crash the server, as it indicates a bug.
            if inserted != is_new_entry {
                return Err(anyhow::anyhow!("Internal error, ref count and query_state mismatch").into());
            }
            if inserted {
                new_queries.push(query.clone());
            }
        }

        #[cfg(test)]
        {
            ci.assert_ref_count_consistency();
        }

        Ok(new_queries)
    }

    /// Adds a client and its queries to the subscription manager.
    /// Sets up the set of subscriptions for the client, replacing any existing legacy subscriptions.
    ///
    /// If a query is not already indexed,
    /// its table ids added to the inverted index.
    // #[tracing::instrument(level = "trace", skip_all)]
    pub fn set_legacy_subscription(&mut self, client: Client, queries: impl IntoIterator<Item = Query>) {
        let mut clients = self.clients.write_arc();

        let client_id = (client.id.identity, client.id.connection_id);
        // First, remove any existing legacy subscriptions.
        self.remove_legacy_subscriptions(&mut clients, &client_id);

        // Now, add the new subscriptions.
        let ci = clients
            .entry(client_id)
            .or_insert_with(|| ClientInfo::new(client.clone()));
        for unit in queries {
            let hash = unit.hash();
            ci.legacy_subscriptions.insert(hash);
            let query_state = self
                .queries
                .entry(hash)
                .or_insert_with(|| QueryState::new(unit.clone()));
            Self::insert_query(&mut self.tables, &mut self.indexes, &mut self.search_args, query_state);
            query_state.legacy_subscribers.insert(client_id);
        }
    }

    // Update the mapping from table id to related queries by removing the given query.
    // If this removes all queries for a table, the map entry for that table is removed altogether.
    // This takes a ref to the table map instead of `self` to avoid borrowing issues.
    fn remove_query_from_tables(
        tables: &mut IntMap<TableId, HashSet<QueryHash>>,
        index_ids: &mut QueriedTableIndexIds,
        search_args: &mut SearchArguments,
        query: &Query,
    ) {
        let hash = query.hash();
        search_args.remove_query(&hash);
        index_ids.delete_index_ids_for_query(query);
        for table_id in query.table_ids() {
            if let Entry::Occupied(mut entry) = tables.entry(table_id) {
                let hashes = entry.get_mut();
                if hashes.remove(&hash) && hashes.is_empty() {
                    entry.remove();
                }
            }
        }
    }

    // Update the mapping from table id to related queries by inserting the given query.
    // Also add any search arguments the query may have.
    // This takes a ref to the table map instead of `self` to avoid borrowing issues.
    fn insert_query(
        tables: &mut IntMap<TableId, HashSet<QueryHash>>,
        index_ids: &mut QueriedTableIndexIds,
        search_args: &mut SearchArguments,
        query_state: &QueryState,
    ) {
        // If this is new, we need to update the table to query mapping.
        if !query_state.has_subscribers() {
            index_ids.insert_index_ids_for_query(query_state.query());
            let hash = query_state.query.hash();
            let mut table_ids = query_state.query.table_ids().collect::<HashSet<_>>();
            for (table_id, col_id, arg) in query_state.search_args() {
                table_ids.remove(&table_id);
                search_args.insert_query(table_id, col_id, arg, hash);
            }
            // Update the `tables` map if this query reads from a table,
            // but does not have a search argument for that table.
            for table_id in table_ids {
                tables.entry(table_id).or_default().insert(hash);
            }
        }
    }

    /// Remove all subscriptions for `client` from `clients` and `self`,
    /// when the caller already has a lock on `self.clients`.
    ///
    /// Dirty hack alert! This method takes an `ArcRwLockWriteGuard`
    /// to the `clients` which is inside of `self`.
    /// We take the `Arc` version to avoid borrowck complaining about multiple borrows on `self` coexisting.
    fn remove_all_subscriptions_inner(&mut self, clients: &mut ArcRwLockWriteGuard<ClientsMap>, client: &ClientId) {
        self.remove_legacy_subscriptions(clients, client);
        let Some(client_info) = clients.remove(client) else {
            return;
        };
        debug_assert!(client_info.legacy_subscriptions.is_empty());
        let mut queries_to_remove = Vec::new();
        for query_hash in client_info.subscription_ref_count.keys() {
            let Some(query_state) = self.queries.get_mut(query_hash) else {
                tracing::warn!("Query state not found for query hash: {:?}", query_hash);
                return;
            };
            query_state.subscriptions.remove(client);
            // This could happen twice for the same hash if a client has a duplicate, but that's fine. It is idepotent.
            if !query_state.has_subscribers() {
                queries_to_remove.push(*query_hash);
                SubscriptionManager::remove_query_from_tables(
                    &mut self.tables,
                    &mut self.indexes,
                    &mut self.search_args,
                    &query_state.query,
                );
            }
        }
        for query_hash in queries_to_remove {
            self.queries.remove(&query_hash);
        }
    }

    /// Removes a client from the subscriber mapping.
    /// If a query no longer has any subscribers,
    /// it is removed from the index along with its table ids.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn remove_all_subscriptions(&mut self, client: &ClientId) {
        let mut clients = self.clients.write_arc();
        self.remove_all_subscriptions_inner(&mut clients, client);
    }

    /// Find the queries that need to be evaluated for this table update.
    ///
    /// Note, this tries to prune irrelevant queries from the subscription.
    ///
    /// When is this beneficial?
    ///
    /// If many different clients subscribe to the same parameterized query,
    /// but they all subscribe with different parameter values,
    /// and if these rows contain only a few unique values for this parameter,
    /// most clients will not receive an update,
    /// and so we can avoid evaluating queries for them entirely.
    ///
    /// Ex.
    ///
    /// 1000 clients subscribe to `SELECT * FROM t WHERE id = ?`,
    /// each one with a different value for `?`.
    /// If there are transactions that only ever update one row of `t` at a time,
    /// we only pay the cost of evaluating one query.
    ///
    /// When is this not beneficial?
    ///
    /// If the table update contains a lot of unique values for a parameter,
    /// we won't be able to prune very many queries from the subscription,
    /// so this could add some overhead linear in the size of the table update.
    ///
    /// TODO: This logic should be expressed in the execution plan itself,
    /// so that we don't have to preprocess the table update before execution.
    fn queries_for_table_update<'a>(
        &'a self,
        table_update: &'a DatabaseTableUpdate,
    ) -> impl Iterator<Item = &'a QueryHash> {
        let mut queries = HashSet::new();
        for hash in table_update
            .inserts
            .iter()
            .chain(table_update.deletes.iter())
            .flat_map(|row| self.search_args.queries_for_row(table_update.table_id, row))
        {
            queries.insert(hash);
        }
        for hash in self.tables.get(&table_update.table_id).into_iter().flatten() {
            queries.insert(hash);
        }
        queries.into_iter()
    }

    /// Returns the index ids that are used in subscription queries
    pub fn index_ids_for_subscriptions(&self) -> &QueriedTableIndexIds {
        &self.indexes
    }

    /// This method takes a set of delta tables,
    /// evaluates only the necessary queries for those delta tables,
    /// and then sends the results to each client.
    ///
    /// This previously used rayon to parallelize subscription evaluation.
    /// However, in order to optimize for the common case of small updates,
    /// we removed rayon and switched to a single-threaded execution,
    /// which removed significant overhead associated with thread switching.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn eval_updates_sequential(
        &self,
        tx: &DeltaTx,
        event: Arc<ModuleEvent>,
        caller: Option<Arc<ClientConnectionSender>>,
    ) -> ExecutionMetrics {
        use FormatSwitch::{Bsatn, Json};

        let clients = self.clients.read_arc();

        let tables = &event.status.database_update().unwrap().tables;

        let span = tracing::info_span!("eval_incr").entered();

        #[derive(Default)]
        struct FoldState {
            updates: Vec<ClientUpdate>,
            errs: Vec<(ClientId, Box<str>)>,
            metrics: ExecutionMetrics,
        }

        let FoldState { updates, errs, metrics } = tables
            .iter()
            .filter(|table| !table.inserts.is_empty() || !table.deletes.is_empty())
            .flat_map(|table_update| self.queries_for_table_update(table_update))
            // deduplicate queries by their hash
            .filter({
                let mut seen = HashSet::new();
                // (HashSet::insert returns true for novel elements)
                move |&hash| seen.insert(hash)
            })
            .flat_map(|hash| {
                let qstate = &self.queries[hash];
                qstate
                    .query
                    .plans_fragments()
                    .map(move |plan_fragment| (qstate, plan_fragment))
            })
            // If N clients are subscribed to a query,
            // we copy the DatabaseTableUpdate N times,
            // which involves cloning BSATN (binary) or product values (json).
            .fold(FoldState::default(), |mut acc, (qstate, plan)| {
                let table_id = plan.subscribed_table_id();
                let table_name = plan.subscribed_table_name().clone();
                // Store at most one copy for both the serialization to BSATN and JSON.
                // Each subscriber gets to pick which of these they want,
                // but we only fill `ops_bin_uncompressed` and `ops_json` at most once.
                // The former will be `Some(_)` if some subscriber uses `Protocol::Binary`
                // and the latter `Some(_)` if some subscriber uses `Protocol::Text`.
                //
                // Previously we were compressing each `QueryUpdate` within a `TransactionUpdate`.
                // The reason was simple - many clients can subscribe to the same query.
                // If we compress `TransactionUpdate`s independently for each client,
                // we could be doing a lot of redundant compression.
                //
                // However the risks associated with this approach include:
                //   1. We have to hold the tx lock when compressing
                //   2. A potentially worse compression ratio
                //   3. Extra decompression overhead on the client
                //
                // Because transaction processing is currently single-threaded,
                // the risks of holding the tx lock for longer than necessary,
                // as well as additional the message processing overhead on the client,
                // outweighed the benefit of reduced cpu with the former approach.
                let mut ops_bin_uncompressed: Option<(CompressableQueryUpdate<BsatnFormat>, _, _)> = None;
                let mut ops_json: Option<(QueryUpdate<JsonFormat>, _, _)> = None;

                fn memo_encode<F: WebsocketFormat>(
                    updates: &UpdatesRelValue<'_>,
                    memory: &mut Option<(F::QueryUpdate, u64, usize)>,
                    metrics: &mut ExecutionMetrics,
                ) -> SingleQueryUpdate<F> {
                    let (update, num_rows, num_bytes) = memory
                        .get_or_insert_with(|| {
                            let encoded = updates.encode::<F>();
                            // The first time we insert into this map, we call encode.
                            // This is when we serialize the rows to BSATN/JSON.
                            // Hence this is where we increment `bytes_scanned`.
                            metrics.bytes_scanned += encoded.2;
                            encoded
                        })
                        .clone();
                    // We call this function for each query,
                    // and for each client subscribed to it.
                    // Therefore every time we call this function,
                    // we update the `bytes_sent_to_clients` metric.
                    metrics.bytes_sent_to_clients += num_bytes;
                    SingleQueryUpdate { update, num_rows }
                }

                // filter out clients that've dropped
                let clients_for_query = qstate.all_clients().filter(|id| {
                    clients
                        .get(*id)
                        .is_some_and(|info| !info.dropped.load(Ordering::Acquire))
                });

                match eval_delta(tx, &mut acc.metrics, plan) {
                    Err(err) => {
                        tracing::error!(
                            message = "Query errored during tx update",
                            sql = qstate.query.sql,
                            reason = ?err,
                        );
                        let err = DBError::WithSql {
                            sql: qstate.query.sql.as_str().into(),
                            error: Box::new(err.into()),
                        }
                        .to_string()
                        .into_boxed_str();

                        acc.errs.extend(clients_for_query.map(|id| (*id, err.clone())))
                    }
                    // The query didn't return any rows to update
                    Ok(None) => {}
                    // The query did return updates - process them and add them to the accumulator
                    Ok(Some(delta_updates)) => {
                        let row_iter = clients_for_query.map(|id| {
                            let client = &clients[id].outbound_ref;
                            let update = match client.config.protocol {
                                Protocol::Binary => Bsatn(memo_encode::<BsatnFormat>(
                                    &delta_updates,
                                    &mut ops_bin_uncompressed,
                                    &mut acc.metrics,
                                )),
                                Protocol::Text => Json(memo_encode::<JsonFormat>(
                                    &delta_updates,
                                    &mut ops_json,
                                    &mut acc.metrics,
                                )),
                            };
                            ClientUpdate {
                                id: *id,
                                table_id,
                                table_name: table_name.clone(),
                                update,
                            }
                        });
                        acc.updates.extend(row_iter);
                    }
                }

                acc
            });

        if let Some(metric) = &self.send_queue_length_metric {
            metric.inc();
        }

        // We've now finished all of the work which needs to read from the datastore,
        // so get this work off the main thread and over to the `send_worker`,
        // then return ASAP in order to unlock the datastore and start running the next transaction.
        // See comment on the `send_worker_tx` field in [`SubscriptionManager`] for more motivation.
        self.send_worker_tx
            .send(ComputedQueries {
                updates,
                errs,
                event,
                caller,
                clients,
            })
            .expect("send worker has panicked, or otherwise dropped its recv queue!");

        drop(span);

        metrics
    }

    /// Asynchronous background worker which aggregates each of the clients' updates from a [`ComputedQueries`]
    /// into `DbUpdate`s and then sends them to the clients' WebSocket workers.
    ///
    /// See comment on the `send_worker_tx` field in [`SubscriptionManager`] for motivation.
    ///
    /// If `queue_length_metric` is supplied, it will be decremented each time we pop a [`ComputedQueries`] from `rx`.
    ///
    /// `_clean_up_metric` will be dropped upon exiting this worker,
    /// and should be a [`ScopeGuard`] which does `remove_label_values` on the `queue_length_metric`.
    async fn send_worker(
        mut rx: mpsc::UnboundedReceiver<ComputedQueries>,
        queue_length_metric: Option<IntGauge>,
        _clean_up_metric: ScopeGuard<(), impl FnOnce(()) + Send + 'static>,
    ) {
        while let Some(queries) = rx.recv().await {
            if let Some(metric) = &queue_length_metric {
                metric.dec();
            }

            Self::send_one_computed_queries(queries);
        }
    }

    fn send_one_computed_queries(
        ComputedQueries {
            updates,
            errs,
            event,
            caller,
            clients,
        }: ComputedQueries,
    ) {
        use FormatSwitch::{Bsatn, Json};

        let clients_with_errors = errs.iter().map(|(id, _)| id).collect::<HashSet<_>>();

        let span = tracing::info_span!("eval_incr_group_messages_by_client");

        let mut eval = updates
            .into_iter()
            // Filter out clients whose subscriptions failed
            .filter(|upd| !clients_with_errors.contains(&upd.id))
            // For each subscriber, aggregate all the updates for the same table.
            // That is, we build a map `(subscriber_id, table_id) -> updates`.
            // A particular subscriber uses only one format,
            // so their `TableUpdate` will contain either JSON (`Protocol::Text`)
            // or BSATN (`Protocol::Binary`).
            .fold(
                HashMap::<(ClientId, TableId), SwitchedTableUpdate>::new(),
                |mut tables, upd| {
                    match tables.entry((upd.id, upd.table_id)) {
                        Entry::Occupied(mut entry) => match entry.get_mut().zip_mut(upd.update) {
                            Bsatn((tbl_upd, update)) => tbl_upd.push(update),
                            Json((tbl_upd, update)) => tbl_upd.push(update),
                        },
                        Entry::Vacant(entry) => drop(entry.insert(match upd.update {
                            Bsatn(update) => Bsatn(TableUpdate::new(upd.table_id, (&*upd.table_name).into(), update)),
                            Json(update) => Json(TableUpdate::new(upd.table_id, (&*upd.table_name).into(), update)),
                        })),
                    }
                    tables
                },
            )
            .into_iter()
            // Each client receives a single list of updates per transaction.
            // So before sending the updates to each client,
            // we must stitch together the `TableUpdate*`s into an aggregated list.
            .fold(
                HashMap::<ClientId, SwitchedDbUpdate>::new(),
                |mut updates, ((id, _), update)| {
                    let entry = updates.entry(id);
                    let entry = entry.or_insert_with(|| match &update {
                        Bsatn(_) => Bsatn(<_>::default()),
                        Json(_) => Json(<_>::default()),
                    });
                    match entry.zip_mut(update) {
                        Bsatn((list, elem)) => list.tables.push(elem),
                        Json((list, elem)) => list.tables.push(elem),
                    }
                    updates
                },
            );

        drop(clients_with_errors);
        drop(span);

        let _span = tracing::info_span!("eval_send").entered();

        // We might have a known caller that hasn't been hidden from here..
        // This caller may have subscribed to some query.
        // If they haven't, we'll send them an empty update.
        // Regardless, the update that we send to the caller, if we send any,
        // is a full tx update, rather than a light one.
        // That is, in the case of the caller, we don't respect the light setting.
        if let Some(caller) = caller {
            let caller_id = (caller.id.identity, caller.id.connection_id);
            let database_update = eval
                .remove(&caller_id)
                .map(|update| SubscriptionUpdateMessage::from_event_and_update(&event, update))
                .unwrap_or_else(|| {
                    SubscriptionUpdateMessage::default_for_protocol(caller.config.protocol, event.request_id)
                });
            let message = TransactionUpdateMessage {
                event: Some(event.clone()),
                database_update,
            };
            send_to_client(&caller, message);
        }

        // Send all the other updates.
        for (id, update) in eval {
            let database_update = SubscriptionUpdateMessage::from_event_and_update(&event, update);
            let client = clients[&id].outbound_ref.clone();
            // Conditionally send out a full update or a light one otherwise.
            let event = client.config.tx_update_full.then(|| event.clone());
            let message = TransactionUpdateMessage { event, database_update };
            send_to_client(&client, message);
        }

        // Send error messages and mark clients for removal
        for (id, message) in errs {
            if let Some(client) = clients.get(&id) {
                client.dropped.store(true, Ordering::Release);
                send_to_client(
                    &client.outbound_ref,
                    SubscriptionMessage {
                        request_id: None,
                        query_id: None,
                        timer: None,
                        result: SubscriptionResult::Error(SubscriptionError {
                            table_id: None,
                            message,
                        }),
                    },
                );
            }
        }
    }
}

fn send_to_client(client: &ClientConnectionSender, message: impl Into<SerializableMessage>) {
    if let Err(e) = client.send_message(message) {
        tracing::warn!(%client.id, "failed to send update message to client: {e}")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use spacetimedb_client_api_messages::websocket::QueryId;
    use spacetimedb_lib::AlgebraicValue;
    use spacetimedb_lib::{error::ResultTest, identity::AuthCtx, AlgebraicType, ConnectionId, Identity, Timestamp};
    use spacetimedb_primitives::{ColId, TableId};
    use spacetimedb_sats::product;
    use spacetimedb_subscription::SubscriptionPlan;

    use super::{Plan, SubscriptionManager};
    use crate::db::relational_db::tests_utils::with_read_only;
    use crate::execution_context::Workload;
    use crate::host::module_host::DatabaseTableUpdate;
    use crate::sql::ast::SchemaViewer;
    use crate::subscription::module_subscription_manager::ClientQueryId;
    use crate::{
        client::{ClientActorId, ClientConfig, ClientConnectionSender, ClientName},
        db::relational_db::{tests_utils::TestDB, RelationalDB},
        energy::EnergyQuanta,
        host::{
            module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall},
            ArgsTuple,
        },
        subscription::execution_unit::QueryHash,
    };

    fn create_table(db: &RelationalDB, name: &str) -> ResultTest<TableId> {
        Ok(db.create_table_for_test(name, &[("a", AlgebraicType::U8)], &[])?)
    }

    fn compile_plan(db: &RelationalDB, sql: &str) -> ResultTest<Arc<Plan>> {
        with_read_only(db, |tx| {
            let auth = AuthCtx::for_testing();
            let tx = SchemaViewer::new(&*tx, &auth);
            let (plans, has_param) = SubscriptionPlan::compile(sql, &tx, &auth).unwrap();
            let hash = QueryHash::from_string(sql, auth.caller, has_param);
            Ok(Arc::new(Plan::new(plans, hash, sql.into())))
        })
    }

    fn id(connection_id: u128) -> (Identity, ConnectionId) {
        (Identity::ZERO, ConnectionId::from_u128(connection_id))
    }

    fn client(connection_id: u128) -> ClientConnectionSender {
        let (identity, connection_id) = id(connection_id);
        ClientConnectionSender::dummy(
            ClientActorId {
                identity,
                connection_id,
                name: ClientName(0),
            },
            ClientConfig::for_test(),
        )
    }

    #[test]
    fn test_subscribe_legacy() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.set_legacy_subscription(client.clone(), [plan.clone()]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_legacy_subscription(&id, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_subscribe_single_adds_table_mapping() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), plan.clone(), query_id)?;
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe_from_the_only_subscription() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), plan.clone(), query_id)?;
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        let client_id = (client.id.identity, client.id.connection_id);
        subscriptions.remove_subscription(client_id, query_id)?;
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe_with_unknown_query_id_fails() -> ResultTest<()> {
        let db = TestDB::durable()?;

        create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), plan.clone(), query_id)?;

        let client_id = (client.id.identity, client.id.connection_id);
        assert!(subscriptions.remove_subscription(client_id, QueryId::new(2)).is_err());

        Ok(())
    }

    #[test]
    fn test_subscribe_and_unsubscribe_with_duplicate_queries() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), plan.clone(), query_id)?;
        subscriptions.add_subscription(client.clone(), plan.clone(), QueryId::new(2))?;

        let client_id = (client.id.identity, client.id.connection_id);
        subscriptions.remove_subscription(client_id, query_id)?;

        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    /// A very simple test case of a duplicate query.
    #[test]
    fn test_subscribe_and_unsubscribe_with_duplicate_queries_multi() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        let added_query = subscriptions.add_subscription_multi(client.clone(), vec![plan.clone()], query_id)?;
        assert!(added_query.len() == 1);
        assert_eq!(added_query[0].hash, hash);
        let second_one = subscriptions.add_subscription_multi(client.clone(), vec![plan.clone()], QueryId::new(2))?;
        assert!(second_one.is_empty());

        let client_id = (client.id.identity, client.id.connection_id);
        let removed_queries = subscriptions.remove_subscription(client_id, query_id)?;
        assert!(removed_queries.is_empty());

        assert!(subscriptions.query_reads_from_table(&hash, &table_id));
        let removed_queries = subscriptions.remove_subscription(client_id, QueryId::new(2))?;
        assert!(removed_queries.len() == 1);
        assert_eq!(removed_queries[0].hash, hash);

        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe_doesnt_remove_other_clients() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let clients = (0..3).map(|i| Arc::new(client(i))).collect::<Vec<_>>();

        // All of the clients are using the same query id.
        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(clients[0].clone(), plan.clone(), query_id)?;
        subscriptions.add_subscription(clients[1].clone(), plan.clone(), query_id)?;
        subscriptions.add_subscription(clients[2].clone(), plan.clone(), query_id)?;

        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        let client_ids = clients
            .iter()
            .map(|client| (client.id.identity, client.id.connection_id))
            .collect::<Vec<_>>();
        subscriptions.remove_subscription(client_ids[0], query_id)?;
        // There are still two left.
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));
        subscriptions.remove_subscription(client_ids[1], query_id)?;
        // There is still one left.
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));
        subscriptions.remove_subscription(client_ids[2], query_id)?;
        // Now there are no subscribers.
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe_all_doesnt_remove_other_clients() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let clients = (0..3).map(|i| Arc::new(client(i))).collect::<Vec<_>>();

        // All of the clients are using the same query id.
        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(clients[0].clone(), plan.clone(), query_id)?;
        subscriptions.add_subscription(clients[1].clone(), plan.clone(), query_id)?;
        subscriptions.add_subscription(clients[2].clone(), plan.clone(), query_id)?;

        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        let client_ids = clients
            .iter()
            .map(|client| (client.id.identity, client.id.connection_id))
            .collect::<Vec<_>>();
        subscriptions.remove_all_subscriptions(&client_ids[0]);
        assert!(!subscriptions.contains_client(&client_ids[0]));
        // There are still two left.
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));
        subscriptions.remove_all_subscriptions(&client_ids[1]);
        // There is still one left.
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));
        assert!(!subscriptions.contains_client(&client_ids[1]));
        subscriptions.remove_all_subscriptions(&client_ids[2]);
        // Now there are no subscribers.
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));
        assert!(!subscriptions.contains_client(&client_ids[2]));

        Ok(())
    }

    // This test has a single client with 3 queries of different tables, and tests removing them.
    #[test]
    fn test_multiple_queries() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_names = ["T", "S", "U"];
        let table_ids = table_names
            .iter()
            .map(|name| create_table(&db, name))
            .collect::<ResultTest<Vec<_>>>()?;
        let queries = table_names
            .iter()
            .map(|name| format!("select * from {}", name))
            .map(|sql| compile_plan(&db, &sql))
            .collect::<ResultTest<Vec<_>>>()?;

        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), queries[0].clone(), QueryId::new(1))?;
        subscriptions.add_subscription(client.clone(), queries[1].clone(), QueryId::new(2))?;
        subscriptions.add_subscription(client.clone(), queries[2].clone(), QueryId::new(3))?;
        for i in 0..3 {
            assert!(subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        let client_id = (client.id.identity, client.id.connection_id);
        subscriptions.remove_subscription(client_id, QueryId::new(1))?;
        assert!(!subscriptions.query_reads_from_table(&queries[0].hash(), &table_ids[0]));
        // Assert that the rest are there.
        for i in 1..3 {
            assert!(subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        // Now remove the final two at once.
        subscriptions.remove_all_subscriptions(&client_id);
        for i in 0..3 {
            assert!(!subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        Ok(())
    }

    #[test]
    fn test_multiple_query_sets() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_names = ["T", "S", "U"];
        let table_ids = table_names
            .iter()
            .map(|name| create_table(&db, name))
            .collect::<ResultTest<Vec<_>>>()?;
        let queries = table_names
            .iter()
            .map(|name| format!("select * from {}", name))
            .map(|sql| compile_plan(&db, &sql))
            .collect::<ResultTest<Vec<_>>>()?;

        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        let added = subscriptions.add_subscription_multi(client.clone(), vec![queries[0].clone()], QueryId::new(1))?;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].hash, queries[0].hash());
        let added = subscriptions.add_subscription_multi(client.clone(), vec![queries[1].clone()], QueryId::new(2))?;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].hash, queries[1].hash());
        let added = subscriptions.add_subscription_multi(client.clone(), vec![queries[2].clone()], QueryId::new(3))?;
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].hash, queries[2].hash());
        for i in 0..3 {
            assert!(subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        let client_id = (client.id.identity, client.id.connection_id);
        let removed = subscriptions.remove_subscription(client_id, QueryId::new(1))?;
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].hash, queries[0].hash());
        assert!(!subscriptions.query_reads_from_table(&queries[0].hash(), &table_ids[0]));
        // Assert that the rest are there.
        for i in 1..3 {
            assert!(subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        // Now remove the final two at once.
        subscriptions.remove_all_subscriptions(&client_id);
        for i in 0..3 {
            assert!(!subscriptions.query_reads_from_table(&queries[i].hash(), &table_ids[i]));
        }

        Ok(())
    }

    #[test]
    fn test_internals_for_search_args() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "t")?;

        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();

        // Subscribe to queries that have search arguments
        let queries = (0u8..5)
            .map(|name| format!("select * from t where a = {}", name))
            .map(|sql| compile_plan(&db, &sql))
            .collect::<ResultTest<Vec<_>>>()?;

        for (i, query) in queries.iter().enumerate().take(5) {
            let added =
                subscriptions.add_subscription_multi(client.clone(), vec![query.clone()], QueryId::new(i as u32))?;
            assert_eq!(added.len(), 1);
            assert_eq!(added[0].hash, queries[i].hash);
        }

        // Assert this table has a search parameter
        assert!(subscriptions.table_has_search_param(table_id, ColId(0)));

        for (i, query) in queries.iter().enumerate().take(5) {
            assert!(subscriptions.query_has_search_arg(query.hash, table_id, ColId(0), AlgebraicValue::U8(i as u8)));

            // Only one of `query_reads_from_table` and `query_has_search_arg` can be true at any given time
            assert!(!subscriptions.query_reads_from_table(&queries[i].hash, &table_id));
        }

        // Remove one of the subscriptions
        let query_id = QueryId::new(2);
        let client_id = (client.id.identity, client.id.connection_id);
        let removed = subscriptions.remove_subscription(client_id, query_id)?;
        assert_eq!(removed.len(), 1);

        // We haven't removed the other subscriptions,
        // so this table should still have a search parameter.
        assert!(subscriptions.table_has_search_param(table_id, ColId(0)));

        // We should have removed the search argument for this query
        assert!(!subscriptions.query_reads_from_table(&queries[2].hash, &table_id));
        assert!(!subscriptions.query_has_search_arg(queries[2].hash, table_id, ColId(0), AlgebraicValue::U8(2)));

        for (i, query) in queries.iter().enumerate().take(5) {
            if i != 2 {
                assert!(subscriptions.query_has_search_arg(
                    query.hash,
                    table_id,
                    ColId(0),
                    AlgebraicValue::U8(i as u8)
                ));
            }
        }

        // Remove all of the subscriptions
        subscriptions.remove_all_subscriptions(&client_id);

        // We should no longer record a search parameter for this table
        assert!(!subscriptions.table_has_search_param(table_id, ColId(0)));
        for (i, query) in queries.iter().enumerate().take(5) {
            assert!(!subscriptions.query_has_search_arg(query.hash, table_id, ColId(0), AlgebraicValue::U8(i as u8)));
        }

        Ok(())
    }

    #[test]
    fn test_search_args_for_selects() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "t")?;

        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();

        let queries = (0u8..5)
            .map(|name| format!("select * from t where a = {}", name))
            .chain(std::iter::once(String::from("select * from t")))
            .map(|sql| compile_plan(&db, &sql))
            .collect::<ResultTest<Vec<_>>>()?;

        for (i, query) in queries.iter().enumerate() {
            subscriptions.add_subscription_multi(client.clone(), vec![query.clone()], QueryId::new(i as u32))?;
        }

        let hash_for_2 = queries[2].hash;
        let hash_for_3 = queries[3].hash;
        let hash_for_5 = queries[5].hash;

        // Which queries are relevant for this table update? Only:
        //
        // select * from t where a = 2
        // select * from t where a = 3
        // select * from t
        let table_update = DatabaseTableUpdate {
            table_id,
            table_name: "t".into(),
            inserts: [product![2u8]].into(),
            deletes: [product![3u8]].into(),
        };

        let hashes = subscriptions
            .queries_for_table_update(&table_update)
            .collect::<Vec<_>>();

        assert!(hashes.len() == 3);
        assert!(hashes.contains(&&hash_for_2));
        assert!(hashes.contains(&&hash_for_3));
        assert!(hashes.contains(&&hash_for_5));

        // Which queries are relevant for this table update?
        // Only: select * from t
        let table_update = DatabaseTableUpdate {
            table_id,
            table_name: "t".into(),
            inserts: [product![8u8]].into(),
            deletes: [product![9u8]].into(),
        };

        let hashes = subscriptions
            .queries_for_table_update(&table_update)
            .collect::<Vec<_>>();

        assert!(hashes.len() == 1);
        assert!(hashes.contains(&&hash_for_5));

        Ok(())
    }

    #[test]
    fn test_search_args_for_join() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let schema = [("id", AlgebraicType::U8), ("a", AlgebraicType::U8)];

        let t_id = db.create_table_for_test("t", &schema, &[0.into()])?;
        let s_id = db.create_table_for_test("s", &schema, &[0.into()])?;

        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();

        let plan = compile_plan(&db, "select t.* from t join s on t.id = s.id where s.a = 1")?;
        let hash = plan.hash;

        subscriptions.add_subscription_multi(client.clone(), vec![plan], QueryId::new(0))?;

        // Do we need to evaluate the above join query for this table update?
        // Yes, because the above query does not filter on `t`.
        // Therefore we must evaluate it for any update on `t`.
        let table_update = DatabaseTableUpdate {
            table_id: t_id,
            table_name: "t".into(),
            inserts: [product![0u8, 0u8]].into(),
            deletes: [].into(),
        };

        let hashes = subscriptions
            .queries_for_table_update(&table_update)
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(hashes, vec![hash]);

        // Do we need to evaluate the above join query for this table update?
        // Yes, because `s.a = 1`.
        let table_update = DatabaseTableUpdate {
            table_id: s_id,
            table_name: "s".into(),
            inserts: [product![0u8, 1u8]].into(),
            deletes: [].into(),
        };

        let hashes = subscriptions
            .queries_for_table_update(&table_update)
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(hashes, vec![hash]);

        // Do we need to evaluate the above join query for this table update?
        // No, because `s.a != 1`.
        let table_update = DatabaseTableUpdate {
            table_id: s_id,
            table_name: "s".into(),
            inserts: [product![0u8, 2u8]].into(),
            deletes: [].into(),
        };

        let hashes = subscriptions
            .queries_for_table_update(&table_update)
            .cloned()
            .collect::<Vec<_>>();

        assert!(hashes.is_empty());

        Ok(())
    }

    #[test]
    fn test_subscribe_fails_with_duplicate_request_id() -> ResultTest<()> {
        let db = TestDB::durable()?;

        create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.add_subscription(client.clone(), plan.clone(), query_id)?;

        assert!(subscriptions
            .add_subscription(client.clone(), plan.clone(), query_id)
            .is_err());

        Ok(())
    }

    #[test]
    fn test_subscribe_multi_fails_with_duplicate_request_id() -> ResultTest<()> {
        let db = TestDB::durable()?;

        create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        let result = subscriptions.add_subscription_multi(client.clone(), vec![plan.clone()], query_id)?;
        assert_eq!(result[0].hash, plan.hash);

        assert!(subscriptions
            .add_subscription_multi(client.clone(), vec![plan.clone()], query_id)
            .is_err());

        Ok(())
    }

    #[test]
    fn test_unsubscribe() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.set_legacy_subscription(client, [plan]);
        subscriptions.remove_all_subscriptions(&id);

        assert!(!subscriptions.contains_query(&hash));
        assert!(!subscriptions.contains_legacy_subscription(&id, &hash));
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_subscribe_idempotent() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.set_legacy_subscription(client.clone(), [plan.clone()]);
        subscriptions.set_legacy_subscription(client.clone(), [plan.clone()]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_legacy_subscription(&id, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        subscriptions.remove_all_subscriptions(&id);

        assert!(!subscriptions.contains_query(&hash));
        assert!(!subscriptions.contains_legacy_subscription(&id, &hash));
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_share_queries_full() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id0 = id(0);
        let client0 = Arc::new(client(0));

        let id1 = id(1);
        let client1 = Arc::new(client(1));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();

        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.set_legacy_subscription(client0, [plan.clone()]);
        subscriptions.set_legacy_subscription(client1, [plan.clone()]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_legacy_subscription(&id0, &hash));
        assert!(subscriptions.contains_legacy_subscription(&id1, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        subscriptions.remove_all_subscriptions(&id0);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_legacy_subscription(&id1, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        assert!(!subscriptions.contains_legacy_subscription(&id0, &hash));

        Ok(())
    }

    #[test]
    fn test_share_queries_partial() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let t = create_table(&db, "T")?;
        let s = create_table(&db, "S")?;

        let scan = "select * from T";
        let select0 = "select * from T where a = 0";
        let select1 = "select * from S where a = 1";

        let plan_scan = compile_plan(&db, scan)?;
        let plan_select0 = compile_plan(&db, select0)?;
        let plan_select1 = compile_plan(&db, select1)?;

        let hash_scan = plan_scan.hash();
        let hash_select0 = plan_select0.hash();
        let hash_select1 = plan_select1.hash();

        let id0 = id(0);
        let client0 = Arc::new(client(0));

        let id1 = id(1);
        let client1 = Arc::new(client(1));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();
        let mut subscriptions = SubscriptionManager::for_test_without_metrics();
        subscriptions.set_legacy_subscription(client0, [plan_scan.clone(), plan_select0.clone()]);
        subscriptions.set_legacy_subscription(client1, [plan_scan.clone(), plan_select1.clone()]);

        assert!(subscriptions.contains_query(&hash_scan));
        assert!(subscriptions.contains_query(&hash_select0));
        assert!(subscriptions.contains_query(&hash_select1));

        assert!(subscriptions.contains_legacy_subscription(&id0, &hash_scan));
        assert!(subscriptions.contains_legacy_subscription(&id0, &hash_select0));

        assert!(subscriptions.contains_legacy_subscription(&id1, &hash_scan));
        assert!(subscriptions.contains_legacy_subscription(&id1, &hash_select1));

        assert!(subscriptions.query_reads_from_table(&hash_scan, &t));
        assert!(subscriptions.query_has_search_arg(hash_select0, t, ColId(0), AlgebraicValue::U8(0)));
        assert!(subscriptions.query_has_search_arg(hash_select1, s, ColId(0), AlgebraicValue::U8(1)));

        assert!(!subscriptions.query_reads_from_table(&hash_scan, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select0, &t));
        assert!(!subscriptions.query_reads_from_table(&hash_select1, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select0, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select1, &t));

        subscriptions.remove_all_subscriptions(&id0);

        assert!(subscriptions.contains_query(&hash_scan));
        assert!(subscriptions.contains_query(&hash_select1));
        assert!(!subscriptions.contains_query(&hash_select0));

        assert!(subscriptions.contains_legacy_subscription(&id1, &hash_scan));
        assert!(subscriptions.contains_legacy_subscription(&id1, &hash_select1));

        assert!(!subscriptions.contains_legacy_subscription(&id0, &hash_scan));
        assert!(!subscriptions.contains_legacy_subscription(&id0, &hash_select0));

        assert!(subscriptions.query_reads_from_table(&hash_scan, &t));
        assert!(subscriptions.query_has_search_arg(hash_select1, s, ColId(0), AlgebraicValue::U8(1)));

        assert!(!subscriptions.query_reads_from_table(&hash_select1, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_scan, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select1, &t));

        Ok(())
    }

    #[test]
    fn test_caller_transaction_update_without_subscription() -> ResultTest<()> {
        // test if a transaction update is sent to the reducer caller even if
        // the caller haven't subscribed to any updates
        let db = TestDB::durable()?;

        let id0 = Identity::ZERO;
        let client0 = ClientActorId::for_test(id0);
        let config = ClientConfig::for_test();
        let (client0, mut rx) = ClientConnectionSender::dummy_with_channel(client0, config);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _rt = runtime.enter();
        let subscriptions = SubscriptionManager::for_test_without_metrics();

        let event = Arc::new(ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: id0,
            caller_connection_id: Some(client0.id.connection_id),
            function_call: ModuleFunctionCall {
                reducer: "DummyReducer".into(),
                reducer_id: u32::MAX.into(),
                args: ArgsTuple::nullary(),
            },
            status: EventStatus::Committed(DatabaseUpdate::default()),
            energy_quanta_used: EnergyQuanta::ZERO,
            host_execution_duration: Duration::default(),
            request_id: None,
            timer: None,
        });

        db.with_read_only(Workload::Update, |tx| {
            subscriptions.eval_updates_sequential(&(&*tx).into(), event, Some(Arc::new(client0)))
        });

        runtime.block_on(async move {
            tokio::time::timeout(Duration::from_millis(20), async move {
                rx.recv().await.expect("Expected at least one message");
            })
            .await
            .expect("Timed out waiting for a message to the client");
        });

        Ok(())
    }
}
