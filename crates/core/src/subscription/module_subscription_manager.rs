use super::subscription::{ExecutionUnit, QueryHash};
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
    // Subscribing identities and their client connections.
    clients: HashMap<Id, Client>,
    // Queries for which there is at least one subscriber.
    queries: HashMap<QueryHash, Query>,
    // Queries for which there is at least one subscriber.
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

    #[tracing::instrument(skip_all)]
    pub fn add_subscription(&mut self, client: Client, queries: impl Iterator<Item = Query>) {
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
                self.queries.remove(hash);
            }
            !ids.is_empty()
        });
        self.tables.retain(|_, queries| {
            queries.retain(|q| self.queries.contains_key(q));
            !queries.is_empty()
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn eval_updates(&self, db: &RelationalDB, auth: AuthCtx, event: Arc<ModuleEvent>) -> Result<(), DBError> {
        let tables = &event.status.database_update().unwrap().tables;
        let mut units: HashMap<_, SmallVec<[_; 2]>> = HashMap::new();

        for table @ DatabaseTableUpdate { table_id, .. } in tables {
            if let Some(hashes) = self.tables.get(table_id) {
                for hash in hashes {
                    units.entry(hash).or_insert_with(SmallVec::new).push(table);
                }
            }
        }

        let mut delta_tables = HashMap::new();
        let tx = db.begin_tx();

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

        tx.release(&ExecutionContext::incremental_update(db.address()));

        let mut database_updates = HashMap::new();
        for ((id, _), table) in delta_tables {
            if let Some(DatabaseUpdate { tables }) = database_updates.get_mut(id) {
                tables.push(table);
            } else {
                database_updates.insert(id, vec![table].into());
            }
        }

        let tokio_handle = &tokio::runtime::Handle::current();
        let mut tasks = vec![];

        for (id, update) in database_updates {
            let client = self.client(id);
            let event = event.clone();
            tasks.push(tokio_handle.spawn(async move {
                let _ = client.send(serialize_updates(update, &event, client.protocol)).await;
            }));
        }

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
