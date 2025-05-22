use super::{ArgsTuple, InvalidReducerArguments, ReducerArgs, ReducerCallResult, ReducerId, ReducerOutcome, Scheduler};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_logger::{LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
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
use crate::sql::parser::RowLevelExpr;
use crate::subscription::execute_plan;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::subscription::tx::DeltaTx;
use crate::util::asyncify;
use crate::util::lending_pool::{LendingPool, LentResource, PoolClosed};
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use anyhow::Context;
use bytes::Bytes;
use derive_more::From;
use futures::{Future, FutureExt};
use indexmap::IndexSet;
use itertools::Itertools;
use prometheus::{Histogram, IntGauge};
use spacetimedb_client_api_messages::websocket::{ByteListLen, Compression, OneOffTable, QueryUpdate, WebsocketFormat};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::{HashCollectionExt as _, IntMap};
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::ConnectionId;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::TableId;
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::ProductValue;
use spacetimedb_schema::auto_migrate::AutoMigrateError;
use spacetimedb_schema::def::deserialize::ReducerArgsDeserializeSeed;
use spacetimedb_schema::def::{ModuleDef, ReducerDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
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

    pub fn encode<F: WebsocketFormat>(&self) -> (F::QueryUpdate, u64, usize) {
        let (deletes, nr_del) = F::encode_list(self.deletes.iter());
        let (inserts, nr_ins) = F::encode_list(self.inserts.iter());
        let num_rows = nr_del + nr_ins;
        let num_bytes = deletes.num_bytes() + inserts.num_bytes();
        let qu = QueryUpdate { deletes, inserts };
        // We don't compress individual table updates.
        // Previously we were, but the benefits, if any, were unclear.
        // Note, each message is still compressed before being sent to clients,
        // but we no longer have to hold a tx lock when doing so.
        let cqu = F::into_query_update(qu, Compression::None);
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
    /// Metrics handles for this module.
    pub metrics: ModuleMetrics,
}

#[derive(Debug)]
pub struct ModuleMetrics {
    pub connected_clients: IntGauge,
    pub ws_clients_spawned: IntGauge,
    pub ws_clients_aborted: IntGauge,
    pub request_round_trip_subscribe: Histogram,
    pub request_round_trip_unsubscribe: Histogram,
    pub request_round_trip_sql: Histogram,
}

impl ModuleMetrics {
    fn new(db: &Identity) -> Self {
        let connected_clients = WORKER_METRICS.connected_clients.with_label_values(db);
        let ws_clients_spawned = WORKER_METRICS.ws_clients_spawned.with_label_values(db);
        let ws_clients_aborted = WORKER_METRICS.ws_clients_aborted.with_label_values(db);
        let request_round_trip_subscribe =
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Subscribe, db, "");
        let request_round_trip_unsubscribe =
            WORKER_METRICS
                .request_round_trip
                .with_label_values(&WorkloadType::Unsubscribe, db, "");
        let request_round_trip_sql = WORKER_METRICS
            .request_round_trip
            .with_label_values(&WorkloadType::Sql, db, "");
        Self {
            connected_clients,
            ws_clients_spawned,
            ws_clients_aborted,
            request_round_trip_subscribe,
            request_round_trip_unsubscribe,
            request_round_trip_sql,
        }
    }
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
        let metrics = ModuleMetrics::new(&database_identity);
        Arc::new(ModuleInfo {
            module_def,
            owner_identity,
            database_identity,
            module_hash,
            log_tx,
            subscriptions,
            metrics,
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

    /// Update the module instance's database to match the schema of the module instance.
    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
    ) -> anyhow::Result<UpdateDatabaseResult>;

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult;
}

/// If the module instance's replica_ctx is uninitialized, initialize it.
fn init_database(
    replica_ctx: &ReplicaContext,
    module_def: &ModuleDef,
    inst: &mut dyn ModuleInstance,
    program: Program,
) -> anyhow::Result<Option<ReducerCallResult>> {
    log::debug!("init database");
    let timestamp = Timestamp::now();
    let stdb = &*replica_ctx.relational_db;
    let logger = replica_ctx.logger.system_logger();

    let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    let auth_ctx = AuthCtx::for_current(replica_ctx.database.owner_identity);
    let (tx, ()) = stdb
        .with_auto_rollback(tx, |tx| {
            let mut table_defs: Vec<_> = module_def.tables().collect();
            table_defs.sort_by(|a, b| a.name.cmp(&b.name));

            for def in table_defs {
                let table_name = &def.name;
                logger.info(&format!("Creating table `{table_name}`"));
                let schema = TableSchema::from_module_def(module_def, def, (), TableId::SENTINEL);
                stdb.create_table(tx, schema)
                    .with_context(|| format!("failed to create table {table_name}"))?;
            }
            // Insert the late-bound row-level security expressions.
            for rls in module_def.row_level_security() {
                logger.info(&format!("Creating row level security `{}`", rls.sql));

                let rls = RowLevelExpr::build_row_level_expr(tx, &auth_ctx, rls)
                    .with_context(|| format!("failed to create row-level security: `{}`", rls.sql))?;
                let table_id = rls.def.table_id;
                let sql = rls.def.sql.clone();
                stdb.create_row_level_security(tx, rls.def)
                    .with_context(|| format!("failed to create row-level security for table `{table_id}`: `{sql}`",))?;
            }

            stdb.set_initialized(tx, replica_ctx.host_type, program)?;

            anyhow::Ok(())
        })
        .inspect_err(|e| log::error!("{e:?}"))?;

    let rcr = match module_def.lifecycle_reducer(Lifecycle::Init) {
        None => {
            if let Some((tx_data, tx_metrics, reducer)) = stdb.commit_tx(tx)? {
                tx_metrics.report_with_db(&reducer, stdb, Some(&tx_data));
            }
            None
        }

        Some((reducer_id, _)) => {
            logger.info("Invoking `init` reducer");
            let caller_identity = replica_ctx.database.owner_identity;
            Some(inst.call_reducer(
                Some(tx),
                CallReducerParams {
                    timestamp,
                    caller_identity,
                    caller_connection_id: ConnectionId::ZERO,
                    client: None,
                    request_id: None,
                    timer: None,
                    reducer_id,
                    args: ArgsTuple::nullary(),
                },
            ))
        }
    };

    logger.info("Database initialized");
    Ok(rcr)
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

#[derive(thiserror::Error, Debug)]
pub enum ClientConnectedError {
    #[error(transparent)]
    ReducerCall(#[from] ReducerCallError),
    #[error("Failed to insert `st_client` row for module without client_connected reducer: {0}")]
    DBError(#[from] DBError),
    #[error("Connection rejected by `client_connected` reducer: {0}")]
    Rejected(String),
    #[error("Insufficient energy balance to run `client_connected` reducer")]
    OutOfEnergy,
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

        // Operations on module instances (e.g. calling reducers) is blocking,
        // partially because the computation can potentialyl take a long time
        // and partially because interacting with the database requires taking
        // a blocking lock. So, we run `f` inside of `asyncify()`, which runs
        // the provided closure in a tokio blocking task, and bubbles up any
        // panic that may occur.

        // If a reducer call panics, we **must** ensure to call `self.on_panic`
        // so that the module is discarded by the host controller.
        scopeguard::defer_on_unwind!({
            log::warn!("reducer {reducer} panicked");
            (self.on_panic)();
        });
        let result = asyncify(move || f(&mut *inst)).await;
        Ok(result)
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) {
        log::trace!("disconnecting client {}", client_id);
        let this = self.clone();
        asyncify(move || this.subscriptions().remove_subscriber(client_id)).await;
        // ignore NoSuchModule; if the module's already closed, that's fine
        if let Err(e) = self
            .call_identity_disconnected(client_id.identity, client_id.connection_id)
            .await
        {
            log::error!("Error from client_disconnected transaction: {e}");
        }
    }

    /// Invoke the module's `client_connected` reducer, if it has one,
    /// and insert a new row into `st_client` for `(caller_identity, caller_connection_id)`.
    ///
    /// The host inspects `st_client` when restarting in order to run `client_disconnected` reducers
    /// for clients that were connected at the time when the host went down.
    /// This ensures that every client connection eventually has `client_disconnected` invoked.
    ///
    /// If this method returns `Ok`, then the client connection has been approved,
    /// and the new row has been inserted into `st_client`.
    ///
    /// If this method returns `Err`, then the client connection has either failed or been rejected,
    /// and `st_client` has not been modified.
    /// In this case, the caller should terminate the connection.
    pub async fn call_identity_connected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        let reducer_lookup = self.info.module_def.lifecycle_reducer(Lifecycle::OnConnect);

        if let Some((reducer_id, reducer_def)) = reducer_lookup {
            // The module defined a lifecycle reducer to handle new connections.
            // Call this reducer.
            // If the call fails (as in, something unexpectedly goes wrong with WASM execution),
            // abort the connection: we can't really recover.
            let reducer_outcome = self
                .call_reducer_inner(
                    caller_identity,
                    Some(caller_connection_id),
                    None,
                    None,
                    None,
                    reducer_id,
                    reducer_def,
                    ReducerArgs::Nullary,
                )
                .await?;

            match reducer_outcome.outcome {
                // If the reducer committed successfully, we're done.
                // `WasmModuleInstance::call_reducer_with_tx` has already ensured
                // that `st_client` is updated appropriately.
                //
                // It's necessary to spread out the responsibility for updating `st_client` in this way
                // because it's important that `call_identity_connected` commit at most one transaction.
                // A naive implementation of this method would just run the reducer first,
                // then insert into `st_client`,
                // but if we crashed in between, we'd be left in an inconsistent state
                // where the reducer had run but `st_client` was not yet updated.
                ReducerOutcome::Committed => Ok(()),

                // If the reducer returned an error or couldn't run due to insufficient energy,
                // abort the connection: the module code has decided it doesn't want this client.
                ReducerOutcome::Failed(message) => Err(ClientConnectedError::Rejected(message)),
                ReducerOutcome::BudgetExceeded => Err(ClientConnectedError::OutOfEnergy),
            }
        } else {
            // The module doesn't define a client_connected reducer.
            // Commit a transaction to update `st_clients`
            // and to ensure we always have those events paired in the commitlog.
            //
            // This is necessary to be able to disconnect clients after a server crash.
            let reducer_name = reducer_lookup
                .as_ref()
                .map(|(_, def)| &*def.name)
                .unwrap_or("__identity_connected__");

            let workload = Workload::Reducer(ReducerContext {
                name: reducer_name.to_owned(),
                caller_identity,
                caller_connection_id,
                timestamp: Timestamp::now(),
                arg_bsatn: Bytes::new(),
            });

            let stdb = self.inner.replica_ctx().relational_db.clone();
            asyncify(move || {
                stdb.with_auto_commit(workload, |mut_tx| {
                    mut_tx.insert_st_client(caller_identity, caller_connection_id)
                })
            })
            .await
            .inspect_err(|e| {
                log::error!("`call_identity_connected`: fallback transaction to insert into `st_client` failed: {e:#?}")
            })
            .map_err(Into::into)
        }
    }

    /// Invoke the module's `client_disconnected` reducer, if it has one,
    /// and delete the client's row from `st_client`, if any.
    ///
    /// The host inspects `st_client` when restarting in order to run `client_disconnected` reducers
    /// for clients that were connected at the time when the host went down.
    /// This ensures that every client connection eventually has `client_disconnected` invoked.
    ///
    /// Unlike [`Self::call_identity_connected`],
    /// this method swallows errors returned by the `client_disconnected` reducer.
    /// The database can't reject a disconnection - the client's gone, whether the database likes it or not.
    ///
    /// If this method returns an error, the database is likely to wind up in a bad state,
    /// as that means we've somehow failed to delete from `st_client`.
    /// We cannot meaningfully handle this.
    /// Sometimes it just means that the database no longer exists, though, which is fine.
    pub async fn call_identity_disconnected(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ReducerCallError> {
        let reducer_lookup = self.info.module_def.lifecycle_reducer(Lifecycle::OnDisconnect);

        // A fallback transaction that deletes the client from `st_client`.
        let fallback = || async {
            let reducer_name = reducer_lookup
                .as_ref()
                .map(|(_, def)| &*def.name)
                .unwrap_or("__identity_disconnected__");

            let workload = Workload::Reducer(ReducerContext {
                name: reducer_name.to_owned(),
                caller_identity,
                caller_connection_id,
                timestamp: Timestamp::now(),
                arg_bsatn: Bytes::new(),
            });
            let stdb = self.inner.replica_ctx().relational_db.clone();
            let database_identity = self.info.database_identity;
            asyncify(move || {
                stdb.with_auto_commit(workload, |mut_tx| {
                    mut_tx.delete_st_client(caller_identity, caller_connection_id, database_identity)
                })
            })
            .await
            .map_err(|err| {
                log::error!(
                    "`call_identity_disconnected`: fallback transaction to delete from `st_client` failed: {err}"
                );
                InvalidReducerArguments {
                    err: err.into(),
                    reducer: reducer_name.into(),
                }
                .into()
            })
        };

        if let Some((reducer_id, reducer_def)) = reducer_lookup {
            // The module defined a lifecycle reducer to handle disconnects. Call it.
            // If it succeeds, `WasmModuleInstance::call_reducer_with_tx` has already ensured
            // that `st_client` is updated appropriately.
            let result = self
                .call_reducer_inner(
                    caller_identity,
                    Some(caller_connection_id),
                    None,
                    None,
                    None,
                    reducer_id,
                    reducer_def,
                    ReducerArgs::Nullary,
                )
                .await;

            // If it failed, we still need to update `st_client`: the client's not coming back.
            // Commit a separate transaction that just updates `st_client`.
            //
            // It's OK for this to not be atomic with the previous transaction,
            // since that transaction didn't commit. If we crash before committing this one,
            // we'll run the `client_disconnected` reducer again unnecessarily,
            // but the commitlog won't contain two invocations of it, which is what we care about.
            match result {
                Err(e) => {
                    log::error!("call_reducer_inner of client_disconnected failed: {e:#?}");
                    fallback().await
                }
                Ok(ReducerCallResult {
                    outcome: ReducerOutcome::Failed(_) | ReducerOutcome::BudgetExceeded,
                    ..
                }) => fallback().await,

                // If it succeeded, as mentioend above, `st_client` is already updated.
                Ok(ReducerCallResult {
                    outcome: ReducerOutcome::Committed,
                    ..
                }) => Ok(()),
            }
        } else {
            // The module doesn't define a `client_disconnected` reducer.
            // Commit a transaction to update `st_clients`.
            fallback().await
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
        let replica_ctx = self.inner.replica_ctx().clone();
        let info = self.info.clone();
        self.call("<init_database>", move |inst| {
            init_database(&replica_ctx, &info.module_def, inst, program)
        })
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

            let (
                // A query may compile down to several plans.
                // This happens when there are multiple RLS rules per table.
                // The original query is the union of these plans.
                plans,
                _,
                table_name,
                _,
            ) = compile_subscription(&query, &tx, &auth)?;

            // Optimize each fragment
            let optimized = plans
                .into_iter()
                .map(|plan| plan.optimize())
                .collect::<Result<Vec<_>, _>>()?;

            check_row_limit(
                &optimized,
                db,
                &tx,
                // Estimate the number of rows this query will scan
                |plan, tx| estimate_rows_scanned(tx, plan),
                &auth,
            )?;

            let optimized = optimized
                .into_iter()
                // Convert into something we can execute
                .map(PipelinedProject::from)
                .collect::<Vec<_>>();

            // Execute the union and return the results
            execute_plan::<_, F>(&optimized, &DeltaTx::from(&*tx))
                .map(|(rows, _, metrics)| (OneOffTable { table_name, rows }, metrics))
                .context("One-off queries are not allowed to modify the database")
        })?;

        db.exec_counters_for(WorkloadType::Sql).record(&metrics);

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
