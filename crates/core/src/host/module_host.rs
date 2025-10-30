use super::{
    ArgsTuple, FunctionArgs, InvalidProcedureArguments, InvalidReducerArguments, ProcedureCallResult,
    ReducerCallResult, ReducerId, ReducerOutcome, Scheduler,
};
use crate::client::messages::{OneOffQueryResponseMessage, SerializableMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_logger::{LogLevel, Record};
use crate::db::relational_db::RelationalDB;
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::hash::Hash;
use crate::host::InvalidFunctionArguments;
use crate::identity::Identity;
use crate::messages::control_db::{Database, HostType};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::sql::ast::SchemaViewer;
use crate::sql::parser::RowLevelExpr;
use crate::subscription::execute_plan;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::subscription::tx::DeltaTx;
use crate::subscription::websocket_building::BuildableWebsocketFormat;
use crate::util::jobs::{SingleCoreExecutor, WeakSingleCoreExecutor};
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use anyhow::Context;
use bytes::Bytes;
use derive_more::From;
use futures::lock::Mutex;
use indexmap::IndexSet;
use itertools::Itertools;
use prometheus::{Histogram, IntGauge};
use scopeguard::ScopeGuard;
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_client_api_messages::websocket::{ByteListLen, Compression, OneOffTable, QueryUpdate};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::{HashCollectionExt as _, IntMap};
use spacetimedb_datastore::execution_context::{ExecutionContext, ReducerContext, Workload, WorkloadType};
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::system_tables::{ST_CLIENT_ID, ST_CONNECTION_CREDENTIALS_ID};
use spacetimedb_datastore::traits::{IsolationLevel, Program, TxData};
use spacetimedb_durability::DurableOffset;
use spacetimedb_execution::pipelined::PipelinedProject;
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::ConnectionId;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::{ProcedureId, TableId};
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::ProductValue;
use spacetimedb_schema::auto_migrate::{AutoMigrateError, MigrationPolicy};
use spacetimedb_schema::def::deserialize::ArgsSeed;
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, ViewDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_vm::relation::RelValue;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

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

    pub fn encode<F: BuildableWebsocketFormat>(&self) -> (F::QueryUpdate, u64, usize) {
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

impl fmt::Debug for ModuleInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleInfo")
            .field("module_def", &self.module_def)
            .field("owner_identity", &self.owner_identity)
            .field("database_identity", &self.database_identity)
            .field("module_hash", &self.module_hash)
            .finish()
    }
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

/// A runtime that can create modules.
pub trait ModuleRuntime {
    /// Creates a module based on the context `mcc`.
    ///
    /// Also returns the initial instance for the module.
    fn make_actor(&self, mcc: ModuleCreationContext<'_>) -> anyhow::Result<(Module, Instance)>;
}

pub enum Module {
    Wasm(super::wasmtime::Module),
    Js(super::v8::JsModule),
}

pub enum Instance {
    // Box these instances because they're very different sizes,
    // which makes Clippy sad and angry.
    Wasm(Box<super::wasmtime::ModuleInstance>),
    Js(Box<super::v8::JsInstance>),
}

impl Module {
    fn replica_ctx(&self) -> &Arc<ReplicaContext> {
        match self {
            Module::Wasm(module) => module.replica_ctx(),
            Module::Js(module) => module.replica_ctx(),
        }
    }

    fn scheduler(&self) -> &Scheduler {
        match self {
            Module::Wasm(module) => module.scheduler(),
            Module::Js(module) => module.scheduler(),
        }
    }

    fn info(&self) -> Arc<ModuleInfo> {
        match self {
            Module::Wasm(module) => module.info(),
            Module::Js(module) => module.info(),
        }
    }
    async fn create_instance(&self) -> Instance {
        match self {
            Module::Wasm(module) => Instance::Wasm(Box::new(module.create_instance())),
            Module::Js(module) => Instance::Js(Box::new(module.create_instance().await)),
        }
    }
    fn host_type(&self) -> HostType {
        match self {
            Module::Wasm(_) => HostType::Wasm,
            Module::Js(_) => HostType::Js,
        }
    }
}

impl Instance {
    fn trapped(&self) -> bool {
        match self {
            Instance::Wasm(inst) => inst.trapped(),
            Instance::Js(inst) => inst.trapped(),
        }
    }

    /// Update the module instance's database to match the schema of the module instance.
    fn update_database(
        &mut self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        match self {
            Instance::Wasm(inst) => inst.update_database(program, old_module_info, policy),
            Instance::Js(inst) => inst.update_database(program, old_module_info, policy),
        }
    }

    fn call_reducer(&mut self, tx: Option<MutTxId>, params: CallReducerParams) -> ReducerCallResult {
        match self {
            Instance::Wasm(inst) => inst.call_reducer(tx, params),
            Instance::Js(inst) => inst.call_reducer(tx, params),
        }
    }

    async fn call_procedure(&mut self, params: CallProcedureParams) -> Result<ProcedureCallResult, ProcedureCallError> {
        match self {
            Instance::Wasm(inst) => inst.call_procedure(params).await,
            Instance::Js(inst) => inst.call_procedure(params).await,
        }
    }
}

/// Creates the table for `table_def` in `stdb`.
pub fn create_table_from_def(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    module_def: &ModuleDef,
    table_def: &TableDef,
) -> anyhow::Result<()> {
    let schema = TableSchema::from_module_def(module_def, table_def, (), TableId::SENTINEL);
    stdb.create_table(tx, schema)
        .with_context(|| format!("failed to create table {}", &table_def.name))?;
    Ok(())
}

/// Creates the table for `view_def` in `stdb`.
pub fn create_table_from_view_def(
    stdb: &RelationalDB,
    tx: &mut MutTxId,
    module_def: &ModuleDef,
    view_def: &ViewDef,
) -> anyhow::Result<()> {
    stdb.create_view(tx, module_def, view_def)
        .with_context(|| format!("failed to create table for view {}", &view_def.name))?;
    Ok(())
}

/// If the module instance's replica_ctx is uninitialized, initialize it.
fn init_database(
    replica_ctx: &ReplicaContext,
    module_def: &ModuleDef,
    inst: &mut Instance,
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
                logger.info(&format!("Creating table `{}`", &def.name));
                create_table_from_def(stdb, tx, module_def, def)?;
            }

            let mut view_defs: Vec<_> = module_def.views().collect();
            view_defs.sort_by(|a, b| a.name.cmp(&b.name));

            for def in view_defs {
                logger.info(&format!("Creating table for view `{}`", &def.name));
                create_table_from_view_def(stdb, tx, module_def, def)?;
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
            if let Some((_tx_offset, tx_data, tx_metrics, reducer)) = stdb.commit_tx(tx)? {
                stdb.report_mut_tx_metrics(reducer, tx_metrics, Some(tx_data));
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

pub struct CallProcedureParams {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub caller_connection_id: ConnectionId,
    pub timer: Option<Instant>,
    pub procedure_id: ProcedureId,
    pub args: ArgsTuple,
}

/// Holds a [`Module`] and a set of [`Instance`]s from it,
/// and allocates the [`Instance`]s to be used for function calls.
///
/// Capable of managing and allocating multiple instances of the same module,
/// but this functionality is currently unused, as only one reducer runs at a time.
/// When we introduce procedures, it will be necessary to have multiple instances,
/// as each procedure invocation will have its own sandboxed instance,
/// and multiple procedures can run concurrently with up to one reducer.
struct ModuleInstanceManager {
    instances: VecDeque<Instance>,
    module: Arc<Module>,
    create_instance_time_metric: CreateInstanceTimeMetric,
}

/// Handle on the `spacetime_module_create_instance_time_seconds` label for a particular database
/// which calls `remove_label_values` to clean up on drop.
struct CreateInstanceTimeMetric {
    metric: Histogram,
    host_type: HostType,
    database_identity: Identity,
}

impl Drop for CreateInstanceTimeMetric {
    fn drop(&mut self) {
        let _ = WORKER_METRICS
            .module_create_instance_time_seconds
            .remove_label_values(&self.database_identity, &self.host_type);
    }
}

impl CreateInstanceTimeMetric {
    fn observe(&self, duration: std::time::Duration) {
        self.metric.observe(duration.as_secs_f64());
    }
}

impl ModuleInstanceManager {
    fn new(module: Arc<Module>, init_inst: Instance, database_identity: Identity) -> Self {
        let host_type = module.host_type();
        let create_instance_time_metric = CreateInstanceTimeMetric {
            metric: WORKER_METRICS
                .module_create_instance_time_seconds
                .with_label_values(&database_identity, &host_type),
            host_type,
            database_identity,
        };

        // Add the first instance.
        let mut instances = VecDeque::new();
        instances.push_front(init_inst);

        Self {
            instances,
            module,
            create_instance_time_metric,
        }
    }
    async fn get_instance(&mut self) -> Instance {
        if let Some(inst) = self.instances.pop_back() {
            inst
        } else {
            let start_time = std::time::Instant::now();
            // TODO: should we be calling `create_instance` on the `SingleCoreExecutor` rather than the calling thread?
            let res = self.module.create_instance().await;
            let elapsed_time = start_time.elapsed();
            self.create_instance_time_metric.observe(elapsed_time);
            res
        }
    }

    fn return_instance(&mut self, inst: Instance) {
        if inst.trapped() {
            // Don't return trapped instances;
            // they may have left internal data structures in the guest `Instance`
            // (WASM linear memory, V8 global scope) in a bad state.
            return;
        }

        self.instances.push_front(inst);
    }
}

#[derive(Clone)]
pub struct ModuleHost {
    pub info: Arc<ModuleInfo>,
    module: Arc<Module>,
    /// Called whenever a reducer call on this host panics.
    on_panic: Arc<dyn Fn() + Send + Sync + 'static>,
    instance_manager: Arc<Mutex<ModuleInstanceManager>>,
    executor: SingleCoreExecutor,

    /// Marks whether this module has been closed by [`Self::exit`].
    ///
    /// When this is true, most operations will fail with [`NoSuchModule`].
    closed: Arc<AtomicBool>,
}

impl fmt::Debug for ModuleHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleHost")
            .field("info", &self.info)
            .field("module", &Arc::as_ptr(&self.module))
            .finish()
    }
}

pub struct WeakModuleHost {
    info: Arc<ModuleInfo>,
    inner: Weak<Module>,
    on_panic: Weak<dyn Fn() + Send + Sync + 'static>,
    instance_manager: Weak<Mutex<ModuleInstanceManager>>,
    executor: WeakSingleCoreExecutor,
    closed: Weak<AtomicBool>,
}

#[derive(Debug)]
pub enum UpdateDatabaseResult {
    NoUpdateNeeded,
    UpdatePerformed,
    UpdatePerformedWithClientDisconnect,
    AutoMigrateError(ErrorStream<AutoMigrateError>),
    ErrorExecutingMigration(anyhow::Error),
}
impl UpdateDatabaseResult {
    /// Check if a database update was successful.
    pub fn was_successful(&self) -> bool {
        matches!(
            self,
            UpdateDatabaseResult::UpdatePerformed
                | UpdateDatabaseResult::NoUpdateNeeded
                | UpdateDatabaseResult::UpdatePerformedWithClientDisconnect
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
pub enum ProcedureCallError {
    #[error(transparent)]
    Args(#[from] InvalidProcedureArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error("No such procedure")]
    NoSuchProcedure,
    #[error("Procedure terminated due to insufficient budget")]
    OutOfEnergy,
    #[error("The module instance encountered a fatal error: {0}")]
    InternalError(String),
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
    pub(super) fn new(
        module: Module,
        init_inst: Instance,
        on_panic: impl Fn() + Send + Sync + 'static,
        executor: SingleCoreExecutor,
        database_identity: Identity,
    ) -> Self {
        let info = module.info();
        let module = Arc::new(module);
        let on_panic = Arc::new(on_panic);

        let module_clone = module.clone();

        let instance_manager = ModuleInstanceManager::new(module_clone, init_inst, database_identity);
        let instance_manager = Arc::new(Mutex::new(instance_manager));

        ModuleHost {
            info,
            module,
            on_panic,
            instance_manager,
            executor,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    #[inline]
    pub fn info(&self) -> &ModuleInfo {
        &self.info
    }

    #[inline]
    pub fn subscriptions(&self) -> &ModuleSubscriptions {
        &self.info.subscriptions
    }

    fn is_marked_closed(&self) -> bool {
        // `self.closed` isn't used for any synchronization, it's just a shared flag,
        // so `Ordering::Relaxed` is sufficient.
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn guard_closed(&self) -> Result<(), NoSuchModule> {
        if self.is_marked_closed() {
            Err(NoSuchModule)
        } else {
            Ok(())
        }
    }

    /// Run a function on the JobThread for this module.
    /// This would deadlock if it is called within another call to `on_module_thread`.
    /// Since this is async, and `f` is sync, deadlocking shouldn't be a problem.
    pub async fn on_module_thread<F, R>(&self, label: &str, f: F) -> Result<R, anyhow::Error>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.guard_closed()?;

        let timer_guard = self.start_call_timer(label);

        let res = self
            .executor
            .run_sync_job(move || {
                drop(timer_guard);
                f()
            })
            .await;

        Ok(res)
    }

    fn start_call_timer(&self, label: &str) -> ScopeGuard<(), impl FnOnce(())> {
        // Record the time until our function starts running.
        let queue_timer = WORKER_METRICS
            .reducer_wait_time
            .with_label_values(&self.info.database_identity, label)
            .start_timer();
        let queue_length_gauge = WORKER_METRICS
            .instance_queue_length
            .with_label_values(&self.info.database_identity);
        queue_length_gauge.inc();
        {
            let queue_length = queue_length_gauge.get();
            WORKER_METRICS
                .instance_queue_length_histogram
                .with_label_values(&self.info.database_identity)
                .observe(queue_length as f64);
        }
        // Ensure that we always decrement the gauge.
        scopeguard::guard((), move |_| {
            // Decrement the queue length gauge when we're done.
            // This is done in a defer so that it happens even if the reducer call panics.
            queue_length_gauge.dec();
            queue_timer.stop_and_record();
        })
    }

    async fn call_async_with_instance<Fun, Fut, R>(&self, label: &str, f: Fun) -> Result<R, NoSuchModule>
    where
        Fun: (FnOnce(Instance) -> Fut) + Send + 'static,
        Fut: Future<Output = (R, Instance)> + Send + 'static,
        R: Send + 'static,
    {
        self.guard_closed()?;
        let timer_guard = self.start_call_timer(label);

        scopeguard::defer_on_unwind!({
            log::warn!("procedure {label} panicked");
            (self.on_panic)();
        });

        // TODO: should we be calling and/or `await`-ing `get_instance` within the below `run_job`?
        // Unclear how much overhead this call can have.
        let instance = self.instance_manager.lock().await.get_instance().await;

        let (res, instance) = self
            .executor
            .run_job(async move {
                drop(timer_guard);
                f(instance).await
            })
            .await;

        self.instance_manager.lock().await.return_instance(instance);

        Ok(res)
    }

    /// Run a function on the JobThread for this module which has access to the module instance.
    async fn call<F, R>(&self, label: &str, f: F) -> Result<R, NoSuchModule>
    where
        F: FnOnce(&mut Instance) -> R + Send + 'static,
        R: Send + 'static,
    {
        self.guard_closed()?;
        let timer_guard = self.start_call_timer(label);

        // Operations on module instances (e.g. calling reducers) is blocking,
        // partially because the computation can potentially take a long time
        // and partially because interacting with the database requires taking
        // a blocking lock. So, we run `f` on a dedicated thread with `self.executor`.
        // This will bubble up any panic that may occur.

        // If a reducer call panics, we **must** ensure to call `self.on_panic`
        // so that the module is discarded by the host controller.
        scopeguard::defer_on_unwind!({
            log::warn!("reducer {label} panicked");
            (self.on_panic)();
        });

        let mut instance = self.instance_manager.lock().await.get_instance().await;

        let (res, instance) = self
            .executor
            .run_sync_job(move || {
                drop(timer_guard);
                let res = f(&mut instance);
                (res, instance)
            })
            .await;

        self.instance_manager.lock().await.return_instance(instance);

        Ok(res)
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) {
        log::trace!("disconnecting client {client_id}");
        let this = self.clone();
        if let Err(e) = self
            .call("disconnect_client", move |inst| {
                // Call the `client_disconnected` reducer, if it exists.
                // This is a no-op if the module doesn't define such a reducer.
                this.subscriptions().remove_subscriber(client_id);
                this.call_identity_disconnected_inner(client_id.identity, client_id.connection_id, inst)
            })
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
        caller_auth: ConnectionAuthCtx,
        caller_connection_id: ConnectionId,
    ) -> Result<(), ClientConnectedError> {
        let me = self.clone();
        self.call("call_identity_connected", move |inst| {
            let reducer_lookup = me.info.module_def.lifecycle_reducer(Lifecycle::OnConnect);
            let stdb = &me.module.replica_ctx().relational_db;
            let workload = Workload::Reducer(ReducerContext {
                name: "call_identity_connected".to_owned(),
                caller_identity: caller_auth.claims.identity,
                caller_connection_id,
                timestamp: Timestamp::now(),
                arg_bsatn: Bytes::new(),
            });
            let mut_tx = stdb.begin_mut_tx(IsolationLevel::Serializable, workload);
            let mut mut_tx = scopeguard::guard(mut_tx, |mut_tx| {
                // If we crash before committing, we need to ensure that the transaction is rolled back.
                // This is necessary to avoid leaving the database in an inconsistent state.
                log::debug!("call_identity_connected: rolling back transaction");
                let (_, metrics, reducer_name) = mut_tx.rollback();
                stdb.report_mut_tx_metrics(reducer_name, metrics, None);
            });

            mut_tx
                .insert_st_client(
                    caller_auth.claims.identity,
                    caller_connection_id,
                    &caller_auth.jwt_payload,
                )
                .map_err(DBError::from)?;

            if let Some((reducer_id, reducer_def)) = reducer_lookup {
                // The module defined a lifecycle reducer to handle new connections.
                // Call this reducer.
                // If the call fails (as in, something unexpectedly goes wrong with guest execution),
                // abort the connection: we can't really recover.
                let reducer_outcome = me.call_reducer_inner_with_inst(
                    Some(ScopeGuard::into_inner(mut_tx)),
                    caller_auth.claims.identity,
                    Some(caller_connection_id),
                    None,
                    None,
                    None,
                    reducer_id,
                    reducer_def,
                    FunctionArgs::Nullary,
                    inst,
                )?;

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
                // We need to commit the transaction to update st_clients and st_connection_credentials.
                //
                // This is necessary to be able to disconnect clients after a server crash.

                // TODO: Is this being broadcast? Does it need to be, or are st_client table subscriptions
                // not allowed?
                // I (jsdt) don't think it was being broadcast previously. See:
                // https://github.com/clockworklabs/SpacetimeDB/issues/3130
                stdb.finish_tx(ScopeGuard::into_inner(mut_tx), Ok(()))
                    .map_err(|e: DBError| {
                        log::error!("`call_identity_connected`: finish transaction failed: {e:#?}");
                        ClientConnectedError::DBError(e)
                    })?;
                Ok(())
            }
        })
        .await
        .map_err(ReducerCallError::from)?
    }

    /// Invokes the `client_disconnected` reducer, if present,
    /// then deletes the client’s rows from `st_client` and `st_connection_credentials`.
    /// If the reducer fails, the rows are still deleted.
    /// Calling this on an already-disconnected client is a no-op.
    pub fn call_identity_disconnected_inner(
        &self,
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        inst: &mut Instance,
    ) -> Result<(), ReducerCallError> {
        let reducer_lookup = self.info.module_def.lifecycle_reducer(Lifecycle::OnDisconnect);
        let reducer_name = reducer_lookup
            .as_ref()
            .map(|(_, def)| &*def.name)
            .unwrap_or("__identity_disconnected__");

        let is_client_exist = |mut_tx: &MutTxId| mut_tx.st_client_row(caller_identity, caller_connection_id).is_some();

        let workload = || {
            Workload::Reducer(ReducerContext {
                name: reducer_name.to_owned(),
                caller_identity,
                caller_connection_id,
                timestamp: Timestamp::now(),
                arg_bsatn: Bytes::new(),
            })
        };

        let me = self.clone();
        let stdb = me.module.replica_ctx().relational_db.clone();

        // A fallback transaction that deletes the client from `st_client`.
        let fallback = || {
            let database_identity = me.info.database_identity;
            stdb.with_auto_commit(workload(), |mut_tx| {
                if !is_client_exist(mut_tx) {
                    // The client is already gone. Nothing to do.
                    log::debug!(
                        "`call_identity_disconnected`: no row in `st_client` for ({caller_identity}, {caller_connection_id}), nothing to do",
                    );
                    return Ok(());
                }

                mut_tx
                    .delete_st_client(caller_identity, caller_connection_id, database_identity)
                    .map_err(DBError::from)
            })
            .map_err(|err| {
                log::error!(
                    "`call_identity_disconnected`: fallback transaction to delete from `st_client` failed: {err}"
                );
                InvalidReducerArguments(InvalidFunctionArguments {
                    err: err.into(),
                    function_name: reducer_name.into(),
                })
                .into()
            })
        };

        if let Some((reducer_id, reducer_def)) = reducer_lookup {
            let stdb = me.module.replica_ctx().relational_db.clone();
            let mut_tx = stdb.begin_mut_tx(IsolationLevel::Serializable, workload());

            if !is_client_exist(&mut_tx) {
                // The client is already gone. Nothing to do.
                log::debug!(
                    "`call_identity_disconnected`: no row in `st_client` for ({caller_identity}, {caller_connection_id}), nothing to do",
                );
                return Ok(());
            }

            // The module defined a lifecycle reducer to handle disconnects. Call it.
            // If it succeeds, `WasmModuleInstance::call_reducer_with_tx` has already ensured
            // that `st_client` is updated appropriately.
            let result = me.call_reducer_inner_with_inst(
                Some(mut_tx),
                caller_identity,
                Some(caller_connection_id),
                None,
                None,
                None,
                reducer_id,
                reducer_def,
                FunctionArgs::Nullary,
                inst,
            );

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
                    fallback()
                }
                Ok(ReducerCallResult {
                    outcome: ReducerOutcome::Failed(_) | ReducerOutcome::BudgetExceeded,
                    ..
                }) => fallback(),

                // If it succeeded, as mentioned above, `st_client` is already updated.
                Ok(ReducerCallResult {
                    outcome: ReducerOutcome::Committed,
                    ..
                }) => Ok(()),
            }
        } else {
            // The module doesn't define a `client_disconnected` reducer.
            // Commit a transaction to update `st_clients`.
            fallback()
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
        let me = self.clone();
        self.call("call_identity_disconnected", move |inst| {
            me.call_identity_disconnected_inner(caller_identity, caller_connection_id, inst)
        })
        .await?
    }

    /// Empty the system tables tracking clients without running any lifecycle reducers.
    pub async fn clear_all_clients(&self) -> anyhow::Result<()> {
        let me = self.clone();
        self.call("clear_all_clients", move |_| {
            let stdb = &me.module.replica_ctx().relational_db;
            let workload = Workload::Internal;
            stdb.with_auto_commit(workload, |mut_tx| {
                stdb.clear_table(mut_tx, ST_CONNECTION_CREDENTIALS_ID)?;
                stdb.clear_table(mut_tx, ST_CLIENT_ID)?;
                Ok::<(), DBError>(())
            })
        })
        .await?
        .map_err(anyhow::Error::from)
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
        args: FunctionArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let reducer_seed = ArgsSeed(self.info.module_def.typespace().with_type(reducer_def));
        let args = args.into_tuple(reducer_seed).map_err(InvalidReducerArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);

        Ok(self
            .call(&reducer_def.name, move |inst| {
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
            .await?)
    }
    fn call_reducer_inner_with_inst(
        &self,
        tx: Option<MutTxId>,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_id: ReducerId,
        reducer_def: &ReducerDef,
        args: FunctionArgs,
        module_instance: &mut Instance,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let reducer_seed = ArgsSeed(self.info.module_def.typespace().with_type(reducer_def));
        let args = args.into_tuple(reducer_seed).map_err(InvalidReducerArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);

        Ok(module_instance.call_reducer(
            tx,
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
        ))
    }

    pub async fn call_reducer(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_name: &str,
        args: FunctionArgs,
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
            Err(ReducerCallError::NoSuchReducer) => Some(no_such_function_log_message("reducer", reducer_name)),
            Err(ReducerCallError::Args(_)) => Some(args_error_log_message("reducer", reducer_name)),
            _ => None,
        };
        if let Some(log_message) = log_message {
            self.inject_logs(LogLevel::Error, reducer_name, &log_message)
        }

        res
    }

    pub async fn call_procedure(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        timer: Option<Instant>,
        procedure_name: &str,
        args: FunctionArgs,
    ) -> Result<ProcedureCallResult, ProcedureCallError> {
        let res = async {
            let (procedure_id, procedure_def) = self
                .info
                .module_def
                .procedure_full(procedure_name)
                .ok_or(ProcedureCallError::NoSuchProcedure)?;
            self.call_procedure_inner(
                caller_identity,
                caller_connection_id,
                timer,
                procedure_id,
                procedure_def,
                args,
            )
            .await
        }
        .await;

        let log_message = match &res {
            Err(ProcedureCallError::NoSuchProcedure) => Some(no_such_function_log_message("procedure", procedure_name)),
            Err(ProcedureCallError::Args(_)) => Some(args_error_log_message("procedure", procedure_name)),
            _ => None,
        };

        if let Some(log_message) = log_message {
            self.inject_logs(LogLevel::Error, procedure_name, &log_message)
        }

        res
    }

    async fn call_procedure_inner(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        timer: Option<Instant>,
        procedure_id: ProcedureId,
        procedure_def: &ProcedureDef,
        args: FunctionArgs,
    ) -> Result<ProcedureCallResult, ProcedureCallError> {
        let procedure_seed = ArgsSeed(self.info.module_def.typespace().with_type(procedure_def));
        let args = args.into_tuple(procedure_seed).map_err(InvalidProcedureArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);

        self.call_async_with_instance(&procedure_def.name, async move |mut inst| {
            let res = inst
                .call_procedure(CallProcedureParams {
                    timestamp: Timestamp::now(),
                    caller_identity,
                    caller_connection_id,
                    timer,
                    procedure_id,
                    args,
                })
                .await;
            (res, inst)
        })
        .await?
    }

    // Scheduled reducers require a different function here to call their reducer
    // because their reducer arguments are stored in the database and need to be fetched
    // within the same transaction as the reducer call.
    pub async fn call_scheduled_reducer(
        &self,
        call_reducer_params: impl FnOnce(&MutTxId) -> anyhow::Result<Option<CallReducerParams>> + Send + 'static,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let db = self.module.replica_ctx().relational_db.clone();
        // scheduled reducer name not fetched yet, anyway this is only for logging purpose
        const REDUCER: &str = "scheduled_reducer";
        let module = self.info.clone();
        self.call(REDUCER, move |inst: &mut Instance| {
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
                Err(err) => Err(ReducerCallError::Args(InvalidReducerArguments(
                    InvalidFunctionArguments {
                        err,
                        function_name: REDUCER.into(),
                    },
                ))),
            }
        })
        .await?
    }

    pub fn subscribe_to_logs(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.info().log_tx.subscribe())
    }

    pub async fn init_database(&self, program: Program) -> Result<Option<ReducerCallResult>, InitDatabaseError> {
        let replica_ctx = self.module.replica_ctx().clone();
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
        policy: MigrationPolicy,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.call("<update_database>", move |inst| {
            inst.update_database(program, old_module_info, policy)
        })
        .await?
    }

    pub async fn exit(&self) {
        // As in `Self::marked_closed`, `Relaxed` is sufficient because we're not synchronizing any external state.
        self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
        self.module.scheduler().close();
        self.exited().await;
    }

    pub async fn exited(&self) {
        self.module.scheduler().closed().await;
    }

    pub fn inject_logs(&self, log_level: LogLevel, function_name: &str, message: &str) {
        self.replica_ctx().logger.write(
            log_level,
            &Record {
                function: Some(function_name),
                ..Record::injected(message)
            },
            &(),
        )
    }

    /// Execute a one-off query and send the results to the given client.
    /// This only returns an error if there is a db-level problem.
    /// An error with the query itself will be sent to the client.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn one_off_query<F: BuildableWebsocketFormat>(
        &self,
        caller_identity: Identity,
        query: String,
        client: Arc<ClientConnectionSender>,
        message_id: Vec<u8>,
        timer: Instant,
        // We take this because we only have a way to convert with the concrete types (Bsatn and Json)
        into_message: impl FnOnce(OneOffQueryResponseMessage<F>) -> SerializableMessage + Send + 'static,
    ) -> Result<(), anyhow::Error> {
        let replica_ctx = self.replica_ctx();
        let db = replica_ctx.relational_db.clone();
        let subscriptions = replica_ctx.subscriptions.clone();
        let auth = AuthCtx::new(replica_ctx.owner_identity, caller_identity);
        log::debug!("One-off query: {query}");
        let metrics = self
            .on_module_thread("one_off_query", move || {
                let (tx_offset_sender, tx_offset_receiver) = oneshot::channel();
                let tx = scopeguard::guard(db.begin_tx(Workload::Sql), |tx| {
                    let (tx_offset, tx_metrics, reducer) = db.release_tx(tx);
                    let _ = tx_offset_sender.send(tx_offset);
                    db.report_read_tx_metrics(reducer, tx_metrics);
                });

                // We wrap the actual query in a closure so we can use ? to handle errors without making
                // the entire transaction abort with an error.
                let result: Result<(OneOffTable<F>, ExecutionMetrics), anyhow::Error> = (|| {
                    let tx = SchemaViewer::new(&*tx, &auth);

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
                        &db,
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
                })();

                let total_host_execution_duration = timer.elapsed().into();
                let (message, metrics): (SerializableMessage, Option<ExecutionMetrics>) = match result {
                    Ok((rows, metrics)) => (
                        into_message(OneOffQueryResponseMessage {
                            message_id,
                            error: None,
                            results: vec![rows],
                            total_host_execution_duration,
                        }),
                        Some(metrics),
                    ),
                    Err(err) => (
                        into_message(OneOffQueryResponseMessage {
                            message_id,
                            error: Some(format!("{err}")),
                            results: vec![],
                            total_host_execution_duration,
                        }),
                        None,
                    ),
                };

                subscriptions.send_client_message(client, message, (&*tx, tx_offset_receiver))?;
                Ok::<Option<ExecutionMetrics>, anyhow::Error>(metrics)
            })
            .await??;

        if let Some(metrics) = metrics {
            // Record the metrics for the one-off query
            replica_ctx
                .relational_db
                .exec_counters_for(WorkloadType::Sql)
                .record(&metrics);
        }

        Ok(())
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
            inner: Arc::downgrade(&self.module),
            on_panic: Arc::downgrade(&self.on_panic),
            instance_manager: Arc::downgrade(&self.instance_manager),
            executor: self.executor.downgrade(),
            closed: Arc::downgrade(&self.closed),
        }
    }

    pub fn database_info(&self) -> &Database {
        &self.replica_ctx().database
    }

    pub fn durable_tx_offset(&self) -> Option<DurableOffset> {
        self.replica_ctx().relational_db.durable_tx_offset()
    }

    pub(crate) fn replica_ctx(&self) -> &ReplicaContext {
        self.module.replica_ctx()
    }
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        let inner = self.inner.upgrade()?;
        let on_panic = self.on_panic.upgrade()?;
        let instance_manager = self.instance_manager.upgrade()?;
        let executor = self.executor.upgrade()?;
        let closed = self.closed.upgrade()?;
        Some(ModuleHost {
            info: self.info.clone(),
            module: inner,
            on_panic,
            instance_manager,
            executor,
            closed,
        })
    }
}

fn no_such_function_log_message(function_kind: &str, function_name: &str) -> String {
    format!("External attempt to call nonexistent {function_kind} \"{function_name}\" failed. Have you run `spacetime generate` recently?")
}

fn args_error_log_message(function_kind: &str, function_name: &str) -> String {
    format!(
        "External attempt to call {function_kind} \"{function_name}\" failed, invalid arguments.\n\
         This is likely due to a mismatched client schema, have you run `spacetime generate` recently?"
    )
}
