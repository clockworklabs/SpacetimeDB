use super::execution_unit::{ExecutionUnit, QueryHash};
use crate::client::messages::{CachedMessage, TransactionUpdateMessage};
use crate::client::{ClientConnectionSender, DataMessage, Protocol};
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseTableUpdate, DatabaseUpdate, ModuleEvent};
use smallvec::SmallVec;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::TableId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

type Id = Identity;
type Query = Arc<ExecutionUnit>;
type Client = Arc<ClientConnectionSender>;

/// Responsible for the efficient evaluation of subscriptions.
#[derive(Debug, Default)]
pub struct SubscriptionManager {
    // Subscriber identities and their client connections.
    clients: HashMap<Id, Client>,
    // Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, Query>,
    // The subscribers for each query.
    subscribers: HashMap<QueryHash, HashSet<Id>>,
    // Inverted index from tables to queries that read from them.
    tables: HashMap<TableId, HashSet<QueryHash>>,
}

impl SubscriptionManager {
    pub fn client(&self, id: &Id) -> Client {
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
    fn contains_subscription(&self, subscriber: &Id, query: &QueryHash) -> bool {
        self.subscribers.get(query).is_some_and(|ids| ids.contains(subscriber))
    }

    #[cfg(test)]
    fn query_reads_from_table(&self, query: &QueryHash, table: &TableId) -> bool {
        self.tables.get(table).is_some_and(|queries| queries.contains(query))
    }

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

    #[tracing::instrument(skip_all)]
    pub fn remove_subscription(&mut self, client: &Id) {
        self.clients.remove(client);
        self.subscribers.retain(|hash, ids| {
            ids.remove(client);
            if ids.is_empty() {
                if let Some(query) = self.queries.remove(hash) {
                    if self
                        .tables
                        .get_mut(&query.return_table())
                        .is_some_and(|hashes| hashes.remove(hash) && hashes.is_empty())
                    {
                        self.tables.remove(&query.return_table());
                    }
                    if self
                        .tables
                        .get_mut(&query.filter_table())
                        .is_some_and(|hashes| hashes.remove(hash) && hashes.is_empty())
                    {
                        self.tables.remove(&query.filter_table());
                    }
                }
            }
            !ids.is_empty()
        });
    }

    #[tracing::instrument(skip_all)]
    pub async fn eval_updates(&self, db: &RelationalDB, auth: AuthCtx, event: Arc<ModuleEvent>) -> Result<(), DBError> {
        let tokio_handle = &tokio::runtime::Handle::current();
        let tables = &event.status.database_update().unwrap().tables;
        let tx = db.begin_tx();
        let tasks = rayon::scope(|_| -> Result<_, DBError> {
            let mut units: HashMap<_, SmallVec<[_; 2]>> = HashMap::new();

            for table @ DatabaseTableUpdate { table_id, .. } in tables {
                if let Some(hashes) = self.tables.get(table_id) {
                    for hash in hashes {
                        units.entry(hash).or_insert_with(SmallVec::new).push(table);
                    }
                }
            }

            let mut delta_tables = HashMap::new();

            for (hash, tables) in units {
                if let Some(unit) = self.queries.get(hash) {
                    if let Some(delta) = unit.eval_incr(db, &tx, tables.into_iter(), auth)? {
                        for id in self.subscribers.get(hash).into_iter().flatten() {
                            delta_tables
                                .entry((id, delta.table_id))
                                .and_modify(|table: &mut DatabaseTableUpdate| table.ops.extend(delta.ops.clone()))
                                .or_insert_with(|| delta.clone());
                        }
                    }
                }
            }

            let mut database_updates = HashMap::new();
            for ((id, _), table) in delta_tables {
                if let Some(DatabaseUpdate { tables }) = database_updates.get_mut(id) {
                    tables.push(table);
                } else {
                    database_updates.insert(id, vec![table].into());
                }
            }

            let mut tasks = vec![];

            for (id, update) in database_updates {
                let client = self.client(id);
                let event = event.clone();
                tasks.push(tokio_handle.spawn(async move {
                    let _ = client.send(serialize_updates(update, &event, client.protocol)).await;
                }));
            }
            Ok(tasks)
        })?;

        tx.release(&ExecutionContext::incremental_update(db.address()));

        for task in tasks {
            let _ = task.await;
        }
        Ok(())
    }
}

fn serialize_updates(database_update: DatabaseUpdate, event: &ModuleEvent, protocol: Protocol) -> DataMessage {
    let message = TransactionUpdateMessage { event, database_update };
    let mut cached = CachedMessage::new(message);
    cached.serialize(protocol)
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
            Ok(Arc::new(ExecutionUnit::new(plan, hash)))
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
