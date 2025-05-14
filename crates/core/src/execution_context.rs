use std::sync::Arc;

use bytes::Bytes;
use derive_more::Display;
use spacetimedb_commitlog::{payload::txdata, Varchar};
use spacetimedb_lib::{ConnectionId, Identity, Timestamp};
use spacetimedb_sats::bsatn;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Clone)]
pub struct ExecutionContext {
    /// The identity of the database on which a transaction is being executed.
    pub database_identity: Identity,
    /// The reducer from which the current transaction originated.
    pub reducer: Option<ReducerContext>,
    /// The type of workload that is being executed.
    pub workload: WorkloadType,
}

/// If an [`ExecutionContext`] is a reducer context, describes the reducer.
///
/// Note that this information is written to persistent storage.
#[derive(Clone)]
pub struct ReducerContext {
    /// The name of the reducer.
    pub name: String,
    /// The [`Identity`] of the caller.
    pub caller_identity: Identity,
    /// The [`ConnectionId`] of the caller.
    pub caller_connection_id: ConnectionId,
    /// The timestamp of the reducer invocation.
    pub timestamp: Timestamp,
    /// The BSATN-encoded arguments given to the reducer.
    ///
    /// Note that [`Bytes`] is a refcounted value, but the memory it points to
    /// can be large-ish. The reference should be freed as soon as possible.
    pub arg_bsatn: Bytes,
}

impl From<&ReducerContext> for txdata::Inputs {
    fn from(
        ReducerContext {
            name,
            caller_identity,
            caller_connection_id,
            timestamp,
            arg_bsatn,
        }: &ReducerContext,
    ) -> Self {
        let reducer_name = Arc::new(Varchar::from_str_truncate(name));
        let cap = arg_bsatn.len()
        /* caller_identity */
        + 32
        /* caller_connection_id */
        + 16
        /* timestamp */
        + 8;
        let mut buf = Vec::with_capacity(cap);
        bsatn::to_writer(&mut buf, caller_identity).unwrap();
        bsatn::to_writer(&mut buf, caller_connection_id).unwrap();
        bsatn::to_writer(&mut buf, timestamp).unwrap();
        buf.extend_from_slice(arg_bsatn);

        txdata::Inputs {
            reducer_name,
            reducer_args: buf.into(),
        }
    }
}

impl TryFrom<&txdata::Inputs> for ReducerContext {
    type Error = bsatn::DecodeError;

    fn try_from(inputs: &txdata::Inputs) -> Result<Self, Self::Error> {
        let args = &mut inputs.reducer_args.as_ref();
        let caller_identity = bsatn::from_reader(args)?;
        let caller_connection_id = bsatn::from_reader(args)?;
        let timestamp = bsatn::from_reader(args)?;

        Ok(Self {
            name: inputs.reducer_name.to_string(),
            caller_identity,
            caller_connection_id,
            timestamp,
            arg_bsatn: Bytes::from(args.to_owned()),
        })
    }
}

/// Represents the type of workload that is being executed.
///
/// Used as constructor helper for [ExecutionContext].
#[derive(Clone)]
pub enum Workload {
    #[cfg(any(test, feature = "test"))]
    ForTests,
    Reducer(ReducerContext),
    Sql,
    Subscribe,
    Unsubscribe,
    Update,
    Internal,
}

impl Workload {
    pub fn workload_type(&self) -> WorkloadType {
        match self {
            #[cfg(any(test, feature = "test"))]
            Self::ForTests => WorkloadType::Internal,
            Self::Reducer(_) => WorkloadType::Reducer,
            Self::Sql => WorkloadType::Sql,
            Self::Subscribe => WorkloadType::Subscribe,
            Self::Unsubscribe => WorkloadType::Unsubscribe,
            Self::Update => WorkloadType::Update,
            Self::Internal => WorkloadType::Internal,
        }
    }
}

/// Classifies a transaction according to its workload.
/// A transaction can be executing a reducer.
/// It can be used to satisfy a one-off sql query or subscription.
/// It can also be an internal operation that is not associated with a reducer or sql request.
#[derive(Clone, Copy, Display, Hash, PartialEq, Eq, strum::AsRefStr, enum_map::Enum)]
pub enum WorkloadType {
    Reducer,
    Sql,
    Subscribe,
    Unsubscribe,
    Update,
    Internal,
}

impl Default for WorkloadType {
    fn default() -> Self {
        Self::Internal
    }
}

impl ExecutionContext {
    /// Returns an [ExecutionContext] with the provided parameters and empty metrics.
    fn new(database_identity: Identity, reducer: Option<ReducerContext>, workload: WorkloadType) -> Self {
        Self {
            database_identity,
            reducer,
            workload,
        }
    }

    /// Returns an [ExecutionContext] with the provided [Workload] and empty metrics.
    pub(crate) fn with_workload(database: Identity, workload: Workload) -> Self {
        match workload {
            #[cfg(any(test, feature = "test"))]
            Workload::ForTests => Self::new(database, None, WorkloadType::Internal),
            Workload::Internal => Self::new(database, None, WorkloadType::Internal),
            Workload::Reducer(ctx) => Self::new(database, Some(ctx), WorkloadType::Reducer),
            Workload::Sql => Self::new(database, None, WorkloadType::Sql),
            Workload::Subscribe => Self::new(database, None, WorkloadType::Subscribe),
            Workload::Unsubscribe => Self::new(database, None, WorkloadType::Unsubscribe),
            Workload::Update => Self::new(database, None, WorkloadType::Update),
        }
    }

    /// Returns the identity of the database on which we are operating.
    #[inline]
    pub fn database_identity(&self) -> Identity {
        self.database_identity
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn reducer_name(&self) -> &str {
        self.reducer.as_ref().map(|ctx| ctx.name.as_str()).unwrap_or_default()
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn into_reducer_name(self) -> String {
        self.reducer.map(|ctx| ctx.name).unwrap_or_default()
    }

    /// If this is a reducer context, returns the full reducer metadata.
    #[inline]
    pub fn reducer_context(&self) -> Option<&ReducerContext> {
        self.reducer.as_ref()
    }

    /// Returns the type of workload that is being executed.
    #[inline]
    pub fn workload(&self) -> WorkloadType {
        self.workload
    }
}
