use std::sync::Arc;

use bytes::Bytes;
use derive_more::Display;
use spacetimedb_client_api_messages::timestamp::Timestamp;
use spacetimedb_commitlog::{payload::txdata, Varchar};
use spacetimedb_lib::{Address, Identity};
use spacetimedb_sats::bsatn;

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default, Clone)]
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
    /// The [`Address`] of the caller.
    pub caller_address: Address,
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
            caller_address,
            timestamp,
            arg_bsatn,
        }: &ReducerContext,
    ) -> Self {
        let reducer_name = Arc::new(Varchar::from_str_truncate(name));
        let cap = arg_bsatn.len()
        /* caller_identity */
        + 32
        /* caller_address */
        + 16
        /* timestamp */
        + 8;
        let mut buf = Vec::with_capacity(cap);
        bsatn::to_writer(&mut buf, caller_identity).unwrap();
        bsatn::to_writer(&mut buf, caller_address).unwrap();
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
        let caller_address = bsatn::from_reader(args)?;
        let timestamp = bsatn::from_reader(args)?;

        Ok(Self {
            name: inputs.reducer_name.to_string(),
            caller_identity,
            caller_address,
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
    #[cfg(test)]
    ForTests,
    Reducer(ReducerContext),
    Sql,
    Subscribe,
    Update,
    Internal,
}

/// Classifies a transaction according to its workload.
/// A transaction can be executing a reducer.
/// It can be used to satisfy a one-off sql query or subscription.
/// It can also be an internal operation that is not associated with a reducer or sql request.
#[derive(Clone, Copy, Display, Hash, PartialEq, Eq, strum::AsRefStr)]
pub enum WorkloadType {
    Reducer,
    Sql,
    Subscribe,
    Update,
    Internal,
}

impl From<Workload> for WorkloadType {
    fn from(value: Workload) -> Self {
        match value {
            #[cfg(test)]
            Workload::ForTests => Self::Internal,
            Workload::Reducer(_) => Self::Reducer,
            Workload::Sql => Self::Sql,
            Workload::Subscribe => Self::Subscribe,
            Workload::Update => Self::Update,
            Workload::Internal => Self::Internal,
        }
    }
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
    pub(crate) fn with_workload(database_identity: Identity, workload: Workload) -> Self {
        match workload {
            #[cfg(test)]
            Workload::ForTests => Self::default(),
            Workload::Internal => Self::internal(database_identity),
            Workload::Reducer(ctx) => Self::reducer(database_identity, ctx),
            Workload::Sql => Self::sql(database_identity),
            Workload::Subscribe => Self::subscribe(database_identity),
            Workload::Update => Self::incremental_update(database_identity),
        }
    }

    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database_identity: Identity, ctx: ReducerContext) -> Self {
        Self::new(database_identity, Some(ctx), WorkloadType::Reducer)
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database_identity: Identity) -> Self {
        Self::new(database_identity, None, WorkloadType::Sql)
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Identity) -> Self {
        Self::new(database, None, WorkloadType::Subscribe)
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Identity) -> Self {
        Self::new(database, None, WorkloadType::Update)
    }

    /// Returns an [ExecutionContext] for an incremental subscription update,
    /// where this update is the result of a reducer mutation.
    pub fn incremental_update_for_reducer(database: Identity, ctx: ReducerContext) -> Self {
        Self::new(database, Some(ctx), WorkloadType::Update)
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database_identity: Identity) -> Self {
        Self::new(database_identity, None, WorkloadType::Internal)
    }

    /// Returns the address of the database on which we are operating.
    #[inline]
    pub fn database_identity(&self) -> Identity {
        self.database_identity
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn reducer_name(&self) -> &str {
        self.reducer.as_ref().map(|ctx| ctx.name.as_str()).unwrap_or_default()
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
