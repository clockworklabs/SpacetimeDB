use crate::db::relational_db::RelationalDBWrapper;

#[derive(Clone)]
pub struct Query {
    pub table_name: String,
}

pub fn compile_query(relational_db: &mut RelationalDBWrapper, input: &str) -> Result<Query, ()> {
    let mut stdb = relational_db.lock().unwrap();
    let mut tx_ = stdb.begin_tx();
    let (tx, stdb) = tx_.get();
    let tables = stdb.scan_table_names(tx).unwrap().collect::<Vec<_>>();
    tx_.rollback();

    // Check for the table name
    if !tables.iter().map(|(_, name)| name).any(|name| name == input) {
        return Err(());
    }

    Ok(Query {
        table_name: input.into(),
    })
}
