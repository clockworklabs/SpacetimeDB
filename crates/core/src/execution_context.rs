use derive_more::Display;
use spacetimedb_lib::Address;
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default)]
pub struct ExecutionContext<'a> {
    /// The database on which a transaction is being executed.
    database: Address,
    /// The reducer from which the current transaction originated.
    reducer: Option<&'a str>,
    /// The type of workload that is being executed.
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

impl_prometheusvalue_string!(WorkloadType);

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
            workload: WorkloadType::Reducer,
        }
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Sql,
        }
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Subscribe,
        }
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Update,
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Internal,
        }
    }

    /// Returns the address of the database on which we are operating.
    #[inline]
    pub fn database(&self) -> Address {
        self.database
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn reducer_name(&self) -> &str {
        self.reducer.unwrap_or_default()
    }

    /// Returns the type of workload that is being executed.
    #[inline]
    pub fn workload(&self) -> WorkloadType {
        self.workload
    }
}
