use super::wasm_common::{CLIENT_CONNECTED_DUNDER, CLIENT_DISCONNECTED_DUNDER};
use super::{ArgsTuple, InvalidReducerArguments, ReducerArgs, ReducerCallResult, ReducerId};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::{LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::system_tables::{StClientFields, StClientRow, ST_CLIENT_ID};
use crate::db::datastore::traits::{IsolationLevel, Program, TxData};
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::execution_context::{ExecutionContext, ReducerContext};
use crate::hash::Hash;
use crate::identity::Identity;
use crate::messages::control_db::Database;
use crate::sql;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::util::lending_pool::{Closed, LendingPool, LentResource, PoolClosed};
use crate::worker_metrics::WORKER_METRICS;
use anyhow::Context;
use bytes::Bytes;
use derive_more::From;
use futures::{Future, FutureExt};
use indexmap::IndexSet;
use itertools::Itertools;
use smallvec::SmallVec;
use spacetimedb_client_api_messages::timestamp::Timestamp;
use spacetimedb_client_api_messages::websocket::{QueryUpdate, WebsocketFormat};
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::{HashCollectionExt as _, IntMap};
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::Address;
use spacetimedb_primitives::{col_list, TableId};
use spacetimedb_sats::{algebraic_value, ProductValue};
use spacetimedb_schema::auto_migrate::AutoMigrateError;
use spacetimedb_schema::def::deserialize::ReducerArgsDeserializeSeed;
use spacetimedb_schema::def::{ModuleDef, ReducerDef};
use spacetimedb_vm::relation::{MemTable, RelValue};
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

    pub fn encode<F: WebsocketFormat>(&self) -> QueryUpdate<F> {
        let deletes = F::encode_list(self.deletes.iter());
        let inserts = F::encode_list(self.inserts.iter());
        QueryUpdate { deletes, inserts }
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
    pub caller_address: Option<Address>,
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
    /// Map between reducer IDs and reducer names.
    /// Reducer names are sorted alphabetically.
    pub reducers_map: ReducersMap,
    /// The identity of the module.
    pub identity: Identity,
    /// The address of the module.
    pub address: Address,
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
        identity: Identity,
        address: Address,
        module_hash: Hash,
        log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
        subscriptions: ModuleSubscriptions,
    ) -> Arc<Self> {
        // Note: sorts alphabetically!
        let reducers_map = module_def.reducers().map(|r| &*r.name).collect();
        Arc::new(ModuleInfo {
            module_def,
            reducers_map,
            identity,
            address,
            module_hash,
            log_tx,
            subscriptions,
        })
    }

    /// Get the reducer seed and ID for a reducer name, if any.
    pub fn reducer_seed_and_id(&self, reducer_name: &str) -> Option<(ReducerArgsDeserializeSeed, ReducerId)> {
        let seed = self.module_def.reducer_arg_deserialize_seed(reducer_name)?;
        let reducer_id = self
            .reducers_map
            .lookup_id(reducer_name)
            .expect("seed was present, so ID should be present!");
        Some((seed, reducer_id))
    }

    /// Get a reducer by its ID.
    pub fn get_reducer_by_id(&self, reducer_id: ReducerId) -> Option<&ReducerDef> {
        let name = self.reducers_map.lookup_name(reducer_id)?;
        Some(
            self.module_def
                .reducer(name)
                .expect("id was present, so reducer should be present!"),
        )
    }
}

/// A bidirectional map between `Identifiers` (reducer names) and `ReducerId`s.
/// Invariant: the reducer names are in alphabetical order.
pub struct ReducersMap(IndexSet<Box<str>>);

impl<'a> FromIterator<&'a str> for ReducersMap {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut sorted = Vec::from_iter(iter);
        sorted.sort();
        ReducersMap(sorted.into_iter().map_into().collect())
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
    fn dbic(&self) -> &DatabaseInstanceContext;
    fn close(self);
    #[cfg(feature = "tracelogging")]
    fn get_trace(&self) -> Option<bytes::Bytes>;
    #[cfg(feature = "tracelogging")]
    fn stop_trace(&self) -> anyhow::Result<()>;
}

pub trait ModuleInstance: Send + 'static {
    fn trapped(&self) -> bool;

    /// If the module instance's dbic is uninitialized, initialize it.
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
    pub caller_address: Address,
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
    async fn get_instance(&self, db: Address) -> Result<Box<dyn ModuleInstance>, NoSuchModule>;
    fn dbic(&self) -> &DatabaseInstanceContext;
    fn exit(&self) -> Closed<'_>;
    fn exited(&self) -> Closed<'_>;
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
    async fn get_instance(&self, db: Address) -> Result<Box<dyn ModuleInstance>, NoSuchModule> {
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

    fn dbic(&self) -> &DatabaseInstanceContext {
        self.module.dbic()
    }

    fn exit(&self) -> Closed<'_> {
        self.instance_pool.close()
    }

    fn exited(&self) -> Closed<'_> {
        self.instance_pool.closed()
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
                .with_label_values(&self.info.address, reducer)
                .start_timer();
            self.inner.get_instance(self.info.address).await?
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
        let this = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            this.subscriptions().remove_subscriber(client_id);
        })
        .await;
        // ignore NoSuchModule; if the module's already closed, that's fine
        let _ = self
            .call_identity_connected_disconnected(client_id.identity, client_id.address, false)
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
        caller_address: Address,
        connected: bool,
    ) -> Result<(), ReducerCallError> {
        let reducer_name = if connected {
            CLIENT_CONNECTED_DUNDER
        } else {
            CLIENT_DISCONNECTED_DUNDER
        };

        let db = &self.inner.dbic().relational_db;
        let ctx = || {
            ExecutionContext::reducer(
                db.address(),
                ReducerContext {
                    name: reducer_name.to_owned(),
                    caller_identity,
                    caller_address,
                    timestamp: Timestamp::now(),
                    arg_bsatn: Bytes::new(),
                },
            )
        };

        let result = self
            .call_reducer_inner(
                caller_identity,
                Some(caller_address),
                None,
                None,
                None,
                reducer_name,
                ReducerArgs::Nullary,
            )
            .await
            .map(drop)
            .or_else(|e| match e {
                // If the module doesn't define connected or disconnected, commit
                // a transaction to update `st_clients` and to ensure we always have those events
                // paired in the commitlog.
                //
                // This is necessary to be able to disconnect clients after a server
                // crash.
                ReducerCallError::NoSuchReducer => db
                    .with_auto_commit(&ctx(), |mut_tx| {
                        if connected {
                            self.update_st_clients(mut_tx, caller_identity, caller_address, connected)
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
                    }),
                e => Err(e),
            });

        // Deleting client from `st_clients`does not depend upon result of disconnect reducer hence done in a separate tx.
        if !connected {
            let _ = db
                .with_auto_commit(&ctx(), |mut_tx| {
                    self.update_st_clients(mut_tx, caller_identity, caller_address, connected)
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
        caller_address: Address,
        connected: bool,
    ) -> Result<(), DBError> {
        let db = &*self.inner.dbic().relational_db;
        let ctx = &ExecutionContext::internal(db.address());
        let row = &StClientRow {
            identity: caller_identity.into(),
            address: caller_address.into(),
        };

        if connected {
            db.insert(mut_tx, ST_CLIENT_ID, row.into()).map(|_| ())
        } else {
            let row = db
                .iter_by_col_eq_mut(
                    ctx,
                    mut_tx,
                    ST_CLIENT_ID,
                    col_list![StClientFields::Identity, StClientFields::Address],
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
        caller_address: Option<Address>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let reducer_seed = self
            .info
            .module_def
            .reducer_arg_deserialize_seed(reducer_name)
            .ok_or(ReducerCallError::NoSuchReducer)?;

        let reducer_id = self
            .info
            .reducers_map
            .lookup_id(reducer_name)
            .expect("if we found the seed, we should find the ID!");

        let args = args.into_tuple(reducer_seed)?;
        let caller_address = caller_address.unwrap_or(Address::__DUMMY);

        self.call(reducer_name, move |inst| {
            inst.call_reducer(
                None,
                CallReducerParams {
                    timestamp: Timestamp::now(),
                    caller_identity,
                    caller_address,
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
        caller_address: Option<Address>,
        client: Option<Arc<ClientConnectionSender>>,
        request_id: Option<RequestId>,
        timer: Option<Instant>,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        if reducer_name.starts_with("__") && reducer_name.ends_with("__") {
            return Err(ReducerCallError::NoSuchReducer);
        }
        let res = self
            .call_reducer_inner(
                caller_identity,
                caller_address,
                client,
                request_id,
                timer,
                reducer_name,
                args,
            )
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
        let db = self.inner.dbic().relational_db.clone();
        // scheduled reducer name not fetched yet, anyway this is only for logging purpose
        const REDUCER: &str = "scheduled_reducer";
        self.call(REDUCER, move |inst: &mut dyn ModuleInstance| {
            let mut tx = db.begin_mut_tx(IsolationLevel::Serializable);

            match call_reducer_params(&mut tx) {
                Ok(Some(params)) => Ok(inst.call_reducer(Some(tx), params)),
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
        self.dbic().logger.write(
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

    #[tracing::instrument(skip_all)]
    pub fn one_off_query(&self, caller_identity: Identity, query: String) -> Result<Vec<MemTable>, anyhow::Error> {
        let dbic = self.dbic();
        let db = &dbic.relational_db;
        let auth = AuthCtx::new(dbic.owner_identity, caller_identity);
        log::debug!("One-off query: {query}");
        let ctx = &ExecutionContext::sql(db.address());
        db.with_read_only(ctx, |tx| {
            let ast = sql::compiler::compile_sql(db, tx, &query)?;
            sql::execute::execute_sql_tx(db, tx, &query, ast, auth)?
                .context("One-off queries are not allowed to modify the database")
        })
    }

    /// FIXME(jgilles): this is a temporary workaround for deleting not currently being supported
    /// for tables without primary keys. It is only used in the benchmarks.
    /// Note: this doesn't drop the table, it just clears it!
    pub fn clear_table(&self, table_name: &str) -> Result<(), anyhow::Error> {
        let db = &*self.dbic().relational_db;
        db.with_auto_commit(&ExecutionContext::internal(db.address()), |tx| {
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
        &self.dbic().database
    }

    pub(crate) fn dbic(&self) -> &DatabaseInstanceContext {
        self.inner.dbic()
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
