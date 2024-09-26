use super::execution_unit::{ExecutionUnit, QueryHash};
use crate::client::messages::{SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientConnectionSender, Protocol};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, ModuleEvent};
use crate::messages::websocket::{self as ws, TableUpdate};
use arrayvec::ArrayVec;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CompressableQueryUpdate, FormatSwitch, JsonFormat, QueryUpdate,
};
use spacetimedb_data_structures::map::{Entry, HashMap, HashSet, IntMap};
use spacetimedb_lib::{Address, Identity};
use spacetimedb_primitives::TableId;
use std::sync::Arc;
use std::time::Duration;

/// Clients are uniquely identified by their Identity and Address.
/// Identity is insufficient because different Addresses can use the same Identity.
/// TODO: Determine if Address is sufficient for uniquely identifying a client.
type Id = (Identity, Address);
type Query = Arc<ExecutionUnit>;
type Client = Arc<ClientConnectionSender>;

/// Responsible for the efficient evaluation of subscriptions.
/// It performs basic multi-query optimization,
/// in that if a query has N subscribers,
/// it is only executed once,
/// with the results copied to the N receivers.
#[derive(Debug, Default)]
pub struct SubscriptionManager {
    // Subscriber identities and their client connections.
    clients: HashMap<Id, Client>,
    // Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, Query>,
    // The subscribers for each query.
    subscribers: HashMap<QueryHash, HashSet<Id>>,
    // Inverted index from tables to queries that read from them.
    tables: IntMap<TableId, HashSet<QueryHash>>,
}

impl SubscriptionManager {
    pub fn client(&self, id: &Id) -> Client {
        self.clients[id].clone()
    }

    pub fn query(&self, hash: &QueryHash) -> Option<Query> {
        self.queries.get(hash).cloned()
    }

    pub fn num_queries(&self) -> usize {
        self.queries.len()
    }

    #[cfg(test)]
    fn contains_query(&self, hash: &QueryHash) -> bool {
        self.queries.contains_key(hash)
    }

    #[cfg(test)]
    fn contains_subscription(&self, subscriber: &Id, query: &QueryHash) -> bool {
        self.subscribers.get(query).is_some_and(|ids| ids.contains(subscriber))
    }

    #[cfg(test)]
    fn query_reads_from_table(&self, query: &QueryHash, table: &TableId) -> bool {
        self.tables.get(table).is_some_and(|queries| queries.contains(query))
    }

    /// Adds a client and its queries to the subscription manager.
    /// If a query is not already indexed,
    /// its table ids added to the inverted index.
    #[tracing::instrument(skip_all)]
    pub fn add_subscription(&mut self, client: Client, queries: impl IntoIterator<Item = Query>) {
        let id = (client.id.identity, client.id.address);
        self.clients.insert(id, client);
        for unit in queries {
            let hash = unit.hash();
            self.tables.entry(unit.return_table()).or_default().insert(hash);
            self.tables.entry(unit.filter_table()).or_default().insert(hash);
            self.subscribers.entry(hash).or_default().insert(id);
            self.queries.insert(hash, unit);
        }
    }

    /// Removes a client from the subscriber mapping.
    /// If a query no longer has any subscribers,
    /// it is removed from the index along with its table ids.
    #[tracing::instrument(skip_all)]
    pub fn remove_subscription(&mut self, client: &Id) {
        // Remove `hash` from the set of queries for `table_id`.
        // When the table has no queries, cleanup the map entry altogether.
        let mut remove_table_query = |table_id: TableId, hash: &QueryHash| {
            if let Entry::Occupied(mut entry) = self.tables.entry(table_id) {
                let hashes = entry.get_mut();
                if hashes.remove(hash) && hashes.is_empty() {
                    entry.remove();
                }
            }
        };

        self.clients.remove(client);
        self.subscribers.retain(|hash, ids| {
            ids.remove(client);
            if ids.is_empty() {
                if let Some(query) = self.queries.remove(hash) {
                    remove_table_query(query.return_table(), hash);
                    remove_table_query(query.filter_table(), hash);
                }
            }
            !ids.is_empty()
        });
    }

    /// This method takes a set of delta tables,
    /// evaluates only the necessary queries for those delta tables,
    /// and then sends the results to each client.
    #[tracing::instrument(skip_all)]
    pub fn eval_updates(
        &self,
        ctx: &ExecutionContext,
        db: &RelationalDB,
        tx: &Tx,
        event: Arc<ModuleEvent>,
        sender_client: Option<&ClientConnectionSender>,
        slow_query_threshold: Option<Duration>,
    ) {
        let tables = &event.status.database_update().unwrap().tables;

        // Put the main work on a rayon compute thread.
        rayon::scope(|_| {
            // Collect the delta tables for each query.
            // For selects this is just a single table.
            // For joins it's two tables.
            let mut units: HashMap<_, ArrayVec<_, 2>> = HashMap::new();
            for table @ DatabaseTableUpdate { table_id, .. } in tables {
                if let Some(hashes) = self.tables.get(table_id) {
                    for hash in hashes {
                        units.entry(hash).or_insert_with(ArrayVec::new).push(table);
                    }
                }
            }

            let span = tracing::info_span!("eval_incr").entered();
            let tx = &tx.into();
            let eval = units
                .par_iter()
                .filter_map(|(&hash, tables)| {
                    let unit = self.queries.get(hash)?;
                    unit.eval_incr(ctx, db, tx, &unit.sql, tables.iter().copied(), slow_query_threshold)
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
                    self.subscribers.get(hash).into_iter().flatten().map(move |id| {
                        let ops = match self.clients[id].protocol {
                            Protocol::Binary => FormatSwitch::Bsatn(
                                ops_bin
                                    .get_or_insert_with(|| delta.updates.encode::<BsatnFormat>())
                                    .clone(),
                            ),
                            Protocol::Text => FormatSwitch::Json(
                                ops_json
                                    .get_or_insert_with(|| delta.updates.encode::<JsonFormat>())
                                    .clone(),
                            ),
                        };
                        (id, table_id, table_name.clone(), ops)
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
                    HashMap::<(&Id, TableId), FormatSwitch<TableUpdate<_>, TableUpdate<_>>>::new(),
                    |mut tables, (id, table_id, table_name, update)| {
                        match tables.entry((id, table_id)) {
                            Entry::Occupied(mut entry) => {
                                let tbl_upd = entry.get_mut();
                                match tbl_upd.zip_mut(update) {
                                    FormatSwitch::Bsatn((tbl_upd, (update, num_rows))) => {
                                        tbl_upd.updates.push(update);
                                        tbl_upd.num_rows += num_rows;
                                    }
                                    FormatSwitch::Json((tbl_upd, (update, num_rows))) => {
                                        tbl_upd.updates.push(update);
                                        tbl_upd.num_rows += num_rows;
                                    }
                                }
                            }
                            Entry::Vacant(entry) => {
                                let table_name = table_name.into();
                                entry.insert(match update {
                                    FormatSwitch::Bsatn((update, num_rows)) => FormatSwitch::Bsatn(TableUpdate {
                                        table_id,
                                        table_name,
                                        num_rows,
                                        updates: [update].into(),
                                    }),
                                    FormatSwitch::Json((update, num_rows)) => FormatSwitch::Json(TableUpdate {
                                        table_id,
                                        table_name,
                                        num_rows,
                                        updates: [update].into(),
                                    }),
                                });
                            }
                        }
                        tables
                    },
                )
                .into_iter()
                // Each client receives a single list of updates per transaction.
                // So before sending the updates to each client,
                // we must stitch together the `TableUpdate*`s into an aggregated list.
                .fold(
                    HashMap::<&Id, FormatSwitch<ws::DatabaseUpdate<_>, ws::DatabaseUpdate<_>>>::new(),
                    |mut updates, ((id, _), update)| {
                        let entry = updates.entry(id);
                        let entry = entry.or_insert_with(|| match &update {
                            FormatSwitch::Bsatn(_) => FormatSwitch::Bsatn(<_>::default()),
                            FormatSwitch::Json(_) => FormatSwitch::Json(<_>::default()),
                        });
                        match entry.zip_mut(update) {
                            FormatSwitch::Bsatn((list, elem)) => list.tables.push(elem),
                            FormatSwitch::Json((list, elem)) => list.tables.push(elem),
                        }
                        updates
                    },
                );
            drop(span);

            let _span = tracing::info_span!("eval_send").entered();

            if let Some((_, client)) = event
                .caller_address
                .zip(sender_client)
                .filter(|(addr, _)| !eval.contains_key(&(event.caller_identity, *addr)))
            {
                // Caller is not subscribed to any queries,
                // but send a transaction update with an empty subscription update.
                let update = SubscriptionUpdateMessage::default_for_protocol(client.protocol, event.request_id);
                send_to_client(client, &event, update);
            }

            eval.into_iter().for_each(|(id, tables)| {
                let database_update = SubscriptionUpdateMessage {
                    database_update: tables,
                    request_id: event.request_id,
                    timer: event.timer,
                };
                send_to_client(self.client(id).as_ref(), &event, database_update);
            });
        })
    }
}

fn send_to_client(
    client: &ClientConnectionSender,
    event: &Arc<ModuleEvent>,
    database_update: SubscriptionUpdateMessage,
) {
    if let Err(e) = client.send_message(TransactionUpdateMessage {
        event: event.clone(),
        database_update,
    }) {
        tracing::warn!(%client.id, "failed to send update message to client: {e}")
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use spacetimedb_client_api_messages::timestamp::Timestamp;
    use spacetimedb_lib::{error::ResultTest, Address, AlgebraicType, Identity};
    use spacetimedb_primitives::TableId;
    use spacetimedb_vm::expr::CrudExpr;

    use crate::{
        client::{ClientActorId, ClientConnectionSender, ClientName, Protocol},
        db::relational_db::{tests_utils::TestDB, RelationalDB},
        energy::EnergyQuanta,
        execution_context::ExecutionContext,
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

    use super::SubscriptionManager;

    fn create_table(db: &RelationalDB, name: &str) -> ResultTest<TableId> {
        Ok(db.create_table_for_test(name, &[("a", AlgebraicType::U8)], &[])?)
    }

    fn compile_plan(db: &RelationalDB, sql: &str) -> ResultTest<Arc<ExecutionUnit>> {
        db.with_read_only(&ExecutionContext::default(), |tx| {
            let mut exprs = compile_sql(db, tx, sql)?;
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
        ClientConnectionSender::dummy(
            ClientActorId {
                identity: Identity::ZERO,
                address: Address::from_u128(address),
                name: ClientName(0),
            },
            Protocol::Binary,
        )
    }

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let db = TestDB::durable()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = id(0);
        let client = Arc::new(client(0));

        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client, [plan]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_subscription(&id, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

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
        subscriptions.add_subscription(client, [plan]);
        subscriptions.remove_subscription(&id);

        assert!(!subscriptions.contains_query(&hash));
        assert!(!subscriptions.contains_subscription(&id, &hash));
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
        subscriptions.add_subscription(client.clone(), [plan.clone()]);
        subscriptions.add_subscription(client.clone(), [plan.clone()]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_subscription(&id, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        subscriptions.remove_subscription(&id);

        assert!(!subscriptions.contains_query(&hash));
        assert!(!subscriptions.contains_subscription(&id, &hash));
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
        subscriptions.add_subscription(client0, [plan.clone()]);
        subscriptions.add_subscription(client1, [plan.clone()]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_subscription(&id0, &hash));
        assert!(subscriptions.contains_subscription(&id1, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        subscriptions.remove_subscription(&id0);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_subscription(&id1, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        assert!(!subscriptions.contains_subscription(&id0, &hash));

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
        subscriptions.add_subscription(client0, [plan_scan.clone(), plan_select0.clone()]);
        subscriptions.add_subscription(client1, [plan_scan.clone(), plan_select1.clone()]);

        assert!(subscriptions.contains_query(&hash_scan));
        assert!(subscriptions.contains_query(&hash_select0));
        assert!(subscriptions.contains_query(&hash_select1));

        assert!(subscriptions.contains_subscription(&id0, &hash_scan));
        assert!(subscriptions.contains_subscription(&id0, &hash_select0));

        assert!(subscriptions.contains_subscription(&id1, &hash_scan));
        assert!(subscriptions.contains_subscription(&id1, &hash_select1));

        assert!(subscriptions.query_reads_from_table(&hash_scan, &t));
        assert!(subscriptions.query_reads_from_table(&hash_select0, &t));
        assert!(subscriptions.query_reads_from_table(&hash_select1, &s));

        assert!(!subscriptions.query_reads_from_table(&hash_scan, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select0, &s));
        assert!(!subscriptions.query_reads_from_table(&hash_select1, &t));

        subscriptions.remove_subscription(&id0);

        assert!(subscriptions.contains_query(&hash_scan));
        assert!(subscriptions.contains_query(&hash_select1));
        assert!(!subscriptions.contains_query(&hash_select0));

        assert!(subscriptions.contains_subscription(&id1, &hash_scan));
        assert!(subscriptions.contains_subscription(&id1, &hash_select1));

        assert!(!subscriptions.contains_subscription(&id0, &hash_scan));
        assert!(!subscriptions.contains_subscription(&id0, &hash_select0));

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
        let (client0, mut rx) = ClientConnectionSender::dummy_with_channel(client0, Protocol::Binary);

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

        let ctx = ExecutionContext::incremental_update(db.address());
        db.with_read_only(&ctx, |tx| {
            subscriptions.eval_updates(&ctx, &db, tx, event, Some(&client0), None)
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
