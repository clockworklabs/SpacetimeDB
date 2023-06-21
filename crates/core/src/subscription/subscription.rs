use spacetimedb_sats::{AlgebraicValue, BuiltinValue};
use std::collections::HashSet;

use super::query::Query;
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

impl QuerySet {
    /// Incremental evaluation of `rows` that matched the [Query] (aka subscriptions)
    ///
    /// This is equivalent to run a `trigger` on `INSERT/UPDATE/DELETE`, run the [Query] and see if the `row` is matched.
    ///
    /// NOTE: The returned `rows` in [DatabaseUpdate] are **deduplicated** so if 2 queries match the same `row`, only one copy is returned.
    pub fn eval_incr(
        &self,
        relational_db: &RelationalDB,
        database_update: &DatabaseUpdate,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };
        let mut seen = HashSet::new();

        for query in &self.0 {
            for table in database_update.tables.iter().cloned() {
                for q in query.queries_of_table_id(&table) {
                    if let Some(result) = run_query(relational_db, &q)?.into_iter().find(|x| !x.data.is_empty()) {
                        let mut table_row_operations = table.clone();
                        table_row_operations.ops.clear();
                        for mut row in result.data {
                            //Hack: remove the hidden field OP_TYPE_FIELD_NAME. see `to_mem_table`
                            // needs to be done before calculate the PK
                            let op_type =
                                if let Some(AlgebraicValue::Builtin(BuiltinValue::U8(op))) = row.elements.pop() {
                                    op
                                } else {
                                    panic!("Fail to extract {OP_TYPE_FIELD_NAME}")
                                };

                            let row_pk = RelationalDB::pk_for_row(&row);

                            //Skip rows that are already resolved in a previous subscription...
                            if seen.contains(&(table.table_id, row_pk)) {
                                continue;
                            }

                            seen.insert((table.table_id, row_pk));

                            let row_pk = row_pk.to_bytes();
                            table_row_operations.ops.push(TableOp { op_type, row_pk, row });
                        }
                        output.tables.push(table_row_operations);
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
    pub fn eval(&self, relational_db: &RelationalDB) -> Result<DatabaseUpdate, DBError> {
        let mut database_update = DatabaseUpdate { tables: vec![] };
        let mut seen = HashSet::new();

        for query in &self.0 {
            for q in &query.queries {
                if let Some(t) = q.source.get_db_table() {
                    for table in run_query(relational_db, q)? {
                        {
                            let mut table_row_operations = Vec::new();

                            for row in table.data {
                                let row_pk = RelationalDB::pk_for_row(&row);

                                //Skip rows that are already resolved in a previous subscription...
                                if seen.contains(&(t.table_id, row_pk)) {
                                    continue;
                                }
                                seen.insert((t.table_id, row_pk));

                                let row_pk = row_pk.to_bytes();
                                table_row_operations.push(TableOp {
                                    op_type: 1, // Insert
                                    row_pk,
                                    row,
                                });
                            }

                            // if table_row_operations.is_empty() {
                            //     continue;
                            // }

                            database_update.tables.push(DatabaseTableUpdate {
                                table_id: t.table_id,
                                table_name: t.head.table_name.clone(),
                                ops: table_row_operations,
                            });
                        }
                    }
                }
            }
        }
        Ok(database_update)
    }
}
