use derive_more::Display;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default)]
pub struct ExecutionContext {
    // The database on which a transaction is being executed.
    database_id: u64,
    // The reducer from which the current transaction originated.
    reducer_id: Option<u64>,
    // The type of transaction that is being executed.
    txn_type: TransactionType,
}

/// Classifies a transaction according to where it originates.
/// A transaction can be executing a reducer.
/// It can be used to satisfy a one-off sql query or subscription.
/// It can also be an internal operation that is not associated with a reducer or sql request.
#[derive(Clone, Copy, Display)]
pub enum TransactionType {
    Reducer,
    Sql,
    Internal,
}

impl Default for TransactionType {
    fn default() -> Self {
        Self::Internal
    }
}

impl ExecutionContext {
    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database_id: u64, reducer_id: u64) -> Self {
        Self {
            database_id,
            reducer_id: Some(reducer_id),
            txn_type: TransactionType::Reducer,
        }
    }

    /// Returns an [ExecutionContext] for a sql or subscription transaction.
    pub fn sql(database_id: u64) -> Self {
        Self {
            database_id,
            reducer_id: None,
            txn_type: TransactionType::Sql,
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database_id: u64) -> Self {
        Self {
            database_id,
            reducer_id: None,
            txn_type: TransactionType::Internal,
        }
    }

    /// Returns the id of the database on which we are operating.
    #[inline]
    pub fn database_id(&self) -> u64 {
        self.database_id
    }

    /// Returns the id of the reducer that is being executed.
    /// Returns [None] if this is not a reducer context.
    #[inline]
    pub fn reducer_id(&self) -> Option<u64> {
        self.reducer_id
    }

    /// Returns the type of transaction that is being executed.
    #[inline]
    pub fn txn_type(&self) -> TransactionType {
        self.txn_type
    }
}
