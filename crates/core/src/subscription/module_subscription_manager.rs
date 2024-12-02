use super::execution_unit::{ExecutionUnit, QueryHash};
use crate::client::messages::{SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientConnectionSender, Protocol};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::DBError;
use crate::host::module_host::{DatabaseTableUpdate, ModuleEvent, UpdatesRelValue};
use crate::messages::websocket::{self as ws, TableUpdate};
use arrayvec::ArrayVec;
use hashbrown::hash_map::OccupiedError;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CompressableQueryUpdate, FormatSwitch, JsonFormat, QueryUpdate, WebsocketFormat,
};
use spacetimedb_data_structures::map::{Entry, HashCollectionExt, HashMap, HashSet, IntMap};
use spacetimedb_lib::{Address, Identity};
use spacetimedb_primitives::TableId;
use std::sync::Arc;
use std::time::Duration;

/// Clients are uniquely identified by their Identity and Address.
/// Identity is insufficient because different Addresses can use the same Identity.
/// TODO: Determine if Address is sufficient for uniquely identifying a client.
type ClientId = (Identity, Address);
type Query = Arc<ExecutionUnit>;
type Client = Arc<ClientConnectionSender>;
type SwitchedDbUpdate = FormatSwitch<ws::DatabaseUpdate<BsatnFormat>, ws::DatabaseUpdate<JsonFormat>>;

type ClientRequestId = u32;
type SubscriptionId = (ClientId, ClientRequestId);

#[derive(Debug)]
struct ClientInfo {
    outbound_ref: Client,
    subscriptions: HashMap<SubscriptionId, QueryHash>,
    // This should be removed when we migrate to SubscribeSingle.
    legacy_subscriptions: HashSet<QueryHash>,
}

impl ClientInfo {
    fn new(outbound_ref: Client) -> Self {
        Self {
            outbound_ref,
            subscriptions: HashMap::default(),
            legacy_subscriptions: HashSet::default(),
        }
    }
}

#[derive(Debug)]
struct QueryState {
    query: Query,
    legacy_subscribers: HashSet<ClientId>,
    subscriptions: HashSet<SubscriptionId>,
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

    fn all_clients(&self) -> impl Iterator<Item = &ClientId> {
        let legacy_iter = self.legacy_subscribers.iter();
        let subscriptions_iter = self.subscriptions.iter().map(|(client_id, _)| client_id);
        legacy_iter.chain(subscriptions_iter)
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

impl SubscriptionManager {
    pub fn client(&self, id: &ClientId) -> Client {
        self.clients[id].outbound_ref.clone()
        //self.clients[id].clone()
    }

    pub fn query(&self, hash: &QueryHash) -> Option<Query> {
        self.queries.get(hash).map(|state| state.query.clone())
        // self.queries.get(hash).cloned()
    }

    pub fn num_unique_queries(&self) -> usize {
        self.queries.len()
    }

    #[cfg(test)]
    fn contains_query(&self, hash: &QueryHash) -> bool {
        self.queries.contains_key(hash)
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
                let query_state = self.queries.get_mut(query_hash);
                if query_state.is_none() {
                    tracing::warn!("Query state not found for query hash: {:?}", query_hash);
                    continue;
                }
                let query_state = query_state.unwrap();
                query_state.legacy_subscribers.remove(client);
                if !query_state.has_subscribers() {
                    SubscriptionManager::remove_table_query(
                        &mut self.tables,
                        query_state.query.return_table(),
                        query_hash,
                    );
                    SubscriptionManager::remove_table_query(
                        &mut self.tables,
                        query_state.query.filter_table(),
                        query_hash,
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

    pub fn remove_subscription(&mut self, client_id: ClientId, request_id: ClientRequestId) -> Result<Query, DBError> {
        let subscription_id = (client_id, request_id);
        let ci = if let Some(ci) = self.clients.get_mut(&client_id) {
            ci
        } else {
            return Err(anyhow::anyhow!("Client not found: {:?}", client_id).into());
        };

        let query_hash = if let Some(query_hash) = ci.subscriptions.remove(&subscription_id) {
            query_hash
        } else {
            return Err(anyhow::anyhow!("Subscription not found: {:?}", subscription_id).into());
        };
        let query_state = match self.queries.get_mut(&query_hash) {
            Some(query_state) => query_state,
            None => return Err(anyhow::anyhow!("Query state not found for query hash: {:?}", query_hash).into()),
        };
        let query = query_state.query.clone();
        // Check if the query has any subscribers left.
        let should_remove = {
            query_state.subscriptions.remove(&subscription_id);
            if !query_state.has_subscribers() {
                SubscriptionManager::remove_table_query(
                    &mut self.tables,
                    query_state.query.return_table(),
                    &query_hash,
                );
                SubscriptionManager::remove_table_query(
                    &mut self.tables,
                    query_state.query.filter_table(),
                    &query_hash,
                );
                true
            } else {
                false
            }
        };
        if should_remove {
            self.queries.remove(&query_hash);
        }
        Ok(query)
    }

    /// Adds a single subscription for a client.
    pub fn add_subscription(
        &mut self,
        client: Client,
        query: Query,
        request_id: ClientRequestId,
    ) -> Result<(), DBError> {
        let client_id = (client.id.identity, client.id.address);
        let ci = self
            .clients
            .entry(client_id)
            .or_insert_with(|| ClientInfo::new(client.clone()));
        let subscription_id = (client_id, request_id);
        let hash = query.hash();

        if let Err(OccupiedError { value, .. }) = ci.subscriptions.try_insert(subscription_id, hash) {
            return Err(anyhow::anyhow!(
                "Subscription with id {:?} already exists for client: {:?}",
                request_id,
                client_id
            )
            .into());
        }

        let query_state = self
            .queries
            .entry(hash)
            .or_insert_with(|| QueryState::new(query.clone()));

        // If this is new, we need to update the table to query mapping.
        if !query_state.has_subscribers() {
            self.tables.entry(query.return_table()).or_default().insert(hash);
            self.tables.entry(query.filter_table()).or_default().insert(hash);
            query_state.subscriptions.insert(subscription_id);
        }

        query_state.subscriptions.insert(subscription_id);

        Ok(())
    }

    /// Adds a client and its queries to the subscription manager.
    /// Sets up the set of subscriptions for the client, replacing any existing legacy subscriptions.
    ///
    /// If a query is not already indexed,
    /// its table ids added to the inverted index.
    // #[tracing::instrument(skip_all)]
    pub fn set_legacy_subscription(&mut self, client: Client, queries: impl IntoIterator<Item = Query>) {
        // TODO: Remove existing subscriptions.
        let client_id = (client.id.identity, client.id.address);
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
            self.tables.entry(unit.return_table()).or_default().insert(hash);
            self.tables.entry(unit.filter_table()).or_default().insert(hash);
            query_state.legacy_subscribers.insert(client_id);
            // self.subscribers.entry(hash).or_default().insert(id);
            // self.queries.insert(hash, unit);
        }
    }

    // Remove `hash` from the set of queries for `table_id`.
    // When the table has no queries, cleanup the map entry altogether.
    // This takes a ref to the table map instead of `self` to avoid borrowing issues.
    fn remove_table_query(tables: &mut IntMap<TableId, HashSet<QueryHash>>, table_id: TableId, hash: &QueryHash) {
        if let Entry::Occupied(mut entry) = tables.entry(table_id) {
            let hashes = entry.get_mut();
            if hashes.remove(hash) && hashes.is_empty() {
                entry.remove();
            }
        }
    }

    /// Removes a client from the subscriber mapping.
    /// If a query no longer has any subscribers,
    /// it is removed from the index along with its table ids.
    #[tracing::instrument(skip_all)]
    pub fn remove_all_subscriptions(&mut self, client: &ClientId) {
        self.remove_legacy_subscriptions(client);
        let client_info = self.clients.get(client);
        if client_info.is_none() {
            return;
        }
        let client_info = client_info.unwrap();
        debug_assert!(client_info.legacy_subscriptions.is_empty());
        let mut queries_to_remove = Vec::new();
        client_info.subscriptions.iter().for_each(|(sub_id, query_hash)| {
            let query_state = self.queries.get_mut(query_hash);
            if query_state.is_none() {
                tracing::warn!("Query state not found for query hash: {:?}", query_hash);
                return;
            }
            let query_state = query_state.unwrap();
            query_state.subscriptions.remove(sub_id);
            // This could happen twice for the same hash if a client has a duplicate, but that's fine. It is idepotent.
            if !query_state.has_subscribers() {
                queries_to_remove.push(*query_hash);
                SubscriptionManager::remove_table_query(&mut self.tables, query_state.query.return_table(), query_hash);
                SubscriptionManager::remove_table_query(&mut self.tables, query_state.query.filter_table(), query_hash);
            }
        });
        for query_hash in queries_to_remove {
            self.queries.remove(&query_hash);
        }
    }

    /// This method takes a set of delta tables,
    /// evaluates only the necessary queries for those delta tables,
    /// and then sends the results to each client.
    #[tracing::instrument(skip_all)]
    pub fn eval_updates(
        &self,
        db: &RelationalDB,
        tx: &Tx,
        event: Arc<ModuleEvent>,
        caller: Option<&ClientConnectionSender>,
        slow_query_threshold: Option<Duration>,
    ) {
        use FormatSwitch::{Bsatn, Json};

        let tables = &event.status.database_update().unwrap().tables;

        // Put the main work on a rayon compute thread.
        rayon::scope(|_| {
            // Collect the delta tables for each query.
            // For selects this is just a single table.
            // For joins it's two tables.
            let mut units: HashMap<_, ArrayVec<_, 2>> = HashMap::default();
            for table @ DatabaseTableUpdate { table_id, .. } in tables {
                if let Some(hashes) = self.tables.get(table_id) {
                    for hash in hashes {
                        units.entry(hash).or_insert_with(ArrayVec::new).push(table);
                    }
                }
            }

            let span = tracing::info_span!("eval_incr").entered();
            let tx = &tx.into();
            let mut eval = units
                .par_iter()
                .filter_map(|(&hash, tables)| {
                    let unit = &self.queries.get(hash)?.query;
                    unit.eval_incr(db, tx, &unit.sql, tables.iter().copied(), slow_query_threshold)
                        .map(|table| (hash, table))
                })
                // If N clients are subscribed to a query,
                // we copy the DatabaseTableUpdate N times,
                // which involves cloning BSATN (binary) or product values (json).
                .flat_map_iter(|(hash, delta)| {
                    let table_id = delta.table_id;
                    let table_name = delta.table_name;
                    // Store at most one copy of the serialization to BSATN
                    // and ditto for the "serialization" for JSON.
                    // Each subscriber gets to pick which of these they want,
                    // but we only fill `ops_bin` and `ops_json` at most once.
                    // The former will be `Some(_)` if some subscriber uses `Protocol::Binary`
                    // and the latter `Some(_)` if some subscriber uses `Protocol::Text`.
                    let mut ops_bin: Option<(CompressableQueryUpdate<BsatnFormat>, _)> = None;
                    let mut ops_json: Option<(QueryUpdate<JsonFormat>, _)> = None;

                    fn memo_encode<F: WebsocketFormat>(
                        updates: &UpdatesRelValue<'_>,
                        client: &ClientConnectionSender,
                        memory: &mut Option<(F::QueryUpdate, u64)>,
                    ) -> (F::QueryUpdate, u64) {
                        memory
                            .get_or_insert_with(|| updates.encode::<F>(client.config.compression))
                            .clone()
                    }

                    self.queries
                        .get(hash)
                        .into_iter()
                        .flat_map(|query| query.all_clients())
                        .map(move |id| {
                            let client = &self.clients[id].outbound_ref;
                            let update = match client.config.protocol {
                                Protocol::Binary => {
                                    Bsatn(memo_encode::<BsatnFormat>(&delta.updates, client, &mut ops_bin))
                                }
                                Protocol::Text => {
                                    Json(memo_encode::<JsonFormat>(&delta.updates, client, &mut ops_json))
                                }
                            };
                            (id, table_id, table_name.clone(), update)
                        })
                })
                .collect::<Vec<_>>()
                .into_iter()
                // For each subscriber, aggregate all the updates for the same table.
                // That is, we build a map `(subscriber_id, table_id) -> updates`.
                // A particular subscriber uses only one format,
                // so their `TableUpdate` will contain either JSON (`Protocol::Text`)
                // or BSATN (`Protocol::Binary`).
                .fold(
                    HashMap::<(&ClientId, TableId), FormatSwitch<TableUpdate<_>, TableUpdate<_>>>::new(),
                    |mut tables, (id, table_id, table_name, update)| {
                        match tables.entry((id, table_id)) {
                            Entry::Occupied(mut entry) => match entry.get_mut().zip_mut(update) {
                                Bsatn((tbl_upd, update)) => tbl_upd.push(update),
                                Json((tbl_upd, update)) => tbl_upd.push(update),
                            },
                            Entry::Vacant(entry) => drop(entry.insert(match update {
                                Bsatn(update) => Bsatn(TableUpdate::new(table_id, table_name, update)),
                                Json(update) => Json(TableUpdate::new(table_id, table_name, update)),
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
            drop(span);

            let _span = tracing::info_span!("eval_send").entered();

            // We might have a known caller that hasn't been hidden from here..
            // This caller may have subscribed to some query.
            // If they haven't, we'll send them an empty update.
            // Regardless, the update that we send to the caller, if we send any,
            // is a full tx update, rather than a light one.
            // That is, in the case of the caller, we don't respect the light setting.
            if let Some((caller, addr)) = caller.zip(event.caller_address) {
                let update = eval
                    .remove(&(event.caller_identity, addr))
                    .map(|update| SubscriptionUpdateMessage::from_event_and_update(&event, update))
                    .unwrap_or_else(|| {
                        SubscriptionUpdateMessage::default_for_protocol(caller.config.protocol, event.request_id)
                    });
                send_to_client(caller, Some(event.clone()), update);
            }

            // Send all the other updates.
            for (id, update) in eval {
                let message = SubscriptionUpdateMessage::from_event_and_update(&event, update);
                let client = self.client(id);
                // Conditionally send out a full update or a light one otherwise.
                let event = client.config.tx_update_full.then(|| event.clone());
                send_to_client(&client, event, message);
            }
        })
    }
}

fn send_to_client(
    client: &ClientConnectionSender,
    event: Option<Arc<ModuleEvent>>,
    database_update: SubscriptionUpdateMessage,
) {
    if let Err(e) = client.send_message(TransactionUpdateMessage { event, database_update }) {
        tracing::warn!(%client.id, "failed to send update message to client: {e}")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use spacetimedb_client_api_messages::timestamp::Timestamp;
    use spacetimedb_lib::{error::ResultTest, identity::AuthCtx, Address, AlgebraicType, Identity};
    use spacetimedb_primitives::TableId;
    use spacetimedb_vm::expr::CrudExpr;

    use super::SubscriptionManager;
    use crate::execution_context::Workload;
    use crate::subscription::module_subscription_manager::ClientRequestId;
    use crate::{
        client::{ClientActorId, ClientConfig, ClientConnectionSender, ClientName},
        db::relational_db::{tests_utils::TestDB, RelationalDB},
        energy::EnergyQuanta,
        host::{
            module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall},
            ArgsTuple,
        },
        sql::compiler::compile_sql,
        subscription::{
            execution_unit::{ExecutionUnit, QueryHash},
            subscription::SupportedQuery,
        },
    };

    fn create_table(db: &RelationalDB, name: &str) -> ResultTest<TableId> {
        Ok(db.create_table_for_test(name, &[("a", AlgebraicType::U8)], &[])?)
    }

    fn compile_plan(db: &RelationalDB, sql: &str) -> ResultTest<Arc<ExecutionUnit>> {
        db.with_read_only(Workload::ForTests, |tx| {
            let mut exprs = compile_sql(db, &AuthCtx::for_testing(), tx, sql)?;
            assert_eq!(1, exprs.len());
            assert!(matches!(exprs[0], CrudExpr::Query(_)));
            let CrudExpr::Query(query) = exprs.remove(0) else {
                unreachable!();
            };
            let plan = SupportedQuery::new(query, sql.to_owned())?;
            let hash = QueryHash::from_string(sql);
            Ok(Arc::new(ExecutionUnit::new(plan, hash)?))
        })
    }

    fn id(address: u128) -> (Identity, Address) {
        (Identity::ZERO, Address::from_u128(address))
    }

    fn client(address: u128) -> ClientConnectionSender {
        let (identity, address) = id(address);
        ClientConnectionSender::dummy(
            ClientActorId {
                identity,
                address,
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

        let id = id(0);
        let client = Arc::new(client(0));

        let request_id: ClientRequestId = 1;
        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id)?;
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

        let id = id(0);
        let client = Arc::new(client(0));

        let request_id: ClientRequestId = 1;
        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id)?;
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        let client_id = (client.id.identity, client.id.address);
        subscriptions.remove_subscription(client_id, request_id)?;
        assert!(!subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe_with_unknown_request_id_fails() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let request_id: ClientRequestId = 1;
        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id)?;

        let client_id = (client.id.identity, client.id.address);
        assert!(subscriptions.remove_subscription(client_id, 2).is_err());

        Ok(())
    }

    #[test]
    fn test_subscribe_and_unsubscribe_with_duplicate_queries() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let request_id: ClientRequestId = 1;
        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id)?;
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id + 1)?;

        let client_id = (client.id.identity, client.id.address);
        subscriptions.remove_subscription(client_id, request_id)?;

        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_subscribe_fails_with_duplicate_request_id() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let request_id: ClientRequestId = 1;
        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client.clone(), plan.clone(), request_id)?;

        assert!(subscriptions
            .add_subscription(client.clone(), plan.clone(), request_id)
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
            caller_address: Some(client0.id.address),
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
            subscriptions.eval_updates(&db, tx, event, Some(&client0), None)
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
