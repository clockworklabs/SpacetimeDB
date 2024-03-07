use super::execution_unit::{ExecutionUnit, QueryHash};
use crate::client::messages::{SubscriptionUpdate, TransactionUpdateMessage};
use crate::client::ClientConnectionSender;
use crate::db::relational_db::RelationalDB;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, ModuleEvent};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use smallvec::SmallVec;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::TableId;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    clients: HashMap<Identity, Client>,
    // Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, Query>,
    // The subscribers for each query.
    subscribers: HashMap<QueryHash, HashSet<Identity>>,
    // Inverted index from tables to queries that read from them.
    tables: HashMap<TableId, HashSet<QueryHash>>,
}

impl SubscriptionManager {
    pub fn client(&self, id: &Identity) -> Client {
        self.clients[id].clone()
    }

    pub fn query(&self, hash: &QueryHash) -> Option<Query> {
        self.queries.get(hash).map(Arc::clone)
    }

    pub fn num_queries(&self) -> usize {
        self.queries.len()
    }

    #[cfg(test)]
    fn contains_query(&self, hash: &QueryHash) -> bool {
        self.queries.contains_key(hash)
    }

    #[cfg(test)]
    fn contains_subscription(&self, subscriber: &Identity, query: &QueryHash) -> bool {
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
        let id = client.id.identity;
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
    pub fn remove_subscription(&mut self, client: &Identity) {
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
    pub fn eval_updates(&self, db: &RelationalDB, event: &ModuleEvent) {
        let tables = &event.status.database_update().unwrap().tables;

        let tx = scopeguard::guard(db.begin_tx(), |tx| {
            tx.release(&ExecutionContext::incremental_update(db.address()));
        });

        // Put the main work on a rayon compute thread.
        rayon::scope(|_| {
            // Collect the delta tables for each query.
            // For selects this is just a single table.
            // For joins it's two tables.
            let mut units: HashMap<_, SmallVec<[_; 2]>> = HashMap::new();
            for table @ DatabaseTableUpdate { table_id, .. } in tables {
                if let Some(hashes) = self.tables.get(table_id) {
                    for hash in hashes {
                        units.entry(hash).or_insert_with(SmallVec::new).push(table);
                    }
                }
            }

            units
                .into_par_iter()
                .filter_map(|(hash, tables)| self.queries.get(hash).map(|unit| (hash, tables, unit)))
                .filter_map(|(hash, tables, unit)| {
                    match unit.eval_incr(db, &tx, tables.into_iter()) {
                        Ok(None) => None,
                        Ok(Some(table)) => Some((hash, table)),
                        Err(err) => {
                            // TODO: log an id for the subscription somehow as well
                            tracing::error!(err = &err as &dyn std::error::Error, "subscription eval_incr failed");
                            None
                        }
                    }
                })
                // If N clients are subscribed to a query,
                // we copy the DatabaseTableUpdate N times,
                // which involves cloning product values.
                // TODO(perf): In order to reduce heap allocations,
                // we should serialize and memcpy bsatn directly.
                .flat_map_iter(|(hash, delta)| {
                    self.subscribers
                        .get(hash)
                        .into_iter()
                        .flatten()
                        .map(move |id| (id, delta.table_id, delta.clone()))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .fold(
                    HashMap::<_, DatabaseTableUpdate>::new(),
                    |mut tables, (id, table_id, delta)| {
                        match tables.entry((id, table_id)) {
                            Entry::Occupied(mut entry) => entry.get_mut().ops.extend(delta.ops),
                            Entry::Vacant(entry) => drop(entry.insert(delta)),
                        }
                        tables
                    },
                )
                .into_iter()
                // Each client receives a single DatabaseUpdate per transaction.
                // So before sending an update to each client,
                // we must stitch together the DatabaseTableUpdates into a final DatabaseUpdate.
                .fold(HashMap::<_, DatabaseUpdate>::new(), |mut updates, ((id, _), delta)| {
                    updates.entry(id).or_default().tables.push(delta);
                    updates
                })
                .into_iter()
                .for_each(|(id, database_update)| {
                    let client = self.client(id);
                    let database_update = SubscriptionUpdate {
                        database_update,
                        request_id: event.request_id,
                        timer: event.timer,
                    };
                    let message = TransactionUpdateMessage { event, database_update };
                    if let Err(e) = client.send_message(message) {
                        tracing::warn!(%client.id, "failed to send update message to client: {e}")
                    }
                });
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_primitives::TableId;
    use spacetimedb_vm::expr::CrudExpr;

    use crate::{
        client::{ClientActorId, ClientConnectionSender, Protocol},
        db::relational_db::{tests_utils::make_test_db, RelationalDB},
        execution_context::ExecutionContext,
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

    #[test]
    fn test_subscribe() -> ResultTest<()> {
        let (db, _) = make_test_db()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = Identity::ZERO;
        let client = ClientActorId::for_test(id);
        let client = Arc::new(ClientConnectionSender::dummy(client, Protocol::Binary));

        let mut subscriptions = SubscriptionManager::default();
        subscriptions.add_subscription(client, [plan]);

        assert!(subscriptions.contains_query(&hash));
        assert!(subscriptions.contains_subscription(&id, &hash));
        assert!(subscriptions.query_reads_from_table(&hash, &table_id));

        Ok(())
    }

    #[test]
    fn test_unsubscribe() -> ResultTest<()> {
        let (db, _) = make_test_db()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = Identity::ZERO;
        let client = ClientActorId::for_test(id);
        let client = Arc::new(ClientConnectionSender::dummy(client, Protocol::Binary));

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
        let (db, _) = make_test_db()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id = Identity::ZERO;
        let client = ClientActorId::for_test(id);
        let client = Arc::new(ClientConnectionSender::dummy(client, Protocol::Binary));

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
        let (db, _) = make_test_db()?;

        let table_id = create_table(&db, "T")?;
        let sql = "select * from T";
        let plan = compile_plan(&db, sql)?;
        let hash = plan.hash();

        let id0 = Identity::ZERO;
        let client0 = ClientActorId::for_test(id0);
        let client0 = Arc::new(ClientConnectionSender::dummy(client0, Protocol::Binary));

        let id1 = Identity::from_byte_array([1; 32]);
        let client1 = ClientActorId::for_test(id1);
        let client1 = Arc::new(ClientConnectionSender::dummy(client1, Protocol::Binary));

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
        let (db, _) = make_test_db()?;

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

        let id0 = Identity::ZERO;
        let client0 = ClientActorId::for_test(id0);
        let client0 = Arc::new(ClientConnectionSender::dummy(client0, Protocol::Binary));

        let id1 = Identity::from_byte_array([1; 32]);
        let client1 = ClientActorId::for_test(id1);
        let client1 = Arc::new(ClientConnectionSender::dummy(client1, Protocol::Binary));

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
}
