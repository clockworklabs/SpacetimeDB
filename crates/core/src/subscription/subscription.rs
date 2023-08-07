use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::RelValue;
use spacetimedb_lib::PrimaryKey;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_vm::expr::QueryExpr;
use std::collections::HashSet;

use super::query::Query;
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::DBError;
use crate::subscription::query::{run_query, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};

pub struct Subscription {
    pub queries: QuerySet,
    pub subscribers: Vec<ClientConnectionSender>,
}

pub struct QuerySet(pub Vec<Query>);

impl FromIterator<Query> for QuerySet {
    fn from_iter<T: IntoIterator<Item = Query>>(iter: T) -> Self {
        QuerySet(Vec::from_iter(iter))
    }
}

impl PartialEq for QuerySet {
    fn eq(&self, other: &Self) -> bool {
        let mut a = self.0.clone();
        let mut b = other.0.clone();
        a.sort();
        b.sort();
        a == b
    }
}

impl Subscription {
    pub fn remove_subscriber(&mut self, client_id: ClientActorId) -> Option<ClientConnectionSender> {
        let i = self.subscribers.iter().position(|sub| sub.id == client_id)?;
        Some(self.subscribers.swap_remove(i))
    }

    pub fn add_subscriber(&mut self, sender: ClientConnectionSender) {
        if !self.subscribers.iter().any(|s| s.id == sender.id) {
            self.subscribers.push(sender);
        }
    }
}

// If a RelValue has an id (DataKey) return it directly, otherwise we must construct it from the
// row itself which can be an expensive operation.
fn pk_for_row(row: &RelValue) -> PrimaryKey {
    match row.id {
        Some(data_key) => PrimaryKey { data_key },
        None => RelationalDB::pk_for_row(&row.data),
    }
}

impl QuerySet {
    /// Queries all the [`StTableType::User`] tables *right now*
    /// and turns them into [`QueryExpr`],
    /// the moral equivalent of `SELECT * FROM table`.
    pub(crate) fn get_all(relational_db: &RelationalDB, tx: &MutTxId, auth: &AuthCtx) -> Result<Query, DBError> {
        let tables = relational_db.get_all_tables(tx)?;
        let same_owner = auth.owner == auth.caller;
        let queries = tables
            .iter()
            .filter(|t| t.table_type == StTableType::User && (same_owner || t.table_access == StAccess::Public))
            .map(QueryExpr::new)
            .collect();

        Ok(Query { queries })
    }

    /// Incremental evaluation of `rows` that matched the [Query] (aka subscriptions)
    ///
    /// This is equivalent to run a `trigger` on `INSERT/UPDATE/DELETE`, run the [Query] and see if the `row` is matched.
    ///
    /// NOTE: The returned `rows` in [DatabaseUpdate] are **deduplicated** so if 2 queries match the same `row`, only one copy is returned.
    #[tracing::instrument(skip_all)]
    pub fn eval_incr(
        &self,
        relational_db: &RelationalDB,
        tx: &mut MutTxId,
        database_update: &DatabaseUpdate,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };
        let mut seen = HashSet::new();

        for query in &self.0 {
            for table in database_update.tables.iter().cloned() {
                for q in query.queries_of_table_id(&table) {
                    if let Some(result) = run_query(relational_db, tx, &q, auth)?
                        .into_iter()
                        .find(|x| !x.data.is_empty())
                    {
                        let pos_op_type = result.head.find_pos_by_name(OP_TYPE_FIELD_NAME).unwrap_or_else(|| {
                            panic!(
                                "failed to locate `{OP_TYPE_FIELD_NAME}` on `{}`. fields: {:?}",
                                result.head.table_name,
                                result.head.fields.iter().map(|x| &x.field).collect::<Vec<_>>()
                            )
                        });

                        // Optimistically assume we have seen nothing in the row list.
                        let mut ops = Vec::with_capacity(result.data.len());
                        for mut row in result.data {
                            // Hack: remove the hidden field OP_TYPE_FIELD_NAME.
                            // See `to_mem_table`
                            // Needs to be done before calculating the PK.
                            let mut data: Vec<_> = row.data.elements.into();
                            let op_type = if let AlgebraicValue::U8(op) = data.remove(pos_op_type) {
                                op
                            } else {
                                panic!("Fail to extract `{OP_TYPE_FIELD_NAME}` on `{}`", result.head.table_name)
                            };
                            row.data = data.try_into().unwrap();

                            let row_pk = pk_for_row(&row);

                            // Skip rows that are already resolved in a previous subscription.
                            if seen.contains(&(table.table_id, row_pk)) {
                                continue;
                            }

                            seen.insert((table.table_id, row_pk));

                            let row_pk = row_pk.to_bytes().into();
                            let row = row.data;
                            ops.push(TableOp { op_type, row_pk, row });
                        }
                        output.tables.push(DatabaseTableUpdate {
                            table_id: table.table_id,
                            table_name: table.table_name.clone(),
                            ops: ops.into(),
                        });
                    }
                }
            }
        }

        Ok(output)
    }

    /// Direct execution of [Query] (aka subscriptions)
    ///
    /// This is equivalent to run a direct query like `SELECT * FROM table` and get back all the `rows` that match it.
    ///
    /// NOTE: The returned `rows` in [DatabaseUpdate] are **deduplicated** so if 2 queries match the same `row`, only one copy is returned.
    ///
    /// This is a *major* difference with normal query execution, where is expected to return the full result set for each query.
    #[tracing::instrument(skip_all)]
    pub fn eval(
        &self,
        relational_db: &RelationalDB,
        tx: &mut MutTxId,
        auth: AuthCtx,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut database_update: DatabaseUpdate = DatabaseUpdate { tables: vec![] };
        let mut seen = HashSet::new();

        for query in &self.0 {
            for q in &query.queries {
                if let Some(t) = q.source.get_db_table() {
                    for table in run_query(relational_db, tx, q, auth)? {
                        {
                            let mut table_row_operations = Vec::new();

                            for row in table.data {
                                let row_pk = pk_for_row(&row);

                                //Skip rows that are already resolved in a previous subscription...
                                if seen.contains(&(t.table_id, row_pk)) {
                                    continue;
                                }
                                seen.insert((t.table_id, row_pk));

                                let row_pk = row_pk.to_bytes().into();
                                let row = row.data;
                                table_row_operations.push(TableOp {
                                    op_type: 1, // Insert
                                    row_pk,
                                    row,
                                });
                            }

                            database_update.tables.push(DatabaseTableUpdate {
                                table_id: t.table_id,
                                table_name: t.head.table_name.clone(),
                                ops: table_row_operations.into(),
                            });
                        }
                    }
                }
            }
        }
        Ok(database_update)
    }
}
