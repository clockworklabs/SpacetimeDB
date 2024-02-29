use crate::database_instance_context::DatabaseInstanceContext;
use crate::db::relational_db::RelationalDB;
use spacetimedb_sats::energy::QueryTimer;

pub struct QueryContext<'a> {
    pub(crate) database_instance_context: &'a DatabaseInstanceContext,
    pub(crate) timer: QueryTimer,
}

impl<'a> QueryContext<'a> {
    pub fn new(database_instance_context: &'a DatabaseInstanceContext) -> Self {
        Self {
            database_instance_context,
            timer: QueryTimer::default(),
        }
    }

    pub fn for_testing() -> Self {
        //Self::new(&DatabaseInstanceContext::for_testing())
        todo!()
    }

    pub fn db(&self) -> &RelationalDB {
        &self.database_instance_context.relational_db
    }
}
