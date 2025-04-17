use super::execution_unit::QueryHash;
use super::tx::DeltaTx;
use crate::client::messages::{
    SerializableMessage, SubscriptionError, SubscriptionMessage, SubscriptionResult, SubscriptionUpdateMessage,
    TransactionUpdateMessage,
};
use crate::client::{ClientConnectionSender, Protocol};
use crate::error::DBError;
use crate::execution_context::WorkloadType;
use crate::host::module_host::{DatabaseTableUpdate, ModuleEvent, UpdatesRelValue};
use crate::messages::websocket::{self as ws, TableUpdate};
use crate::subscription::delta::eval_delta;
use crate::subscription::record_exec_metrics;
use hashbrown::hash_map::OccupiedError;
use hashbrown::{HashMap, HashSet};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CompressableQueryUpdate, Compression, FormatSwitch, JsonFormat, QueryId, QueryUpdate, WebsocketFormat,
};
use spacetimedb_data_structures::map::{Entry, IntMap};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_primitives::TableId;
use spacetimedb_subscription::SubscriptionPlan;
use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
}

/// Responsible for the efficient evaluation of subscriptions.
/// It performs basic multi-query optimization,
/// in that if a query has N subscribers,
/// it is only executed once,
/// with the results copied to the N receivers.
#[derive(Debug, Default)]
pub struct SubscriptionManager {
    // State for each client.
    clients: HashMap<ClientId, ClientInfo>,

    // Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, QueryState>,

    // Inverted index from tables to queries that read from them.
    tables: IntMap<TableId, HashSet<QueryHash>>,
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
    pub fn client(&self, id: &ClientId) -> Client {
        self.clients[id].outbound_ref.clone()
    }

    pub fn query(&self, hash: &QueryHash) -> Option<Query> {
        self.queries.get(hash).map(|state| state.query.clone())
    }

    pub fn calculate_gauge_stats(&self) -> SubscriptionGaugeStats {
        let num_queries = self.queries.len();
        let num_connections = self.clients.len();
        let num_query_subscriptions = self.queries.values().map(|state| state.subscriptions.len()).sum();
        let num_subscription_sets = self.clients.values().map(|ci| ci.subscriptions.len()).sum();
        let num_legacy_subscriptions = self
            .clients
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
        self.clients.contains_key(subscriber)
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

    fn remove_legacy_subscriptions(&mut self, client: &ClientId) {
        if let Some(ci) = self.clients.get_mut(client) {
            let mut queries_to_remove = Vec::new();
            for query_hash in ci.legacy_subscriptions.iter() {
                let Some(query_state) = self.queries.get_mut(query_hash) else {
                    tracing::warn!("Query state not found for query hash: {:?}", query_hash);
                    continue;
                };

                query_state.legacy_subscribers.remove(client);
                if !query_state.has_subscribers() {
                    SubscriptionManager::remove_query_from_tables(&mut self.tables, &query_state.query);
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
        for id in self.clients.keys().copied().collect::<Vec<_>>() {
            if let Some(client) = self.clients.get(&id) {
                if client.dropped.load(Ordering::Relaxed) {
                    self.remove_all_subscriptions(&id);
                }
            }
        }
    }

    /// Remove a single subscription for a client.
    /// This will return an error if the client does not have a subscription with the given query id.
    pub fn remove_subscription(&mut self, client_id: ClientId, query_id: ClientQueryId) -> Result<Vec<Query>, DBError> {
        let subscription_id = (client_id, query_id);
        let Some(ci) = self
            .clients
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
                SubscriptionManager::remove_query_from_tables(&mut self.tables, &query_state.query);
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
        let client_id = (client.id.identity, client.id.connection_id);

        // Clean up any dropped subscriptions
        if self
            .clients
            .get(&client_id)
            .is_some_and(|ci| ci.dropped.load(Ordering::Acquire))
        {
            self.remove_all_subscriptions(&client_id);
        }

        let ci = self
            .clients
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

            // If this is new, we need to update the table to query mapping.
            if !query_state.has_subscribers() {
                for table_id in query.table_ids() {
                    self.tables.entry(table_id).or_default().insert(hash);
                }
            }
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
        let client_id = (client.id.identity, client.id.connection_id);
        // First, remove any existing legacy subscriptions.
        self.remove_legacy_subscriptions(&client_id);

        // Now, add the new subscriptions.
        let ci = self
            .clients
            .entry(client_id)
            .or_insert_with(|| ClientInfo::new(client.clone()));
        for unit in queries {
            let hash = unit.hash();
            ci.legacy_subscriptions.insert(hash);
            let query_state = self
                .queries
                .entry(hash)
                .or_insert_with(|| QueryState::new(unit.clone()));
            for table_id in unit.table_ids() {
                self.tables.entry(table_id).or_default().insert(hash);
            }
            query_state.legacy_subscribers.insert(client_id);
        }
    }

    // Update the mapping from table id to related queries by removing the given query.
    // If this removes all queries for a table, the map entry for that table is removed altogether.
    // This takes a ref to the table map instead of `self` to avoid borrowing issues.
    fn remove_query_from_tables(tables: &mut IntMap<TableId, HashSet<QueryHash>>, query: &Query) {
        let hash = query.hash();
        for table_id in query.table_ids() {
            if let Entry::Occupied(mut entry) = tables.entry(table_id) {
                let hashes = entry.get_mut();
                if hashes.remove(&hash) && hashes.is_empty() {
                    entry.remove();
                }
            }
        }
    }

    /// Removes a client from the subscriber mapping.
    /// If a query no longer has any subscribers,
    /// it is removed from the index along with its table ids.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn remove_all_subscriptions(&mut self, client: &ClientId) {
        self.remove_legacy_subscriptions(client);
        let Some(client_info) = self.clients.remove(client) else {
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
                SubscriptionManager::remove_query_from_tables(&mut self.tables, &query_state.query);
            }
        }
        for query_hash in queries_to_remove {
            self.queries.remove(&query_hash);
        }
    }

    /// This method takes a set of delta tables,
    /// evaluates only the necessary queries for those delta tables,
    /// and then sends the results to each client.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn eval_updates(
        &self,
        tx: &DeltaTx,
        event: Arc<ModuleEvent>,
        caller: Option<&ClientConnectionSender>,
        database_identity: &Identity,
    ) {
        use FormatSwitch::{Bsatn, Json};

        let tables = &event.status.database_update().unwrap().tables;

        // Put the main work on a rayon compute thread.
        rayon::scope(|_| {
            let span = tracing::info_span!("eval_incr").entered();

            type ClientQueryUpdate<F> = (<F as WebsocketFormat>::QueryUpdate, /* num_rows */ u64);
            struct ClientUpdate<'a> {
                id: &'a ClientId,
                table_id: TableId,
                table_name: &'a str,
                update: FormatSwitch<ClientQueryUpdate<BsatnFormat>, ClientQueryUpdate<JsonFormat>>,
            }

            // rayon has a fold-reduce idiom, which we use here. First, rayon splits
            // the task onto a number of worker threads, and then on each thread, we *fold*
            // each item of the iterator into an accumulator. This is that accumulator
            // state; we use vecs because we're only ever going to be appending onto the
            // end, so reallocation is more or less amortized.
            #[derive(Default)]
            struct FoldState<'a> {
                updates: Vec<ClientUpdate<'a>>,
                errs: Vec<(&'a ClientId, Box<str>)>,
                metrics: ExecutionMetrics,
            }

            // Next, we *reduce* the result of multiple threads into one final output.
            // This is the accumulator for that; we use `VecList`s here because they
            // have good characteristics for this use case, namely cheap appension
            // of the result of each thread.
            #[derive(Default)]
            struct ReduceState<'a> {
                updates: VecList<ClientUpdate<'a>>,
                errs: VecList<(&'a ClientId, Box<str>)>,
                metrics: ExecutionMetrics,
            }
            impl<'a> ReduceState<'a> {
                /// Convert the result of a single-thread fold to get ready for a multi-thread reduce.
                fn from_fold(acc: FoldState<'a>) -> Self {
                    Self {
                        updates: acc.updates.into(),
                        errs: acc.errs.into(),
                        metrics: acc.metrics,
                    }
                }
                /// Concatenate this `ReduceState` with another one.
                ///
                /// This is a cheap operation, since `LinkedList::append` is `O(1)`.
                fn append(mut self, rhs: Self) -> Self {
                    self.updates.append(rhs.updates);
                    self.errs.append(rhs.errs);
                    self.metrics.merge(rhs.metrics);
                    self
                }
            }

            let plans = tables
                .iter()
                .filter(|table| !table.inserts.is_empty() || !table.deletes.is_empty())
                .filter_map(|DatabaseTableUpdate { table_id, .. }| self.tables.get(table_id))
                .flatten()
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
                // collect all plan fragments we want to do work on into a
                // single vec, which is more efficient for rayon to work with.
                .collect::<Vec<_>>();

            let ReduceState { updates, errs, metrics } = plans
                .into_par_iter()
                // If N clients are subscribed to a query,
                // we copy the DatabaseTableUpdate N times,
                // which involves cloning BSATN (binary) or product values (json).
                .fold(FoldState::default, |mut acc, (qstate, plan)| {
                    let table_id = plan.subscribed_table_id();
                    let table_name = plan.subscribed_table_name();
                    // Store at most one copy of the serialization to BSATN x Compression
                    // and ditto for the "serialization" for JSON.
                    // Each subscriber gets to pick which of these they want,
                    // but we only fill `ops_bin_{compression}` and `ops_json` at most once.
                    // The former will be `Some(_)` if some subscriber uses `Protocol::Binary`
                    // and the latter `Some(_)` if some subscriber uses `Protocol::Text`.
                    let mut ops_bin_brotli: Option<(CompressableQueryUpdate<BsatnFormat>, _, _)> = None;
                    let mut ops_bin_gzip: Option<(CompressableQueryUpdate<BsatnFormat>, _, _)> = None;
                    let mut ops_bin_none: Option<(CompressableQueryUpdate<BsatnFormat>, _, _)> = None;
                    let mut ops_json: Option<(QueryUpdate<JsonFormat>, _, _)> = None;

                    fn memo_encode<F: WebsocketFormat>(
                        updates: &UpdatesRelValue<'_>,
                        client: &ClientConnectionSender,
                        memory: &mut Option<(F::QueryUpdate, u64, usize)>,
                        metrics: &mut ExecutionMetrics,
                    ) -> (F::QueryUpdate, u64) {
                        let (update, num_rows, num_bytes) = memory
                            .get_or_insert_with(|| {
                                let encoded = updates.encode::<F>(client.config.compression);
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
                        (update, num_rows)
                    }

                    // filter out clients that've dropped
                    let clients_for_query = qstate.all_clients().filter(|id| {
                        self.clients
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
                            acc.errs
                                .extend(clients_for_query.map(|id| (id, err.to_string().into_boxed_str())))
                        }
                        // The query didn't return any rows to update
                        Ok(None) => {}
                        // The query did return updates - process them and add them to the accumulator
                        Ok(Some(delta_updates)) => {
                            let row_iter = clients_for_query.map(|id| {
                                let client = &self.clients[id].outbound_ref;
                                let update = match client.config.protocol {
                                    Protocol::Binary => Bsatn(memo_encode::<BsatnFormat>(
                                        &delta_updates,
                                        client,
                                        match client.config.compression {
                                            Compression::Brotli => &mut ops_bin_brotli,
                                            Compression::Gzip => &mut ops_bin_gzip,
                                            Compression::None => &mut ops_bin_none,
                                        },
                                        &mut acc.metrics,
                                    )),
                                    Protocol::Text => Json(memo_encode::<JsonFormat>(
                                        &delta_updates,
                                        client,
                                        &mut ops_json,
                                        &mut acc.metrics,
                                    )),
                                };
                                ClientUpdate {
                                    id,
                                    table_id,
                                    table_name,
                                    update,
                                }
                            });
                            acc.updates.extend(row_iter);
                        }
                    }

                    acc
                })
                // it would be nice to use `.collect_into_vec()` here, and reap the
                // benefits of having an `IndexedParallelIterator`, but we actually
                // produce many elements per `SubscriptionPlan` and would need to
                // `flatten` them, meaning it effectively becomes unindexed.
                .map(ReduceState::from_fold)
                .reduce(ReduceState::default, ReduceState::append);

            record_exec_metrics(&WorkloadType::Update, database_identity, metrics);

            let clients_with_errors = errs.iter().map(|(id, _)| *id).collect::<HashSet<_>>();

            let mut eval = updates
                .into_iter()
                // Filter out clients whose subscriptions failed
                .filter(|upd| !clients_with_errors.contains(upd.id))
                // For each subscriber, aggregate all the updates for the same table.
                // That is, we build a map `(subscriber_id, table_id) -> updates`.
                // A particular subscriber uses only one format,
                // so their `TableUpdate` will contain either JSON (`Protocol::Text`)
                // or BSATN (`Protocol::Binary`).
                .fold(
                    HashMap::<(&ClientId, TableId), SwitchedTableUpdate>::new(),
                    |mut tables, upd| {
                        match tables.entry((upd.id, upd.table_id)) {
                            Entry::Occupied(mut entry) => match entry.get_mut().zip_mut(upd.update) {
                                Bsatn((tbl_upd, update)) => tbl_upd.push(update),
                                Json((tbl_upd, update)) => tbl_upd.push(update),
                            },
                            Entry::Vacant(entry) => drop(entry.insert(match upd.update {
                                Bsatn(update) => Bsatn(TableUpdate::new(upd.table_id, upd.table_name.into(), update)),
                                Json(update) => Json(TableUpdate::new(upd.table_id, upd.table_name.into(), update)),
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
                    HashMap::<&ClientId, SwitchedDbUpdate>::new(),
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
            if let Some((caller, conn_id)) = caller.zip(event.caller_connection_id) {
                let database_update = eval
                    .remove(&(event.caller_identity, conn_id))
                    .map(|update| SubscriptionUpdateMessage::from_event_and_update(&event, update))
                    .unwrap_or_else(|| {
                        SubscriptionUpdateMessage::default_for_protocol(caller.config.protocol, event.request_id)
                    });
                let message = TransactionUpdateMessage {
                    event: Some(event.clone()),
                    database_update,
                };
                send_to_client(caller, message);
            }

            // Send all the other updates.
            for (id, update) in eval {
                let database_update = SubscriptionUpdateMessage::from_event_and_update(&event, update);
                let client = self.client(id);
                // Conditionally send out a full update or a light one otherwise.
                let event = client.config.tx_update_full.then(|| event.clone());
                let message = TransactionUpdateMessage { event, database_update };
                send_to_client(&client, message);
            }

            // Send error messages and mark clients for removal
            for (id, message) in errs {
                if let Some(client) = self.clients.get(id) {
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
        })
    }
}

fn send_to_client(client: &ClientConnectionSender, message: impl Into<SerializableMessage>) {
    if let Err(e) = client.send_message(message) {
        tracing::warn!(%client.id, "failed to send update message to client: {e}")
    }
}

/// A linked list of vecs.
///
/// To quote the docs for [`ParallelIterator::collect_vec_list`] (which I (Noa) also wrote):
///
/// > This is useful when you need to condense a parallel iterator into a
/// > collection, but have no specific requirements for what that collection
/// > should be. [...] This is a very efficient way to collect an unindexed
/// > parallel iterator, without much intermediate data movement.
struct VecList<T>(LinkedList<Vec<T>>);

impl<T> Default for VecList<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<T> From<Vec<T>> for VecList<T> {
    fn from(vec: Vec<T>) -> Self {
        let mut list = LinkedList::new();
        if !vec.is_empty() {
            list.push_back(vec);
        }
        Self(list)
    }
}
impl<T> VecList<T> {
    /// Append another `VecList` onto this one.
    ///
    /// This operation is `O(1)`.
    fn append(&mut self, mut other: Self) {
        self.0.append(&mut other.0)
    }
    /// Iterate over the individual elements of this `VecList`.
    fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter().flatten()
    }
}
impl<T> IntoIterator for VecList<T> {
    type Item = T;
    type IntoIter = std::iter::Flatten<std::collections::linked_list::IntoIter<Vec<T>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use spacetimedb_client_api_messages::websocket::QueryId;
    use spacetimedb_lib::{error::ResultTest, identity::AuthCtx, AlgebraicType, ConnectionId, Identity, Timestamp};
    use spacetimedb_primitives::TableId;
    use spacetimedb_subscription::SubscriptionPlan;

    use super::{Plan, SubscriptionManager};
    use crate::execution_context::Workload;
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
        db.with_read_only(Workload::ForTests, |tx| {
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

        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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
    fn test_subscribe_fails_with_duplicate_request_id() -> ResultTest<()> {
        let db = TestDB::durable()?;

        create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;

        let client = Arc::new(client(0));

        let query_id: ClientQueryId = QueryId::new(1);
        let mut subscriptions = SubscriptionManager::default();
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
        let mut subscriptions = SubscriptionManager::default();
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

        let mut subscriptions = SubscriptionManager::default();
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

        let mut subscriptions = SubscriptionManager::default();
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

        let mut subscriptions = SubscriptionManager::default();
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

        let mut subscriptions = SubscriptionManager::default();
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
        assert!(subscriptions.query_reads_from_table(&hash_select0, &t));
        assert!(subscriptions.query_reads_from_table(&hash_select1, &s));

        assert!(!subscriptions.query_reads_from_table(&hash_scan, &s));
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
        assert!(subscriptions.query_reads_from_table(&hash_select1, &s));

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

        let subscriptions = SubscriptionManager::default();

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
            subscriptions.eval_updates(&(&*tx).into(), event, Some(&client0), &db.database_identity())
        });

        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async move {
                tokio::time::timeout(Duration::from_millis(20), async move {
                    rx.recv().await.expect("Expected at least one message");
                })
                .await
                .expect("Timed out waiting for a message to the client");
            });

        Ok(())
    }
}
