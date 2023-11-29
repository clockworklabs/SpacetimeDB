use derive_more::Display;
use spacetimedb_lib::Address;

use crate::sql::query_debug_info::QueryDebugInfo;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default)]
pub struct ExecutionContext<'a> {
    /// The database on which a transaction is being executed.
    database: Address,
    /// The reducer from which the current transaction originated.
    /// Note: this will never be set at the same time as `query`.
    reducer: Option<&'a str>,
    /// The SQL query being executed, if any.
    /// Note: this will never be set at the same time as `reducer`.
    /// It is also NOT guaranteed to be set, even if workload == Sql.
    /// This is because some transactions tagged "SQL" don't exactly correspond
    /// to any particular query.
    query_debug_info: Option<&'a QueryDebugInfo>,
    // The type of workload that is being executed.
    workload: WorkloadType,
}

/// Classifies a transaction according to its workload.
/// A transaction can be executing a reducer.
/// It can be used to satisfy a one-off sql query or subscription.
/// It can also be an internal operation that is not associated with a reducer or sql request.
#[derive(Clone, Copy, Display, Hash, PartialEq, Eq)]
pub enum WorkloadType {
    Reducer,
    Sql,
    Subscribe,
    Update,
    Internal,
}

impl Default for WorkloadType {
    fn default() -> Self {
        Self::Internal
    }
}

impl<'a> ExecutionContext<'a> {
    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database: Address, name: &'a str) -> Self {
        Self {
            database,
            reducer: Some(name),
            query_debug_info: None,
            workload: WorkloadType::Reducer,
        }
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address, query_debug_info: Option<&'a QueryDebugInfo>) -> Self {
        Self {
            database,
            reducer: None,
            query_debug_info,
            workload: WorkloadType::Sql,
        }
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address, query_debug_info: Option<&'a QueryDebugInfo>) -> Self {
        Self {
            database,
            reducer: None,
            query_debug_info,
            workload: WorkloadType::Subscribe,
        }
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address, query_debug_info: Option<&'a QueryDebugInfo>) -> Self {
        Self {
            database,
            reducer: None,
            query_debug_info,
            workload: WorkloadType::Update,
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            query_debug_info: None,
            workload: WorkloadType::Internal,
        }
    }

    /// Returns the address of the database on which we are operating.
    #[inline]
    pub fn database(&self) -> Address {
        self.database
    }

    /// Returns the name of the reducer that is being executed.
    /// Returns [None] if this is not a reducer context.
    #[inline]
    pub fn reducer_name(&self) -> Option<&str> {
        self.reducer
    }

    /// Returns the debug info for the query being executed.
    /// Returns [None] if this is not a sql context.
    #[inline]
    pub fn query_debug_info(&self) -> Option<&QueryDebugInfo> {
        self.query_debug_info
    }

    /// If this is a reducer context, returns the name of the reducer.
    /// If this is a query context, returns the query string.
    #[inline]
    pub fn reducer_or_query(&self) -> &str {
        self.reducer
            .unwrap_or_else(|| self.query_debug_info.map(|info| info.source()).unwrap_or_default())
    }

    /// Returns the type of workload that is being executed.
    #[inline]
    pub fn workload(&self) -> WorkloadType {
        self.workload
    }
}
