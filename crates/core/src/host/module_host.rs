use super::{ArgsTuple, InvalidReducerArguments, ReducerArgs, ReducerCallResult, ReducerId, Scheduler};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_logger::{LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::system_tables::{StClientFields, StClientRow, ST_CLIENT_ID};
use crate::db::datastore::traits::{IsolationLevel, Program, TxData};
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::execution_context::{ExecutionContext, ReducerContext, Workload, WorkloadType};
use crate::hash::Hash;
use crate::identity::Identity;
use crate::messages::control_db::Database;
use crate::replica_context::ReplicaContext;
use crate::sql::ast::SchemaViewer;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::subscription::tx::DeltaTx;
use crate::subscription::{execute_plan, record_exec_metrics};
use crate::util::lending_pool::{LendingPool, LentResource, PoolClosed};
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use anyhow::Context;
use bytes::Bytes;
use derive_more::From;
use futures::{Future, FutureExt};
use indexmap::IndexSet;
use itertools::Itertools;
use smallvec::SmallVec;
use spacetimedb_client_api_messages::websocket::{ByteListLen, Compression, OneOffTable, QueryUpdate, WebsocketFormat};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::{HashCollectionExt as _, IntMap};
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::ConnectionId;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::{col_list, TableId};
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::{algebraic_value, ProductValue};
use spacetimedb_schema::auto_migrate::AutoMigrateError;
use spacetimedb_schema::def::deserialize::ReducerArgsDeserializeSeed;
use spacetimedb_schema::def::{ModuleDef, ReducerDef};
use spacetimedb_vm::relation::RelValue;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, From)]
pub struct DatabaseUpdate {
    pub tables: Vec<DatabaseTableUpdate>,
}

impl FromIterator<DatabaseTableUpdate> for DatabaseUpdate {
    fn from_iter<T: IntoIterator<Item = DatabaseTableUpdate>>(iter: T) -> Self {
        DatabaseUpdate {
            tables: iter.into_iter().collect(),
        }
    }
}

impl DatabaseUpdate {
    pub fn is_empty(&self) -> bool {
        if self.tables.len() == 0 {
            return true;
        }
        false
    }

    pub fn from_writes(tx_data: &TxData) -> Self {
        let mut map: IntMap<TableId, DatabaseTableUpdate> = IntMap::new();
        let new_update = |table_id, table_name: &str| DatabaseTableUpdate {
            table_id,
            table_name: table_name.into(),
            inserts: [].into(),
            deletes: [].into(),
        };
        for (table_id, table_name, rows) in tx_data.inserts_with_table_name() {
            map.entry(*table_id)
                .or_insert_with(|| new_update(*table_id, table_name))
                .inserts = rows.clone();
        }
        for (table_id, table_name, rows) in tx_data.deletes_with_table_name() {
            map.entry(*table_id)
                .or_insert_with(|| new_update(*table_id, table_name))
                .deletes = rows.clone();
        }
        DatabaseUpdate {
            tables: map.into_values().collect(),
        }
    }

    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.tables.iter().map(|t| t.inserts.len() + t.deletes.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseTableUpdate {
    pub table_id: TableId,
    pub table_name: Box<str>,
    // Note: `Arc<[ProductValue]>` allows to cheaply
    // use the values from `TxData` without cloning the
    // contained `ProductValue`s.
    pub inserts: Arc<[ProductValue]>,
    pub deletes: Arc<[ProductValue]>,
}

#[derive(Debug)]
pub struct DatabaseUpdateRelValue<'a> {
    pub tables: Vec<DatabaseTableUpdateRelValue<'a>>,
}

#[derive(PartialEq, Debug)]
pub struct DatabaseTableUpdateRelValue<'a> {
    pub table_id: TableId,
    pub table_name: Box<str>,
    pub updates: UpdatesRelValue<'a>,
}

#[derive(Default, PartialEq, Debug)]
pub struct UpdatesRelValue<'a> {
    pub deletes: Vec<RelValue<'a>>,
    pub inserts: Vec<RelValue<'a>>,
}

impl UpdatesRelValue<'_> {
    /// Returns whether there are any updates.
    pub fn has_updates(&self) -> bool {
        !(self.deletes.is_empty() && self.inserts.is_empty())
    }

    pub fn encode<F: WebsocketFormat>(&self, compression: Compression) -> (F::QueryUpdate, u64, usize) {
        let (deletes, nr_del) = F::encode_list(self.deletes.iter());
        let (inserts, nr_ins) = F::encode_list(self.inserts.iter());
        let num_rows = nr_del + nr_ins;
        let num_bytes = deletes.num_bytes() + inserts.num_bytes();
        let qu = QueryUpdate { deletes, inserts };
        let cqu = F::into_query_update(qu, compression);
        (cqu, num_rows, num_bytes)
    }
}

#[derive(Debug, Clone)]
pub enum EventStatus {
    Committed(DatabaseUpdate),
    Failed(String),
    OutOfEnergy,
}

impl EventStatus {
    pub fn database_update(&self) -> Option<&DatabaseUpdate> {
        match self {
            EventStatus::Committed(upd) => Some(upd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleFunctionCall {
    pub reducer: String,
    pub reducer_id: ReducerId,
    pub args: ArgsTuple,
}

#[derive(Debug, Clone)]
pub struct ModuleEvent {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub caller_connection_id: Option<ConnectionId>,
    pub function_call: ModuleFunctionCall,
    pub status: EventStatus,
    pub energy_quanta_used: EnergyQuanta,
    pub host_execution_duration: Duration,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
}

/// Information about a running module.
#[derive(Debug)]
pub struct ModuleInfo {
    /// The definition of the module.
    /// Loaded by loading the module's program from the system tables, extracting its definition,
    /// and validating.
    pub module_def: ModuleDef,
    /// The identity of the module.
    pub owner_identity: Identity,
    /// The identity of the database.
    pub database_identity: Identity,
    /// The hash of the module.
    pub module_hash: Hash,
    /// Allows subscribing to module logs.
    pub log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
    /// Subscriptions to this module.
    pub subscriptions: ModuleSubscriptions,
}

impl ModuleInfo {
    /// Create a new `ModuleInfo`.
    /// Reducers are sorted alphabetically by name and assigned IDs.
    pub fn new(
        module_def: ModuleDef,
        owner_identity: Identity,
        database_identity: Identity,
        module_hash: Hash,
        log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
        subscriptions: ModuleSubscriptions,
    ) -> Arc<Self> {
        Arc::new(ModuleInfo {
            module_def,
            owner_identity,
            database_identity,
            module_hash,
            log_tx,
            subscriptions,
        })
    }
}

/// A bidirectional map between `Identifiers` (reducer names) and `ReducerId`s.
/// Invariant: the reducer names are in the same order as they were declared in the `ModuleDef`.
pub struct ReducersMap(IndexSet<Box<str>>);

impl<'a> FromIterator<&'a str> for ReducersMap {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        Self(iter.into_iter().map_into().collect())
    }
}

impl fmt::Debug for ReducersMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ReducersMap {
    /// Lookup the ID for a reducer name.
    pub fn lookup_id(&self, reducer_name: &str) -> Option<ReducerId> {
        self.0.get_index_of(reducer_name).map(ReducerId::from)
    }

    /// Lookup the name for a reducer ID.
    pub fn lookup_name(&self, reducer_id: ReducerId) -> Option<&str> {
        let result = self.0.get_index(reducer_id.0 as _)?;
        Some(&**result)
    }
}

pub trait Module: Send + Sync + 'static {
    type Instance: ModuleInstance;
    type InitialInstances<'a>: IntoIterator<Item = Self::Instance> + 'a;
    fn initial_instances(&mut self) -> Self::InitialInstances<'_>;
    fn info(&self) -> Arc<ModuleInfo>;
    fn create_instance(&self) -> Self::Instance;
    fn replica_ctx(&self) -> &ReplicaContext;
    fn scheduler(&self) -> &Scheduler;
}

pub trait ModuleInstance: Send + 'static {
    fn trapped(&self) -> bool;

    /// If the module instance's replica_ctx is uninitialized, initialize it.
    fn init_database(&mut self, program: Program) -> anyhow::Result<Option<ReducerCallResult>>;

    /// Update the module instance's database to match the schema of the module instance.
    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult>;

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult;
}

pub struct CallReducerParams {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub caller_connection_id: ConnectionId,
    pub client: Option<Arc<ClientConnectionSender>>,
    pub request_id: Option<RequestId>,
    pub timer: Option<Instant>,
    pub reducer_id: ReducerId,
    pub args: ArgsTuple,
}

// TODO: figure out how we want to handle traps. maybe it should just not return to the LendingPool and
//       let the get_instance logic handle it?
struct AutoReplacingModuleInstance<T: Module> {
    inst: LentResource<T::Instance>,
    module: Arc<T>,
}

impl<T: Module> AutoReplacingModuleInstance<T> {
    fn check_trap(&mut self) {
        if self.inst.trapped() {
            *self.inst = self.module.create_instance()
        }
    }
}

impl<T: Module> ModuleInstance for AutoReplacingModuleInstance<T> {
    fn trapped(&self) -> bool {
        self.inst.trapped()
    }
    fn init_database(&mut self, program: Program) -> anyhow::Result<Option<ReducerCallResult>> {
        let ret = self.inst.init_database(program);
        self.check_trap();
        ret
    }
    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let ret = self.inst.update_database(program, old_module_info);
        self.check_trap();
        ret
    }
    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        let ret = self.inst.call_reducer(tx, params);
        self.check_trap();
        ret
    }
}

#[derive(Clone)]
pub struct ModuleHost {
    pub info: Arc<ModuleInfo>,
    inner: Arc<dyn DynModuleHost>,
    /// Called whenever a reducer call on this host panics.
    on_panic: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl fmt::Debug for ModuleHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleHost")
            .field("info", &self.info)
            .field("inner", &Arc::as_ptr(&self.inner))
            .finish()
    }
}

#[async_trait::async_trait]
trait DynModuleHost: Send + Sync + 'static {
    async fn get_instance(&self, db: Identity) -> Result<Box<dyn ModuleInstance>, NoSuchModule>;
    fn replica_ctx(&self) -> &ReplicaContext;
    async fn exit(&self);
    async fn exited(&self);
}

struct HostControllerActor<T: Module> {
    module: Arc<T>,
    instance_pool: LendingPool<T::Instance>,
}

impl<T: Module> HostControllerActor<T> {
    fn spinup_new_instance(&self) {
        let (module, instance_pool) = (self.module.clone(), self.instance_pool.clone());
        rayon::spawn(move || {
            let instance = module.create_instance();
            match instance_pool.add(instance) {
                Ok(()) => {}
                Err(PoolClosed) => {
                    // if the module closed since this new instance was requested, oh well, just throw it away
                }
            }
        })
    }
}

/// runs future A and future B concurrently. if A completes before B, B is cancelled. if B completes
/// before A, A is polled to completion
async fn select_first<A: Future, B: Future<Output = ()>>(fut_a: A, fut_b: B) -> A::Output {
    tokio::select! {
        ret = fut_a => ret,
        Err(x) = fut_b.never_error() => match x {},
    }
}

#[async_trait::async_trait]
impl<T: Module> DynModuleHost for HostControllerActor<T> {
    async fn get_instance(&self, db: Identity) -> Result<Box<dyn ModuleInstance>, NoSuchModule> {
        // in the future we should do something like in the else branch here -- add more instances based on load.
        // we need to do write-skew retries first - right now there's only ever once instance per module.
        let inst = if true {
            self.instance_pool
                .request_with_context(db)
                .await
                .map_err(|_| NoSuchModule)?
        } else {
            const GET_INSTANCE_TIMEOUT: Duration = Duration::from_millis(500);
            select_first(
                self.instance_pool.request_with_context(db),
                tokio::time::sleep(GET_INSTANCE_TIMEOUT).map(|()| self.spinup_new_instance()),
            )
            .await
            .map_err(|_| NoSuchModule)?
        };
        Ok(Box::new(AutoReplacingModuleInstance {
            inst,
            module: self.module.clone(),
        }))
    }

    fn replica_ctx(&self) -> &ReplicaContext {
        self.module.replica_ctx()
    }

    async fn exit(&self) {
        self.module.scheduler().close();
        self.instance_pool.close();
        self.exited().await
    }

    async fn exited(&self) {
        tokio::join!(self.module.scheduler().closed(), self.instance_pool.closed());
    }
}

pub struct WeakModuleHost {
    info: Arc<ModuleInfo>,
    inner: Weak<dyn DynModuleHost>,
    on_panic: Weak<dyn Fn() + Send + Sync + 'static>,
}

#[derive(Debug)]
pub enum UpdateDatabaseResult {
    NoUpdateNeeded,
    UpdatePerformed,
    AutoMigrateError(ErrorStream<AutoMigrateError>),
    ErrorExecutingMigration(anyhow::Error),
}
impl UpdateDatabaseResult {
    /// Check if a database update was successful.
    pub fn was_successful(&self) -> bool {
        matches!(
            self,
            UpdateDatabaseResult::UpdatePerformed | UpdateDatabaseResult::NoUpdateNeeded
        )
    }
}

#[derive(thiserror::Error, Debug)]
#[error("no such module")]
pub struct NoSuchModule;

#[derive(thiserror::Error, Debug)]
pub enum ReducerCallError {
    #[error(transparent)]
    Args(#[from] InvalidReducerArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error("no such reducer")]
    NoSuchReducer,
    #[error("no such scheduled reducer")]
    ScheduleReducerNotFound,
    #[error("can't directly call special {0:?} lifecycle reducer")]
    LifecycleReducer(Lifecycle),
}

#[derive(thiserror::Error, Debug)]
pub enum InitDatabaseError {
    #[error(transparent)]
    Args(#[from] InvalidReducerArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error(transparent)]
    Other(anyhow::Error),
}

impl ModuleHost {
    pub fn new(mut module: impl Module, on_panic: impl Fn() + Send + Sync + 'static) -> Self {
        let info = module.info();
        let instance_pool = LendingPool::new();
        instance_pool.add_multiple(module.initial_instances()).unwrap();
        let inner = Arc::new(HostControllerActor {
            module: Arc::new(module),
            instance_pool,
        });
        let on_panic = Arc::new(on_panic);
        ModuleHost { info, inner, on_panic }
    }

    #[inline]
    pub fn info(&self) -> &ModuleInfo {
        &self.info
    }

    #[inline]
    pub fn subscriptions(&self) -> &ModuleSubscriptions {
        &self.info.subscriptions
    }

    async fn call<F, R>(&self, reducer: &str, f: F) -> Result<R, NoSuchModule>
    where
        F: FnOnce(&mut dyn ModuleInstance) -> R + Send + 'static,
        R: Send + 'static,
    {
        let mut inst = {
            // Record the time spent waiting in the queue
            let _guard = WORKER_METRICS
                .reducer_wait_time
                .with_label_values(&self.info.database_identity, reducer)
                .start_timer();
            self.inner.get_instance(self.info.database_identity).await?
        };

        let result = tokio::task::spawn_blocking(move || f(&mut *inst))
            .await
            .unwrap_or_else(|e| {
                log::warn!("reducer `{reducer}` panicked");
                (self.on_panic)();
                std::panic::resume_unwind(e.into_panic())
            });
        Ok(result)
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) {
        log::trace!("disconnecting client {}", client_id);
        let this = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            this.subscriptions().remove_subscriber(client_id);
        })
        .await;
        // ignore NoSuchModule; if the module's already closed, that's fine
        let _ = self
            .call_identity_connected_disconnected(client_id.identity, client_id.connection_id, false)
            .await;
    }

    /// Method is responsible for handling connect/disconnect events.
    ///
    /// It ensures pairing up those event in commitlogs
    /// Though It can also create two entries `__identity_disconnect__`.
    /// One is to actually run the reducer and another one to delete client from `st_clients`
    pub async fn call_identity_connected_disconnected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        connected: bool,
    ) -> Result<(), ReducerCallError> {
        let (lifecycle, fake_name) = if connected {
            (Lifecycle::OnConnect, "__identity_connected__")
        } else {
            (Lifecycle::OnDisconnect, "__identity_disconnected__")
        };

        let reducer_lookup = self.info.module_def.lifecycle_reducer(lifecycle);
        let reducer_name = reducer_lookup.as_ref().map(|(_, def)| &*def.name).unwrap_or(fake_name);

        let db = &self.inner.replica_ctx().relational_db;
        let workload = || {
            Workload::Reducer(ReducerContext {
                name: reducer_name.to_owned(),
                caller_identity,
                caller_connection_id,
                timestamp: Timestamp::now(),
                arg_bsatn: Bytes::new(),
            })
        };

        let result = if let Some((reducer_id, reducer_def)) = reducer_lookup {
            self.call_reducer_inner(
                caller_identity,
                Some(caller_connection_id),
                None,
                None,
                None,
                reducer_id,
                reducer_def,
                ReducerArgs::Nullary,
            )
            .await
            .map(drop)
        } else {
            // If the module doesn't define connected or disconnected, commit
            // a transaction to update `st_clients` and to ensure we always have those events
            // paired in the commitlog.
            //
            // This is necessary to be able to disconnect clients after a server
            // crash.
            db.with_auto_commit(workload(), |mut_tx| {
                if connected {
                    self.update_st_clients(mut_tx, caller_identity, caller_connection_id, connected)
                } else {
                    Ok(())
                }
            })
            .map_err(|err| {
                InvalidReducerArguments {
                    err: err.into(),
                    reducer: reducer_name.into(),
                }
                .into()
            })
        };

        // Deleting client from `st_clients`does not depend upon result of disconnect reducer hence done in a separate tx.
        if !connected {
            let _ = db
                .with_auto_commit(workload(), |mut_tx| {
                    self.update_st_clients(mut_tx, caller_identity, caller_connection_id, connected)
                })
                .map_err(|e| {
                    log::error!("st_clients table update failed with params with error: {:?}", e);
                });
        }
        result
    }

    fn update_st_clients(
        &self,
        mut_tx: &mut MutTxId,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        connected: bool,
    ) -> Result<(), DBError> {
        let db = &*self.inner.replica_ctx().relational_db;

        let row = &StClientRow {
            identity: caller_identity.into(),
            connection_id: caller_connection_id.into(),
        };

        if connected {
            mut_tx.insert_via_serialize_bsatn(ST_CLIENT_ID, &row).map(|_| ())
        } else {
            let row = db
                .iter_by_col_eq_mut(
                    mut_tx,
                    ST_CLIENT_ID,
                    col_list![StClientFields::Identity, StClientFields::ConnectionId],
                    &algebraic_value::AlgebraicValue::product(row),
                )?
                .map(|row_ref| row_ref.pointer())
                .collect::<SmallVec<[_; 1]>>();
            db.delete(mut_tx, ST_CLIENT_ID, row);
            Ok::<(), DBError>(())
        }
    }

    async fn call_reducer_inner(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_id: ReducerId,
        reducer_def: &ReducerDef,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let reducer_seed = ReducerArgsDeserializeSeed(self.info.module_def.typespace().with_type(reducer_def));
        let args = args.into_tuple(reducer_seed)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);

        self.call(&reducer_def.name, move |inst| {
            inst.call_reducer(
                None,
                CallReducerParams {
                    timestamp: Timestamp::now(),
                    caller_identity,
                    caller_connection_id,
                    client,
                    request_id,
                    timer,
                    reducer_id,
                    args,
                },
            )
        })
        .await
        .map_err(Into::into)
    }

    pub async fn call_reducer(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let res = async {
            let (reducer_id, reducer_def) = self
                .info
                .module_def
                .reducer_full(reducer_name)
                .ok_or(ReducerCallError::NoSuchReducer)?;
            if let Some(lifecycle) = reducer_def.lifecycle {
                return Err(ReducerCallError::LifecycleReducer(lifecycle));
            }
            self.call_reducer_inner(
                caller_identity,
                caller_connection_id,
                client,
                request_id,
                timer,
                reducer_id,
                reducer_def,
                args,
            )
            .await
        }
        .await;

        let log_message = match &res {
            Err(ReducerCallError::NoSuchReducer) => Some(format!(
                "External attempt to call nonexistent reducer \"{}\" failed. Have you run `spacetime generate` recently?",
                reducer_name
            )),
            Err(ReducerCallError::Args(_)) => Some(format!(
                "External attempt to call reducer \"{}\" failed, invalid arguments.\n\
                 This is likely due to a mismatched client schema, have you run `spacetime generate` recently?",
                reducer_name,
            )),
            _ => None,
        };
        if let Some(log_message) = log_message {
            self.inject_logs(LogLevel::Error, &log_message)
        }

        res
    }

    // Scheduled reducers require a different function here to call their reducer
    // because their reducer arguments are stored in the database and need to be fetched
    // within the same transaction as the reducer call.
    pub async fn call_scheduled_reducer(
        &self,
        call_reducer_params: impl FnOnce(&MutTxId) -> anyhow::Result<Option<CallReducerParams>> + Send + 'static,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let db = self.inner.replica_ctx().relational_db.clone();
        // scheduled reducer name not fetched yet, anyway this is only for logging purpose
        const REDUCER: &str = "scheduled_reducer";
        let module = self.info.clone();
        self.call(REDUCER, move |inst: &mut dyn ModuleInstance| {
            let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);

            match call_reducer_params(&mut tx) {
                Ok(Some(params)) => {
                    // Is necessary to patch the context with the actual calling reducer
                    let reducer_def = module
                        .module_def
                        .get_reducer_by_id(params.reducer_id)
                        .ok_or(ReducerCallError::ScheduleReducerNotFound)?;
                    let reducer = &*reducer_def.name;

                    tx.ctx = ExecutionContext::with_workload(
                        tx.ctx.database_identity(),
                        Workload::Reducer(ReducerContext {
                            name: reducer.into(),
                            caller_identity: params.caller_identity,
                            caller_connection_id: params.caller_connection_id,
                            timestamp: Timestamp::now(),
                            arg_bsatn: params.args.get_bsatn().clone(),
                        }),
                    );

                    Ok(inst.call_reducer(Some(tx), params))
                }
                Ok(None) => Err(ReducerCallError::ScheduleReducerNotFound),
                Err(err) => Err(ReducerCallError::Args(InvalidReducerArguments {
                    err,
                    reducer: REDUCER.into(),
                })),
            }
        })
        .await
        .unwrap_or_else(|e| Err(e.into()))
        .map_err(Into::into)
    }

    pub fn subscribe_to_logs(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.info().log_tx.subscribe())
    }

    pub async fn init_database(&self, program: Program) -> Result<Option<ReducerCallResult>, InitDatabaseError> {
        self.call("<init_database>", move |inst| inst.init_database(program))
            .await?
            .map_err(InitDatabaseError::Other)
    }

    pub async fn update_database(
        &self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.call("<update_database>", move |inst| {
            inst.update_database(program, old_module_info)
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn exit(&self) {
        self.inner.exit().await
    }

    pub async fn exited(&self) {
        self.inner.exited().await
    }

    pub fn inject_logs(&self, log_level: LogLevel, message: &str) {
        self.replica_ctx().logger.write(
            log_level,
            &Record {
                ts: chrono::Utc::now(),
                target: None,
                filename: Some("external"),
                line_number: None,
                message,
            },
            &(),
        )
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn one_off_query<F: WebsocketFormat>(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<OneOffTable<F>, anyhow::Error> {
        let replica_ctx = self.replica_ctx();
        let db = &replica_ctx.relational_db;
        let auth = AuthCtx::new(replica_ctx.owner_identity, caller_identity);
        log::debug!("One-off query: {query}");

        let (rows, metrics) = db.with_read_only(Workload::Sql, |tx| {
            let tx = SchemaViewer::new(tx, &auth);
            let (plan, _, table_name, _) = compile_subscription(&query, &tx, &auth)?;
            let plan = plan.optimize()?;
            check_row_limit(&plan, db, &tx, |plan, tx| estimate_rows_scanned(tx, plan), &auth)?;
            execute_plan::<_, F>(&plan.into(), &DeltaTx::from(&*tx))
                .map(|(rows, _, metrics)| (OneOffTable { table_name, rows }, metrics))
                .context("One-off queries are not allowed to modify the database")
        })?;

        record_exec_metrics(&WorkloadType::Sql, &db.database_identity(), metrics);

        Ok(rows)
    }

    /// FIXME(jgilles): this is a temporary workaround for deleting not currently being supported
    /// for tables without primary keys. It is only used in the benchmarks.
    /// Note: this doesn't drop the table, it just clears it!
    pub fn clear_table(&self, table_name: &str) -> Result<(), anyhow::Error> {
        let db = &*self.replica_ctx().relational_db;

        db.with_auto_commit(Workload::Internal, |tx| {
            let tables = db.get_all_tables_mut(tx)?;
            // We currently have unique table names,
            // so we can assume there's only one table to clear.
            if let Some(table_id) = tables
                .iter()
                .find_map(|t| (&*t.table_name == table_name).then_some(t.table_id))
            {
                db.clear_table(tx, table_id)?;
            }
            Ok(())
        })
    }

    pub fn downgrade(&self) -> WeakModuleHost {
        WeakModuleHost {
            info: self.info.clone(),
            inner: Arc::downgrade(&self.inner),
            on_panic: Arc::downgrade(&self.on_panic),
        }
    }

    pub fn database_info(&self) -> &Database {
        &self.replica_ctx().database
    }

    pub(crate) fn replica_ctx(&self) -> &ReplicaContext {
        self.inner.replica_ctx()
    }
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        let inner = self.inner.upgrade()?;
        let on_panic = self.on_panic.upgrade()?;
        Some(ModuleHost {
            info: self.info.clone(),
            inner,
            on_panic,
        })
    }
}
