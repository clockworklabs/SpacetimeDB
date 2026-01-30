use super::{
    ArgsTuple, FunctionArgs, InvalidProcedureArguments, InvalidReducerArguments, ReducerCallResult, ReducerId,
    ReducerOutcome, Scheduler,
};
use crate::client::messages::{OneOffQueryResponseMessage, SerializableMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_logger::{DatabaseLogger, LogLevel, Record};
use crate::db::relational_db::RelationalDB;
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::hash::Hash;
use crate::host::host_controller::CallProcedureReturn;
use crate::host::scheduler::{CallScheduledFunctionResult, ScheduledFunctionParams};
use crate::host::v8::JsInstance;
pub use crate::host::wasm_common::module_host_actor::{InstanceCommon, WasmInstance};
use crate::host::wasmtime::ModuleInstance;
use crate::host::{InvalidFunctionArguments, InvalidViewArguments};
use crate::identity::Identity;
use crate::messages::control_db::{Database, HostType};
use crate::replica_context::ReplicaContext;
use crate::sql::ast::SchemaViewer;
use crate::sql::execute::SqlResult;
use crate::sql::parser::RowLevelExpr;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::subscription::tx::DeltaTx;
use crate::subscription::websocket_building::{BuildableWebsocketFormat, RowListBuilderSource};
use crate::subscription::{execute_plan, execute_plan_for_view};
use crate::util::jobs::SingleCoreExecutor;
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use anyhow::Context;
use derive_more::From;
use futures::lock::Mutex;
use indexmap::IndexSet;
use itertools::Itertools;
use prometheus::{Histogram, IntGauge};
use scopeguard::ScopeGuard;
use smallvec::SmallVec;
use spacetimedb_auth::identity::ConnectionAuthCtx;
use spacetimedb_client_api_messages::energy::FunctionBudget;
use spacetimedb_client_api_messages::websocket::{
    ByteListLen, Compression, OneOffTable, QueryUpdate, Subscribe, SubscribeMulti, SubscribeSingle,
};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashSet};
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::execution_context::{Workload, WorkloadType};
use spacetimedb_datastore::locking_tx_datastore::{MutTxId, ViewCallInfo};
use spacetimedb_datastore::traits::{IsolationLevel, Program, TxData};
use spacetimedb_durability::DurableOffset;
use spacetimedb_execution::pipelined::{PipelinedProject, ViewProject};
use spacetimedb_expr::expr::CollectViews;
use spacetimedb_lib::db::raw_def::v9::Lifecycle;
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::{ConnectionId, Timestamp};
use spacetimedb_primitives::{ArgId, ProcedureId, TableId, ViewFnPtr, ViewId};
use spacetimedb_query::compile_subscription;
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductValue};
use spacetimedb_schema::auto_migrate::{AutoMigrateError, MigrationPolicy};
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, ViewDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::reducer_name::ReducerName;
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::table_name::TableName;
use spacetimedb_vm::relation::RelValue;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

#[derive(Debug, Default, Clone, From)]
pub struct DatabaseUpdate {
    pub tables: SmallVec<[DatabaseTableUpdate; 1]>,
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
        let entries = tx_data.iter_table_entries();
        let mut tables = SmallVec::with_capacity(entries.len());
        tables.extend(entries.map(|(table_id, e)| DatabaseTableUpdate {
            table_id,
            table_name: e.table_name.clone(),
            inserts: e.inserts.clone(),
            deletes: e.deletes.clone(),
        }));
        DatabaseUpdate { tables }
    }

    /// The number of rows in the payload
    pub fn num_rows(&self) -> usize {
        self.tables.iter().map(|t| t.inserts.len() + t.deletes.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseTableUpdate {
    pub table_id: TableId,
    pub table_name: TableName,
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
    pub table_name: TableName,
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

    pub fn encode<F: BuildableWebsocketFormat>(
        &self,
        rlb_pool: &impl RowListBuilderSource<F>,
    ) -> (F::QueryUpdate, u64, usize) {
        let (deletes, nr_del) = F::encode_list(rlb_pool.take_row_list_builder(), self.deletes.iter());
        let (inserts, nr_ins) = F::encode_list(rlb_pool.take_row_list_builder(), self.inserts.iter());
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
    pub reducer: ReducerName,
    pub reducer_id: ReducerId,
    pub args: ArgsTuple,
}

impl ModuleFunctionCall {
    pub fn update() -> Self {
        let reducer = ReducerName::new(Identifier::new_assume_valid(RawIdentifier::new("update")));
        Self {
            reducer,
            reducer_id: u32::MAX.into(),
            args: ArgsTuple::nullary(),
        }
    }
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
        subscriptions: ModuleSubscriptions,
    ) -> Arc<Self> {
        let metrics = ModuleMetrics::new(&database_identity);
        Arc::new(ModuleInfo {
            module_def,
            owner_identity,
            database_identity,
            module_hash,
            subscriptions,
            metrics,
        })
    }

    pub fn relational_db(&self) -> &Arc<RelationalDB> {
        self.subscriptions.relational_db()
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

pub enum ModuleWithInstance {
    Wasm {
        module: super::wasmtime::Module,
        executor: SingleCoreExecutor,
        init_inst: Box<super::wasmtime::ModuleInstance>,
    },
    Js {
        module: super::v8::JsModule,
        init_inst: super::v8::JsInstance,
    },
}

enum ModuleHostInner {
    Wasm(WasmtimeModuleHost),
    Js(V8ModuleHost),
}

struct WasmtimeModuleHost {
    executor: SingleCoreExecutor,
    instance_manager: ModuleInstanceManager<super::wasmtime::Module>,
}

struct V8ModuleHost {
    instance_manager: ModuleInstanceManager<super::v8::JsModule>,
}

/// A module; used as a bound on `InstanceManager`.
trait GenericModule {
    type Instance: GenericModuleInstance;
    async fn create_instance(&self) -> Self::Instance;
    fn host_type(&self) -> HostType;
}

trait GenericModuleInstance {
    fn trapped(&self) -> bool;
}

impl<T: WasmInstance> GenericModuleInstance for super::wasm_common::module_host_actor::WasmModuleInstance<T> {
    fn trapped(&self) -> bool {
        self.trapped()
    }
}

impl<T: GenericModuleInstance + ?Sized> GenericModuleInstance for Box<T> {
    fn trapped(&self) -> bool {
        (**self).trapped()
    }
}

impl GenericModule for super::wasmtime::Module {
    type Instance = Box<super::wasmtime::ModuleInstance>;
    async fn create_instance(&self) -> Self::Instance {
        Box::new(self.create_instance())
    }
    fn host_type(&self) -> HostType {
        HostType::Wasm
    }
}

impl GenericModule for super::v8::JsModule {
    type Instance = super::v8::JsInstance;
    async fn create_instance(&self) -> Self::Instance {
        self.create_instance().await
    }
    fn host_type(&self) -> HostType {
        HostType::Js
    }
}

impl GenericModuleInstance for super::v8::JsInstance {
    fn trapped(&self) -> bool {
        self.trapped()
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

/// Moves out the `trapped: bool` from `res`.
fn extract_trapped<T, E>(res: Result<(T, bool), E>) -> (Result<T, E>, bool) {
    match res {
        Ok((x, t)) => (Ok(x), t),
        Err(x) => (Err(x), false),
    }
}

/// If the module instance's `replica_ctx` is uninitialized, initialize it.
pub(crate) fn init_database(
    replica_ctx: &ReplicaContext,
    module_def: &ModuleDef,
    program: Program,
    call_reducer: impl FnOnce(Option<MutTxId>, CallReducerParams) -> (ReducerCallResult, bool),
) -> (anyhow::Result<Option<ReducerCallResult>>, bool) {
    extract_trapped(init_database_inner(replica_ctx, module_def, program, call_reducer))
}

fn init_database_inner(
    replica_ctx: &ReplicaContext,
    module_def: &ModuleDef,
    program: Program,
    call_reducer: impl FnOnce(Option<MutTxId>, CallReducerParams) -> (ReducerCallResult, bool),
) -> anyhow::Result<(Option<ReducerCallResult>, bool)> {
    log::debug!("init database");
    let timestamp = Timestamp::now();
    let stdb = &*replica_ctx.relational_db;
    let logger = replica_ctx.logger.system_logger();
    let owner_identity = replica_ctx.database.owner_identity;
    let host_type = replica_ctx.host_type;

    let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    let auth_ctx = AuthCtx::for_current(owner_identity);
    let (tx, ()) = stdb
        .with_auto_rollback(tx, |tx| {
            // Create all in-memory tables defined by the module,
            // with IDs ordered lexicographically by the table names.
            let mut table_defs: Vec<_> = module_def.tables().collect();
            table_defs.sort_by_key(|x| &x.name);
            for def in table_defs {
                logger.info(&format!("Creating table `{}`", &def.name));
                create_table_from_def(stdb, tx, module_def, def)?;
            }

            // Create all in-memory views defined by the module.
            let mut view_defs: Vec<_> = module_def.views().collect();
            view_defs.sort_by_key(|x| &x.name);
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

            stdb.set_initialized(tx, host_type, program)?;

            anyhow::Ok(())
        })
        .inspect_err(|e| log::error!("{e:?}"))?;

    let rcr = match module_def.lifecycle_reducer(Lifecycle::Init) {
        None => {
            if let Some((_tx_offset, tx_data, tx_metrics, reducer)) = stdb.commit_tx(tx)? {
                stdb.report_mut_tx_metrics(reducer, tx_metrics, Some(tx_data));
            }
            (None, false)
        }

        Some((reducer_id, _)) => {
            logger.info("Invoking `init` reducer");
            let params = CallReducerParams::from_system(timestamp, owner_identity, reducer_id, ArgsTuple::nullary());
            let (res, trapped) = call_reducer(Some(tx), params);
            (Some(res), trapped)
        }
    };

    logger.info("Database initialized");
    Ok(rcr)
}

pub fn call_identity_connected(
    caller_auth: ConnectionAuthCtx,
    caller_connection_id: ConnectionId,
    module: &ModuleInfo,
    call_reducer: impl FnOnce(Option<MutTxId>, CallReducerParams) -> (ReducerCallResult, bool),
    trapped_slot: &mut bool,
) -> Result<(), ClientConnectedError> {
    let reducer_lookup = module.module_def.lifecycle_reducer(Lifecycle::OnConnect);
    let stdb = module.relational_db();
    let workload = Workload::reducer_no_args(
        "call_identity_connected",
        caller_auth.claims.identity,
        caller_connection_id,
    );
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
        .map_err(DBError::from)
        .map_err(Box::new)?;

    if let Some((reducer_id, reducer_def)) = reducer_lookup {
        // The module defined a lifecycle reducer to handle new connections.
        // Call this reducer.
        // If the call fails (as in, something unexpectedly goes wrong with guest execution),
        // abort the connection: we can't really recover.
        let tx = Some(ScopeGuard::into_inner(mut_tx));
        let params = ModuleHost::call_reducer_params(
            module,
            caller_auth.claims.identity,
            Some(caller_connection_id),
            None,
            None,
            None,
            reducer_id,
            reducer_def,
            FunctionArgs::Nullary,
        )
        .map_err(ReducerCallError::from)?;
        let (reducer_outcome, trapped) = call_reducer(tx, params);
        *trapped_slot = trapped;

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
            ReducerOutcome::Failed(message) => Err(ClientConnectedError::Rejected(*message)),
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
                ClientConnectedError::DBError(e.into())
            })?;
        Ok(())
    }
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

impl CallReducerParams {
    /// Returns a set of parameters for an internal call
    /// without a client/caller/request_id.
    pub fn from_system(
        timestamp: Timestamp,
        caller_identity: Identity,
        reducer_id: ReducerId,
        args: ArgsTuple,
    ) -> Self {
        Self {
            timestamp,
            caller_identity,
            caller_connection_id: ConnectionId::ZERO,
            client: None,
            request_id: None,
            timer: None,
            reducer_id,
            args,
        }
    }
}

pub enum ViewCommand {
    AddSingleSubscription {
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        request: SubscribeSingle,
        timer: Instant,
    },
    AddMultiSubscription {
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        request: SubscribeMulti,
        timer: Instant,
    },
    AddLegacySubscription {
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        subscribe: Subscribe,
        timer: Instant,
    },
    Sql {
        db: Arc<RelationalDB>,
        sql_text: String,
        auth: AuthCtx,
        subs: Option<ModuleSubscriptions>,
    },
}

#[derive(Debug)]
pub enum ViewCommandResult {
    Subscription {
        result: Result<Option<ExecutionMetrics>, DBError>,
    },

    Sql {
        result: Result<SqlResult, DBError>,
        head: Vec<(RawIdentifier, AlgebraicType)>,
    },
}
pub struct CallViewParams {
    pub view_name: Identifier,
    pub view_id: ViewId,
    pub table_id: TableId,
    pub fn_ptr: ViewFnPtr,
    /// This is not always the same identity as `sender`.
    /// For subscribe and sql calls it will be.
    /// However for atomic view update after a reducer call,
    /// this will be the caller of the reducer.
    pub caller: Identity,
    pub sender: Option<Identity>,
    pub args: ArgsTuple,
    pub row_type: AlgebraicTypeRef,
    pub timestamp: Timestamp,
}

pub struct CallProcedureParams {
    pub timestamp: Timestamp,
    pub caller_identity: Identity,
    pub caller_connection_id: ConnectionId,
    pub timer: Option<Instant>,
    pub procedure_id: ProcedureId,
    pub args: ArgsTuple,
}

impl CallProcedureParams {
    /// Returns a set of parameters for an internal call
    /// without a client/caller/request_id.
    pub fn from_system(
        timestamp: Timestamp,
        caller_identity: Identity,
        procedure_id: ProcedureId,
        args: ArgsTuple,
    ) -> Self {
        Self {
            timestamp,
            caller_identity,
            caller_connection_id: ConnectionId::ZERO,
            timer: None,
            procedure_id,
            args,
        }
    }
}

/// Holds a [`Module`] and a set of [`Instance`]s from it,
/// and allocates the [`Instance`]s to be used for function calls.
///
/// Capable of managing and allocating multiple instances of the same module,
/// but this functionality is currently unused, as only one reducer runs at a time.
/// When we introduce procedures, it will be necessary to have multiple instances,
/// as each procedure invocation will have its own sandboxed instance,
/// and multiple procedures can run concurrently with up to one reducer.
struct ModuleInstanceManager<M: GenericModule> {
    instances: Mutex<VecDeque<M::Instance>>,
    module: M,
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

impl<M: GenericModule> ModuleInstanceManager<M> {
    fn new(module: M, init_inst: M::Instance, database_identity: Identity) -> Self {
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
            instances: Mutex::new(instances),
            module,
            create_instance_time_metric,
        }
    }

    async fn with_instance<R>(&self, f: impl AsyncFnOnce(M::Instance) -> (R, M::Instance)) -> R {
        let inst = self.get_instance().await;
        let (res, inst) = f(inst).await;
        self.return_instance(inst).await;
        res
    }

    async fn get_instance(&self) -> M::Instance {
        let inst = self.instances.lock().await.pop_back();
        if let Some(inst) = inst {
            inst
        } else {
            let start_time = std::time::Instant::now();
            let res = self.module.create_instance().await;
            let elapsed_time = start_time.elapsed();
            self.create_instance_time_metric.observe(elapsed_time);
            res
        }
    }

    async fn return_instance(&self, inst: M::Instance) {
        if inst.trapped() {
            // Don't return trapped instances;
            // they may have left internal data structures in the guest `Instance`
            // (WASM linear memory, V8 global scope) in a bad state.
            return;
        }

        self.instances.lock().await.push_front(inst);
    }
}

#[derive(Clone)]
pub struct ModuleHost {
    pub info: Arc<ModuleInfo>,
    inner: Arc<ModuleHostInner>,
    /// Called whenever a reducer call on this host panics.
    on_panic: Arc<dyn Fn() + Send + Sync + 'static>,

    /// Marks whether this module has been closed by [`Self::exit`].
    ///
    /// When this is true, most operations will fail with [`NoSuchModule`].
    closed: Arc<AtomicBool>,
}

impl fmt::Debug for ModuleHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleHost")
            .field("info", &self.info)
            .field("inner", &Arc::as_ptr(&self.inner))
            .finish()
    }
}

pub struct WeakModuleHost {
    info: Arc<ModuleInfo>,
    inner: Weak<ModuleHostInner>,
    on_panic: Weak<dyn Fn() + Send + Sync + 'static>,
    closed: Weak<AtomicBool>,
}

#[derive(Debug)]
pub enum UpdateDatabaseResult {
    NoUpdateNeeded,
    UpdatePerformed,
    UpdatePerformedWithClientDisconnect,
    AutoMigrateError(Box<ErrorStream<AutoMigrateError>>),
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

#[derive(Debug, PartialEq, Eq)]
pub enum ViewOutcome {
    Success,
    Failed(String),
    BudgetExceeded,
}

impl From<EventStatus> for ViewOutcome {
    fn from(status: EventStatus) -> Self {
        match status {
            EventStatus::Committed(_) => ViewOutcome::Success,
            EventStatus::Failed(e) => ViewOutcome::Failed(e),
            EventStatus::OutOfEnergy => ViewOutcome::BudgetExceeded,
        }
    }
}

pub struct ViewCallResult {
    pub outcome: ViewOutcome,
    pub tx: MutTxId,
    pub energy_used: FunctionBudget,
    pub total_duration: Duration,
    pub abi_duration: Duration,
}

impl fmt::Debug for ViewCallResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewCallResult")
            .field("outcome", &self.outcome)
            .field("energy_used", &self.energy_used)
            .field("total_duration", &self.total_duration)
            .field("abi_duration", &self.abi_duration)
            .finish()
    }
}

impl ViewCallResult {
    pub fn default(tx: MutTxId) -> Self {
        Self {
            outcome: ViewOutcome::Success,
            energy_used: FunctionBudget::ZERO,
            total_duration: Duration::ZERO,
            abi_duration: Duration::ZERO,
            tx,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ViewCallError {
    #[error(transparent)]
    Args(#[from] InvalidViewArguments),
    #[error(transparent)]
    NoSuchModule(#[from] NoSuchModule),
    #[error("no such view")]
    NoSuchView,
    #[error("Table does not exist for view `{0}`")]
    TableDoesNotExist(ViewId),
    #[error("missing client connection for view call trigged by subscription")]
    MissingClientConnection,
    #[error("DB error during view call: {0}")]
    DatastoreError(#[from] DatastoreError),
    #[error("The module instance encountered a fatal error: {0}")]
    InternalError(String),
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
    DBError(#[from] Box<DBError>),
    #[error("Connection rejected by `client_connected` reducer: {0}")]
    Rejected(Box<str>),
    #[error("Insufficient energy balance to run `client_connected` reducer")]
    OutOfEnergy,
}

pub struct RefInstance<'a, I: WasmInstance> {
    pub common: &'a mut InstanceCommon,
    pub instance: &'a mut I,
}

impl ModuleHost {
    pub(super) fn new(
        module: ModuleWithInstance,
        on_panic: impl Fn() + Send + Sync + 'static,
        database_identity: Identity,
    ) -> Self {
        let info;
        let inner = match module {
            ModuleWithInstance::Wasm {
                module,
                executor,
                init_inst,
            } => {
                info = module.info();
                let instance_manager = ModuleInstanceManager::new(module, init_inst, database_identity);
                Arc::new(ModuleHostInner::Wasm(WasmtimeModuleHost {
                    executor,
                    instance_manager,
                }))
            }
            ModuleWithInstance::Js { module, init_inst } => {
                info = module.info();
                let instance_manager = ModuleInstanceManager::new(module, init_inst, database_identity);
                Arc::new(ModuleHostInner::Js(V8ModuleHost { instance_manager }))
            }
        };
        let on_panic = Arc::new(on_panic);

        ModuleHost {
            info,
            inner,
            on_panic,
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
        self.on_module_thread_async(label, async move || f()).await
    }

    /// Run an async function on the JobThread for this module.
    /// Similar to `on_module_thread`, but for async functions.
    pub async fn on_module_thread_async<F, R>(&self, label: &str, f: F) -> Result<R, anyhow::Error>
    where
        F: AsyncFnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.guard_closed()?;

        let timer_guard = self.start_call_timer(label);

        Ok(match &*self.inner {
            ModuleHostInner::Wasm(WasmtimeModuleHost { executor, .. }) => {
                executor
                    .run_job(async move || {
                        drop(timer_guard);
                        f().await
                    })
                    .await
            }
            ModuleHostInner::Js(V8ModuleHost { instance_manager }) => {
                instance_manager
                    .with_instance(async |mut inst| {
                        let res = inst
                            .run_on_thread(async move || {
                                drop(timer_guard);
                                f().await
                            })
                            .await;
                        (res, inst)
                    })
                    .await
            }
        })
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

    /// Run a function for this module which has access to the module instance.
    async fn with_instance<Guard, A, R>(
        &self,
        kind: &str,
        label: &str,
        arg: A,
        timer: impl FnOnce(&str) -> Guard,
        work_wasm: impl AsyncFnOnce(Guard, &SingleCoreExecutor, Box<ModuleInstance>, A) -> (R, Box<ModuleInstance>),
        work_js: impl AsyncFnOnce(Guard, &mut JsInstance, A) -> R,
    ) -> Result<R, NoSuchModule> {
        self.guard_closed()?;
        let timer_guard = timer(label);

        // Operations on module instances (e.g. calling reducers) is blocking,
        // partially because the computation can potentially take a long time
        // and partially because interacting with the database requires taking
        // a blocking lock. So, we run `f` on a dedicated thread with `self.executor`.
        // This will bubble up any panic that may occur.

        // If a function call panics, we **must** ensure to call `self.on_panic`
        // so that the module is discarded by the host controller.
        scopeguard::defer_on_unwind!({
            log::warn!("{kind} {label} panicked");
            (self.on_panic)();
        });

        Ok(match &*self.inner {
            ModuleHostInner::Wasm(WasmtimeModuleHost {
                executor,
                instance_manager,
            }) => {
                instance_manager
                    .with_instance(async |inst| work_wasm(timer_guard, executor, inst, arg).await)
                    .await
            }
            ModuleHostInner::Js(V8ModuleHost { instance_manager }) => {
                instance_manager
                    .with_instance(async |mut inst| (work_js(timer_guard, &mut inst, arg).await, inst))
                    .await
            }
        })
    }

    /// Run a function for this module which has access to the module instance.
    ///
    /// For WASM, the function is run on the module's JobThread.
    /// For V8/JS, the function is run in the current task.
    async fn call<A, R>(
        &self,
        label: &str,
        arg: A,
        wasm: impl AsyncFnOnce(A, &mut ModuleInstance) -> R + Send + 'static,
        js: impl AsyncFnOnce(A, &mut JsInstance) -> R,
    ) -> Result<R, NoSuchModule>
    where
        R: Send + 'static,
        A: Send + 'static,
    {
        self.with_instance(
            "reducer",
            label,
            arg,
            |l| self.start_call_timer(l),
            // Operations on module instances (e.g. calling reducers) is blocking,
            // partially because the computation can potentially take a long time
            // and partially because interacting with the database requires taking a blocking lock.
            // So, we run `work` on a dedicated thread with `self.executor`.
            // This will bubble up any panic that may occur.
            async move |timer_guard, executor, mut inst, arg| {
                executor
                    .run_job(async move || {
                        drop(timer_guard);
                        (wasm(arg, &mut inst).await, inst)
                    })
                    .await
            },
            async move |timer_guard, inst, arg| {
                drop(timer_guard);
                js(arg, inst).await
            },
        )
        .await
    }

    pub async fn disconnect_client(&self, client_id: ClientActorId) {
        log::trace!("disconnecting client {client_id}");
        if let Err(e) = self
            .call(
                "disconnect_client",
                client_id,
                async |client_id, inst| inst.disconnect_client(client_id),
                async |client_id, inst| inst.disconnect_client(client_id).await,
            )
            .await
        {
            log::error!("Error from client_disconnected transaction: {e}");
        }
    }

    pub fn disconnect_client_inner(
        client_id: ClientActorId,
        info: &ModuleInfo,
        call_reducer: impl FnOnce(Option<MutTxId>, CallReducerParams) -> (ReducerCallResult, bool),
        trapped_slot: &mut bool,
    ) -> Result<(), ReducerCallError> {
        // Call the `client_disconnected` reducer, if it exists.
        // This is a no-op if the module doesn't define such a reducer.
        info.subscriptions.remove_subscriber(client_id);
        Self::call_identity_disconnected_inner(
            client_id.identity,
            client_id.connection_id,
            info,
            true,
            call_reducer,
            trapped_slot,
        )
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
        self.call(
            "call_identity_connected",
            (caller_auth, caller_connection_id),
            async |(a, b), inst| inst.call_identity_connected(a, b),
            async |(a, b), inst| inst.call_identity_connected(a, b).await,
        )
        .await
        .map_err(ReducerCallError::from)?
    }

    /// Invokes the `client_disconnected` reducer, if present,
    /// then deletes the client’s rows from `st_client` and `st_connection_credentials`.
    /// If the reducer fails, the rows are still deleted.
    /// Calling this on an already-disconnected client is a no-op.
    pub fn call_identity_disconnected_inner(
        caller_identity: Identity,
        caller_connection_id: ConnectionId,
        info: &ModuleInfo,
        drop_view_subscribers: bool,
        call_reducer: impl FnOnce(Option<MutTxId>, CallReducerParams) -> (ReducerCallResult, bool),
        trapped_slot: &mut bool,
    ) -> Result<(), ReducerCallError> {
        let stdb = info.relational_db();

        let reducer_lookup = info.module_def.lifecycle_reducer(Lifecycle::OnDisconnect);
        let reducer_name = reducer_lookup
            .as_ref()
            .map(|(_, def)| &*def.name)
            .unwrap_or("__identity_disconnected__");

        let is_client_exist = |mut_tx: &MutTxId| mut_tx.st_client_row(caller_identity, caller_connection_id).is_some();

        let workload = || Workload::reducer_no_args(reducer_name, caller_identity, caller_connection_id);

        // Decrement the number of subscribers for each view this caller is subscribed to
        let dec_view_subscribers = |tx: &mut MutTxId| {
            if drop_view_subscribers {
                if let Err(err) = tx.unsubscribe_views(caller_identity) {
                    log::error!("`call_identity_disconnected`: failed to delete client view data: {err}");
                }
            }
        };

        // A fallback transaction that deletes the client from `st_client`.
        let database_identity = stdb.database_identity();
        let fallback = || {
            stdb.with_auto_commit(workload(), |mut_tx| {

                dec_view_subscribers(mut_tx);

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
            let mut mut_tx = stdb.begin_mut_tx(IsolationLevel::Serializable, workload());

            dec_view_subscribers(&mut mut_tx);

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
            let tx = Some(mut_tx);
            let result = Self::call_reducer_params(
                info,
                caller_identity,
                Some(caller_connection_id),
                None,
                None,
                None,
                reducer_id,
                reducer_def,
                FunctionArgs::Nullary,
            )
            .map(|params| {
                let (res, trapped) = call_reducer(tx, params);
                *trapped_slot = trapped;
                res
            });

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
        drop_view_subscribers: bool,
    ) -> Result<(), ReducerCallError> {
        self.call(
            "call_identity_disconnected",
            (caller_identity, caller_connection_id, drop_view_subscribers),
            async |(a, b, c), inst| inst.call_identity_disconnected(a, b, c),
            async |(a, b, c), inst| inst.call_identity_disconnected(a, b, c).await,
        )
        .await?
    }

    /// Empty the system tables tracking clients without running any lifecycle reducers.
    pub async fn clear_all_clients(&self) -> anyhow::Result<()> {
        self.call(
            "clear_all_clients",
            (),
            async |_, inst| inst.clear_all_clients(),
            async |_, inst| inst.clear_all_clients().await,
        )
        .await?
    }

    fn call_reducer_params(
        module: &ModuleInfo,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_id: ReducerId,
        reducer_def: &ReducerDef,
        args: FunctionArgs,
    ) -> Result<CallReducerParams, InvalidReducerArguments> {
        let args = args
            .into_tuple_for_def(&module.module_def, reducer_def)
            .map_err(InvalidReducerArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);
        Ok(CallReducerParams {
            timestamp: Timestamp::now(),
            caller_identity,
            caller_connection_id,
            client,
            request_id,
            timer,
            reducer_id,
            args,
        })
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
        let args = args
            .into_tuple_for_def(&self.info.module_def, reducer_def)
            .map_err(InvalidReducerArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);
        let call_reducer_params = CallReducerParams {
            timestamp: Timestamp::now(),
            caller_identity,
            caller_connection_id,
            client,
            request_id,
            timer,
            reducer_id,
            args,
        };

        Ok(self
            .call(
                &reducer_def.name,
                call_reducer_params,
                async |p, inst| inst.call_reducer(p),
                async |p, inst| inst.call_reducer(p).await,
            )
            .await?)
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

    pub async fn call_view_add_single_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        request: SubscribeSingle,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let cmd = ViewCommand::AddSingleSubscription {
            sender,
            auth,
            request,
            timer,
        };

        let res = self
            .call(
                "call_view_add_single_subscription",
                cmd,
                async |cmd, inst| inst.call_view(cmd),
                async |cmd, inst| inst.call_view(cmd).await,
            )
            .await
            //TODO: handle error better
            .map_err(|e| DBError::Other(anyhow::anyhow!(e)))?;

        match res {
            ViewCommandResult::Subscription { result } => result,
            ViewCommandResult::Sql { .. } => {
                unreachable!("unexpected SQL result in call_view_add_single_subscription")
            }
        }
    }

    pub async fn call_view_add_multi_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        request: SubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let cmd = ViewCommand::AddMultiSubscription {
            sender,
            auth,
            request,
            timer,
        };

        let res = self
            .call(
                "call_view_add_multi_subscription",
                cmd,
                async |cmd, inst| inst.call_view(cmd),
                async |cmd, inst| inst.call_view(cmd).await,
            )
            .await
            //TODO: handle error better
            .map_err(|e| DBError::Other(anyhow::anyhow!(e)))?;

        match res {
            ViewCommandResult::Subscription { result } => result,
            ViewCommandResult::Sql { .. } => {
                unreachable!("unexpected SQL result in call_view_add_single_subscription")
            }
        }
    }

    pub async fn call_view_add_legacy_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        auth: AuthCtx,
        subscribe: spacetimedb_client_api_messages::websocket::Subscribe,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let cmd = ViewCommand::AddLegacySubscription {
            sender,
            auth,
            subscribe,
            timer,
        };

        let res = self
            .call(
                "call_view_add_legacy_subscription",
                cmd,
                async |cmd, inst| inst.call_view(cmd),
                async |cmd, inst| inst.call_view(cmd).await,
            )
            .await
            //TODO: handle error better
            .map_err(|e| DBError::Other(anyhow::anyhow!(e)))?;

        match res {
            ViewCommandResult::Subscription { result } => result,
            ViewCommandResult::Sql { .. } => {
                unreachable!("unexpected SQL result in call_view_add_single_subscription")
            }
        }
    }

    pub async fn call_view_sql(
        &self,
        db: Arc<RelationalDB>,
        sql_text: String,
        auth: AuthCtx,
        subs: Option<ModuleSubscriptions>,
        head: &mut Vec<(RawIdentifier, AlgebraicType)>,
    ) -> Result<SqlResult, DBError> {
        let cmd = ViewCommand::Sql {
            db,
            sql_text,
            auth,
            subs,
        };

        let res = self
            .call(
                "call_view_sql",
                cmd,
                async |cmd, inst| inst.call_view(cmd),
                async |cmd, inst| inst.call_view(cmd).await,
            )
            .await
            //TODO: handle error better
            .map_err(|e| DBError::Other(anyhow::anyhow!(e)))?;

        match res {
            ViewCommandResult::Sql { result, head: new_head } => {
                *head = new_head;
                result
            }
            ViewCommandResult::Subscription { .. } => {
                unreachable!("unexpected subscription result in call_view_sql")
            }
        }
    }

    pub async fn call_procedure(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        timer: Option<Instant>,
        procedure_name: &str,
        args: FunctionArgs,
    ) -> CallProcedureReturn {
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

        let ret = match res {
            Ok(ret) => ret,
            Err(err) => CallProcedureReturn {
                result: Err(err),
                tx_offset: None,
            },
        };

        let log_message = match &ret.result {
            Err(ProcedureCallError::NoSuchProcedure) => Some(no_such_function_log_message("procedure", procedure_name)),
            Err(ProcedureCallError::Args(_)) => Some(args_error_log_message("procedure", procedure_name)),
            _ => None,
        };

        if let Some(log_message) = log_message {
            self.inject_logs(LogLevel::Error, procedure_name, &log_message)
        }

        ret
    }

    async fn call_procedure_inner(
        &self,
        caller_identity: Identity,
        caller_connection_id: Option<ConnectionId>,
        timer: Option<Instant>,
        procedure_id: ProcedureId,
        procedure_def: &ProcedureDef,
        args: FunctionArgs,
    ) -> Result<CallProcedureReturn, ProcedureCallError> {
        let args = args
            .into_tuple_for_def(&self.info.module_def, procedure_def)
            .map_err(InvalidProcedureArguments)?;
        let caller_connection_id = caller_connection_id.unwrap_or(ConnectionId::ZERO);

        let params = CallProcedureParams {
            timestamp: Timestamp::now(),
            caller_identity,
            caller_connection_id,
            timer,
            procedure_id,
            args,
        };

        Ok(self.call_procedure_with_params(&procedure_def.name, params).await?)
    }

    pub async fn call_procedure_with_params(
        &self,
        name: &str,
        params: CallProcedureParams,
    ) -> Result<CallProcedureReturn, NoSuchModule> {
        self.call(
            name,
            params,
            async move |params, inst| inst.call_procedure(params).await,
            async move |params, inst| inst.call_procedure(params).await,
        )
        .await
    }

    pub(super) async fn call_scheduled_function(
        &self,
        params: ScheduledFunctionParams,
    ) -> Result<CallScheduledFunctionResult, NoSuchModule> {
        self.call(
            "unknown scheduled function",
            params,
            async move |params, inst| inst.call_scheduled_function(params).await,
            async move |params, inst| inst.call_scheduled_function(params).await,
        )
        .await
    }

    /// Materializes the views return by the `view_collector`, if not already materialized,
    /// and updates `st_view_sub` accordingly.
    ///
    /// Passing [`Workload::Sql`] will update `st_view_sub.last_called`.
    /// Passing [`Workload::Subscribe`] will also increment `st_view_sub.num_subscribers`,
    /// in addition to updating `st_view_sub.last_called`.
    pub fn materialize_views<I: WasmInstance>(
        mut tx: MutTxId,
        instance: &mut RefInstance<'_, I>,
        view_collector: &impl CollectViews,
        caller: Identity,
        workload: Workload,
    ) -> Result<(MutTxId, bool), ViewCallError> {
        use FunctionArgs::*;
        let mut view_ids = HashSet::new();
        view_collector.collect_views(&mut view_ids);
        for view_id in view_ids {
            let st_view_row = tx.lookup_st_view(view_id)?;
            let view_name = st_view_row.view_name.into_identifier();
            let view_id = st_view_row.view_id;
            let table_id = st_view_row.table_id.ok_or(ViewCallError::TableDoesNotExist(view_id))?;
            let is_anonymous = st_view_row.is_anonymous;
            let sender = if is_anonymous { None } else { Some(caller) };
            if !tx.is_view_materialized(view_id, ArgId::SENTINEL, caller)? {
                let (res, trapped) =
                    Self::call_view(instance, tx, &view_name, view_id, table_id, Nullary, caller, sender)?;
                tx = res.tx;
                if trapped {
                    return Ok((tx, true));
                }
            }
            // If this is a sql call, we only update this view's "last called" timestamp
            if let Workload::Sql = workload {
                tx.update_view_timestamp(view_id, ArgId::SENTINEL, caller)?;
            }
            // If this is a subscribe call, we also increment this view's subscriber count
            if let Workload::Subscribe = workload {
                tx.subscribe_view(view_id, ArgId::SENTINEL, caller)?;
            }
        }
        Ok((tx, false))
    }

    pub fn call_views_with_tx<I: WasmInstance>(
        tx: MutTxId,
        instance: &mut RefInstance<'_, I>,
        caller: Identity,
    ) -> Result<(ViewCallResult, bool), ViewCallError> {
        let mut out = ViewCallResult::default(tx);
        let module_def = &instance.common.info().module_def;
        let mut trapped = false;
        use FunctionArgs::Nullary;
        for ViewCallInfo {
            view_id,
            table_id,
            fn_ptr,
            sender,
        } in out.tx.view_for_update().cloned().collect::<Vec<_>>()
        {
            let view_def = module_def
                .get_view_by_id(fn_ptr, sender.is_none())
                .ok_or(ViewCallError::NoSuchView)?;

            let (result, trap) = Self::call_view(
                instance,
                out.tx,
                &view_def.name,
                view_id,
                table_id,
                Nullary,
                caller,
                sender,
            )?;

            // Increment execution stats
            out.tx = result.tx;
            out.outcome = result.outcome;
            out.energy_used += result.energy_used;
            out.total_duration += result.total_duration;
            out.abi_duration += result.abi_duration;
            trapped |= trap;

            // Terminate early if execution failed
            if !matches!(out.outcome, ViewOutcome::Success) || trapped {
                break;
            }
        }
        Ok((out, trapped))
    }

    fn call_view<I: WasmInstance>(
        instance: &mut RefInstance<'_, I>,
        tx: MutTxId,
        view_name: &Identifier,
        view_id: ViewId,
        table_id: TableId,
        args: FunctionArgs,
        caller: Identity,
        sender: Option<Identity>,
    ) -> Result<(ViewCallResult, bool), ViewCallError> {
        let module_def = &instance.common.info().module_def;
        let view_def = module_def.view(view_name).ok_or(ViewCallError::NoSuchView)?;
        let fn_ptr = view_def.fn_ptr;
        let row_type = view_def.product_type_ref;
        let args = args
            .into_tuple_for_def(module_def, view_def)
            .map_err(InvalidViewArguments)?;

        match Self::call_view_inner(
            instance, tx, view_name, view_id, table_id, fn_ptr, caller, sender, args, row_type,
        ) {
            err @ Err(ViewCallError::NoSuchView) => {
                let _log_message = no_such_function_log_message("view", view_name);
                //   self.inject_logs(LogLevel::Error, view_name, &log_message);
                err
            }
            err @ Err(ViewCallError::Args(_)) => {
                let _log_message = args_error_log_message("view", view_name);
                // self.inject_logs(LogLevel::Error, view_name, &log_message);
                err
            }
            res => res,
        }
    }

    fn call_view_inner<I: WasmInstance>(
        instance: &mut RefInstance<'_, I>,
        tx: MutTxId,
        name: &Identifier,
        view_id: ViewId,
        table_id: TableId,
        fn_ptr: ViewFnPtr,
        caller: Identity,
        sender: Option<Identity>,
        args: ArgsTuple,
        row_type: AlgebraicTypeRef,
    ) -> Result<(ViewCallResult, bool), ViewCallError> {
        let view_name = name.clone();
        let params = CallViewParams {
            timestamp: Timestamp::now(),
            view_name,
            view_id,
            table_id,
            fn_ptr,
            caller,
            sender,
            args,
            row_type,
        };

        Ok(instance.common.call_view_with_tx(tx, params, instance.instance))
    }

    pub async fn init_database(&self, program: Program) -> Result<Option<ReducerCallResult>, InitDatabaseError> {
        self.call(
            "<init_database>",
            program,
            async |p, inst| inst.init_database(p),
            async |p, inst| inst.init_database(p).await,
        )
        .await?
        .map_err(InitDatabaseError::Other)
    }

    pub async fn update_database(
        &self,
        program: Program,
        old_module_info: Arc<ModuleInfo>,
        policy: MigrationPolicy,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.call(
            "<update_database>",
            (program, old_module_info, policy),
            async |(a, b, c), inst| inst.update_database(a, b, c),
            async |(a, b, c), inst| inst.update_database(a, b, c).await,
        )
        .await?
    }

    pub async fn exit(&self) {
        // As in `Self::marked_closed`, `Relaxed` is sufficient because we're not synchronizing any external state.
        self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
        self.scheduler().close();
        self.exited().await;
    }

    pub async fn exited(&self) {
        self.scheduler().closed().await;
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
        auth: AuthCtx,
        query: String,
        client: Arc<ClientConnectionSender>,
        message_id: Vec<u8>,
        timer: Instant,
        rlb_pool: impl 'static + Send + RowListBuilderSource<F>,
        // We take this because we only have a way to convert with the concrete types (Bsatn and Json)
        into_message: impl FnOnce(OneOffQueryResponseMessage<F>) -> SerializableMessage + Send + 'static,
    ) -> Result<(), anyhow::Error> {
        let replica_ctx = self.replica_ctx();
        let db = replica_ctx.relational_db.clone();
        let subscriptions = replica_ctx.subscriptions.clone();
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
                        .map(|plan| plan.optimize(&auth))
                        .collect::<Result<Vec<_>, _>>()?;

                    check_row_limit(
                        &optimized,
                        &db,
                        &tx,
                        // Estimate the number of rows this query will scan
                        |plan, tx| estimate_rows_scanned(tx, plan),
                        &auth,
                    )?;

                    let return_table = || optimized.first().and_then(|plan| plan.return_table());

                    let returns_view_table = optimized.first().is_some_and(|plan| plan.returns_view_table());
                    let num_cols = return_table().map(|schema| schema.num_cols()).unwrap_or_default();
                    let num_private_cols = return_table()
                        .map(|schema| schema.num_private_cols())
                        .unwrap_or_default();

                    let optimized = optimized
                        .into_iter()
                        // Convert into something we can execute
                        .map(PipelinedProject::from)
                        .collect::<Vec<_>>();

                    let table_name = table_name.to_boxed_str();

                    if returns_view_table && num_private_cols > 0 {
                        let optimized = optimized
                            .into_iter()
                            .map(|plan| ViewProject::new(plan, num_cols, num_private_cols))
                            .collect::<Vec<_>>();
                        // Execute the union and return the results
                        return execute_plan_for_view::<F>(&optimized, &DeltaTx::from(&*tx), &rlb_pool)
                            .map(|(rows, _, metrics)| (OneOffTable { table_name, rows }, metrics))
                            .context("One-off queries are not allowed to modify the database");
                    }

                    // Execute the union and return the results
                    execute_plan::<F>(&optimized, &DeltaTx::from(&*tx), &rlb_pool)
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
            inner: Arc::downgrade(&self.inner),
            on_panic: Arc::downgrade(&self.on_panic),
            closed: Arc::downgrade(&self.closed),
        }
    }

    pub fn database_info(&self) -> &Database {
        &self.replica_ctx().database
    }

    pub fn durable_tx_offset(&self) -> Option<DurableOffset> {
        self.replica_ctx().relational_db.durable_tx_offset()
    }

    pub fn database_logger(&self) -> &Arc<DatabaseLogger> {
        &self.replica_ctx().logger
    }

    pub(crate) fn replica_ctx(&self) -> &ReplicaContext {
        match &*self.inner {
            ModuleHostInner::Wasm(wasm) => wasm.instance_manager.module.replica_ctx(),
            ModuleHostInner::Js(js) => js.instance_manager.module.replica_ctx(),
        }
    }

    fn scheduler(&self) -> &Scheduler {
        match &*self.inner {
            ModuleHostInner::Wasm(wasm) => wasm.instance_manager.module.scheduler(),
            ModuleHostInner::Js(js) => js.instance_manager.module.scheduler(),
        }
    }
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        let inner = self.inner.upgrade()?;
        let on_panic = self.on_panic.upgrade()?;
        let closed = self.closed.upgrade()?;
        Some(ModuleHost {
            info: self.info.clone(),
            inner,
            on_panic,
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
