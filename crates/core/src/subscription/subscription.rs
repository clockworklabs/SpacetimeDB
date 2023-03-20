use super::query::Query;
use crate::error::DBError;
use crate::subscription::query::run_query;
use crate::{
    client::{client_connection::Protocol, client_connection_index::ClientConnectionSender, ClientActorId},
    db::relational_db::{RelationalDB, RelationalDBWrapper},
    host::module_host::{DatabaseTableUpdate, DatabaseUpdate, TableOp},
};

#[derive(Clone)]
pub struct Subscriber {
    pub sender: ClientConnectionSender,
    pub protocol: Protocol,
}

pub struct Subscription {
    pub queries: Vec<Query>,
    pub subscribers: Vec<Subscriber>,
}

impl PartialEq for Subscription {
    fn eq(&self, other: &Self) -> bool {
        let mut a = self.queries.clone();
        let mut b = other.queries.clone();
        a.sort();
        b.sort();
        a == b
    }
}

impl PartialEq<Subscription> for &mut Subscription {
    fn eq(&self, other: &Subscription) -> bool {
        let mut a = self.queries.clone();
        let mut b = other.queries.clone();
        a.sort();
        b.sort();
        a == b
    }
}

impl Subscription {
    pub fn remove_subscriber(&mut self, client_id: ClientActorId) -> Option<Subscriber> {
        let mut i = 0;
        while i < self.subscribers.len() {
            let subscriber = &self.subscribers[i];
            if subscriber.sender.id == client_id {
                return Some(self.subscribers.swap_remove(i));
            } else {
                i += 1;
            }
        }
        None
    }

    pub fn add_subscriber(&mut self, sender: ClientConnectionSender, protocol: Protocol) {
        if !self.subscribers.iter().any(|s| s.sender.id == sender.id) {
            self.subscribers.push(Subscriber { sender, protocol });
        }
    }

    pub fn eval_incr_query(
        &mut self,
        relational_db: &mut RelationalDBWrapper,
        database_update: &DatabaseUpdate,
    ) -> Result<DatabaseUpdate, DBError> {
        let mut output = DatabaseUpdate { tables: vec![] };

        for query in &self.queries {
            for table in &database_update.tables {
                if table.table_name == query.table.name {
                    let result = run_query(relational_db.clone(), &query.table, table)?;
                    if result.data.is_empty() {
                        continue;
                    }
                    output.tables.push(table.clone());
                }
            }
        }

        Ok(output)
    }

    pub fn eval_query(&mut self, relational_db: &mut RelationalDBWrapper) -> DatabaseUpdate {
        let mut database_update = DatabaseUpdate { tables: vec![] };

        let mut stdb = relational_db.lock().unwrap();
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();
        let tables = stdb.scan_table_names().collect::<Vec<_>>();

        for query in &self.queries {
            let table_name = &query.table.name;
            let mut table_id: i32 = -1;
            for (t_id, t_name) in &tables {
                if table_name == t_name.as_str() {
                    table_id = *t_id as i32;
                }
            }

            if table_id == -1 {
                panic!("This is not supposed to happen.");
            }

            let table_id = table_id as u32;

            let mut table_row_operations = Vec::new();
            for row in stdb.scan(tx, table_id).unwrap() {
                let row_pk = RelationalDB::pk_for_row(&row);
                let row_pk = row_pk.to_bytes();
                table_row_operations.push(TableOp {
                    op_type: 1, // Insert
                    row_pk,
                    row,
                });
            }
            database_update.tables.push(DatabaseTableUpdate {
                table_id,
                table_name: table_name.clone(),
                ops: table_row_operations,
            });
        }

        tx_.rollback();

        database_update
    }
}
