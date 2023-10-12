use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::DBError;
use crate::subscription::query::{run_query, OP_TYPE_FIELD_NAME};
use crate::{
    client::{ClientActorId, ClientConnectionSender},
    db::relational_db::RelationalDB,
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};
use derive_more::{Deref, DerefMut, From, IntoIterator};
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::relation::RelValue;
use spacetimedb_lib::PrimaryKey;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_vm::expr::QueryExpr;
use std::collections::HashMap;
use std::collections::{btree_set, BTreeSet, HashSet};
use std::ops::Deref;

use super::query::to_mem_table;

pub struct Subscription {
    pub queries: QuerySet,
    pub subscribers: Vec<ClientConnectionSender>,
}

#[derive(Deref, DerefMut, PartialEq, From, IntoIterator)]
pub struct QuerySet(BTreeSet<QueryExpr>);

impl QuerySet {
    pub const fn new() -> Self {
        Self(BTreeSet::new())
    }

    #[tracing::instrument(skip_all, fields(table = table.table_name))]
    pub fn queries_of_table_id<'a>(&'a self, table: &'a DatabaseTableUpdate) -> impl Iterator<Item = QueryExpr> + '_ {
        self.0.iter().filter_map(move |x| {
            if x.source.get_db_table().map(|x| x.table_id) == Some(table.table_id) {
                let t = to_mem_table(x.clone(), table);
                Some(t)
            } else {
                None
            }
        })
    }
}

impl From<QueryExpr> for QuerySet {
    fn from(expr: QueryExpr) -> Self {
        [expr].into()
    }
}

impl<const N: usize> From<[QueryExpr; N]> for QuerySet {
    fn from(exprs: [QueryExpr; N]) -> Self {
        Self(exprs.into())
    }
}

impl FromIterator<QueryExpr> for QuerySet {
    fn from_iter<T: IntoIterator<Item = QueryExpr>>(iter: T) -> Self {
        QuerySet(BTreeSet::from_iter(iter))
    }
}

impl<'a> IntoIterator for &'a QuerySet {
    type Item = &'a QueryExpr;
    type IntoIter = btree_set::Iter<'a, QueryExpr>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Extend<QueryExpr> for QuerySet {
    fn extend<T: IntoIterator<Item = QueryExpr>>(&mut self, iter: T) {
        self.0.extend(iter)
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
    pub(crate) fn get_all(relational_db: &RelationalDB, tx: &MutTxId, auth: &AuthCtx) -> Result<Self, DBError> {
        let tables = relational_db.get_all_tables(tx)?;
        let same_owner = auth.owner == auth.caller;
        let exprs = tables
            .iter()
            .map(Deref::deref)
            .filter(|t| t.table_type == StTableType::User && (same_owner || t.table_access == StAccess::Public))
            .map(QueryExpr::new)
            .collect();

        Ok(Self(exprs))
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
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for table in database_update.tables.iter().cloned() {
            // Get the TableOps for this table
            let (_, table_row_operations) = table_ops
                .entry(table.table_id)
                .or_insert_with(|| (table.table_name.clone(), vec![]));
            for q in self.queries_of_table_id(&table) {
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

                    for mut row in result.data {
                        //Hack: remove the hidden field OP_TYPE_FIELD_NAME. see `to_mem_table`
                        // Needs to be done before calculating the PK.
                        let op_type = if let AlgebraicValue::U8(op) = row.data.elements.remove(pos_op_type) {
                            op
                        } else {
                            panic!("Fail to extract `{OP_TYPE_FIELD_NAME}` on `{}`", result.head.table_name)
                        };

                        let row_pk = pk_for_row(&row);

                        //Skip rows that are already resolved in a previous subscription...
                        if seen.contains(&(table.table_id, row_pk)) {
                            continue;
                        }

                        seen.insert((table.table_id, row_pk));

                        let row_pk = row_pk.to_bytes();
                        let row = row.data;
                        table_row_operations.push(TableOp { op_type, row_pk, row });
                    }
                }
            }
        }
        for (table_id, (table_name, ops)) in table_ops.into_iter().filter(|(_, (_, ops))| !ops.is_empty()) {
            output.tables.push(DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            });
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
        let mut table_ops = HashMap::new();
        let mut seen = HashSet::new();

        for q in &self.0 {
            if let Some(t) = q.source.get_db_table() {
                // Get the TableOps for this table
                let (_, table_row_operations) = table_ops
                    .entry(t.table_id)
                    .or_insert_with(|| (t.head.table_name.clone(), vec![]));
                for table in run_query(relational_db, tx, q, auth)? {
                    for row in table.data {
                        let row_pk = pk_for_row(&row);

                        //Skip rows that are already resolved in a previous subscription...
                        if seen.contains(&(t.table_id, row_pk)) {
                            continue;
                        }
                        seen.insert((t.table_id, row_pk));

                        let row_pk = row_pk.to_bytes();
                        let row = row.data;
                        table_row_operations.push(TableOp {
                            op_type: 1, // Insert
                            row_pk,
                            row,
                        });
                    }
                }
            }
        }
        for (table_id, (table_name, ops)) in table_ops.into_iter().filter(|(_, (_, ops))| !ops.is_empty()) {
            database_update.tables.push(DatabaseTableUpdate {
                table_id,
                table_name,
                ops,
            });
        }
        Ok(database_update)
    }
}
