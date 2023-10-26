use derive_more::Display;
use spacetimedb_lib::Address;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default)]
pub struct ExecutionContext<'a> {
    // The database on which a transaction is being executed.
    database: Address,
    // The reducer from which the current transaction originated.
    reducer: Option<&'a str>,
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

impl<'a> ExecutionContext<'a> {
    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database: Address, name: &'a str) -> Self {
        Self {
            database,
            reducer: Some(name),
            txn_type: TransactionType::Reducer,
        }
    }

    /// Returns an [ExecutionContext] for a sql or subscription transaction.
    pub fn sql(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            txn_type: TransactionType::Sql,
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            txn_type: TransactionType::Internal,
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

    /// Returns the type of transaction that is being executed.
    #[inline]
    pub fn txn_type(&self) -> TransactionType {
        self.txn_type
    }
}
