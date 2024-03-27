use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use futures::{Future, FutureExt};
use indexmap::IndexMap;
use itertools::{Either, Itertools};
use spacetimedb_client_api_messages::client_api::table_row_operation::OperationType;
use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::identity::RequestId;

use super::{ArgsTuple, InvalidReducerArguments, ReducerArgs, ReducerCallResult, ReducerId, Timestamp};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::database_logger::LogLevel;
use crate::db::datastore::traits::{TxData, TxOp};
use crate::db::relational_db::RelationalDB;
use crate::db::update::UpdateDatabaseError;
use crate::energy::EnergyQuanta;
use crate::error::DBError;
use crate::execution_context::ExecutionContext;
use crate::hash::Hash;
use crate::identity::Identity;
use crate::json::client_api::{TableRowOperationJson, TableUpdateJson};
use crate::protobuf::client_api::{TableRowOperation, TableUpdate};
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::util::lending_pool::{Closed, LendingPool, LentResource, PoolClosed};
use crate::util::notify_once::NotifyOnce;
use derive_more::{From, Into};
use spacetimedb_lib::{Address, ReducerDef, TableDesc};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{ProductValue, Typespace, WithTypespace};
use spacetimedb_vm::relation::MemTable;

#[derive(Debug, Clone, From, Into)]
pub struct ProtocolDatabaseUpdate {
    pub tables: Either<Vec<TableUpdate>, Vec<TableUpdateJson>>,
}

impl From<ProtocolDatabaseUpdate> for Vec<TableUpdate> {
    fn from(update: ProtocolDatabaseUpdate) -> Self {
        update.tables.unwrap_left()
    }
}

impl From<ProtocolDatabaseUpdate> for Vec<TableUpdateJson> {
    fn from(update: ProtocolDatabaseUpdate) -> Self {
        update.tables.unwrap_right()
    }
}

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

    pub fn from_writes(stdb: &RelationalDB, tx_data: &TxData) -> Self {
        let mut map: HashMap<TableId, (Vec<ProductValue>, Vec<ProductValue>)> = HashMap::new();
        for record in tx_data.records.iter() {
            let pv = record.product_value.clone();
            let table = map.entry(record.table_id).or_default();
            match record.op {
                TxOp::Delete => &mut table.0,
                TxOp::Insert(_) => &mut table.1,
            }
            .push(pv);
        }

        let ctx = ExecutionContext::internal(stdb.address());
        let table_updates = stdb.with_read_only(&ctx, |tx| {
            map.into_iter()
                .map(|(table_id, (deletes, inserts))| DatabaseTableUpdate {
                    table_id,
                    table_name: stdb.table_name_from_id(tx, table_id).unwrap().unwrap().into_owned(),
                    deletes,
                    inserts,
                })
                .collect()
        });

        DatabaseUpdate { tables: table_updates }
    }
}

impl From<DatabaseUpdate> for Vec<TableUpdate> {
    fn from(update: DatabaseUpdate) -> Self {
        update.tables.into_iter().map_into().collect()
    }
}

impl From<DatabaseUpdate> for Vec<TableUpdateJson> {
    fn from(update: DatabaseUpdate) -> Self {
        update.tables.into_iter().map_into().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseTableUpdate {
    pub table_id: TableId,
    pub table_name: String,
    pub inserts: Vec<ProductValue>,
    pub deletes: Vec<ProductValue>,
}

impl From<DatabaseTableUpdate> for TableUpdate {
    fn from(table: DatabaseTableUpdate) -> Self {
        let deletes = table.deletes.into_iter().map(TableOp::delete);
        let inserts = table.inserts.into_iter().map(TableOp::insert);
        let table_row_operations = deletes.chain(inserts).map(|x| (&x).into()).collect();
        Self {
            table_id: table.table_id.into(),
            table_name: table.table_name,
            table_row_operations,
        }
    }
}

impl From<DatabaseTableUpdate> for TableUpdateJson {
    fn from(table: DatabaseTableUpdate) -> Self {
        let deletes = table.deletes.into_iter().map(TableOp::delete);
        let inserts = table.inserts.into_iter().map(TableOp::insert);
        let table_row_operations = deletes.chain(inserts).map_into().collect();
        Self {
            table_id: table.table_id.into(),
            table_name: table.table_name,
            table_row_operations,
        }
    }
}

#[derive(Debug)]
pub struct DatabaseUpdateCow<'a> {
    pub tables: Vec<DatabaseTableUpdateCow<'a>>,
}

#[derive(PartialEq, Debug)]
pub struct DatabaseTableUpdateCow<'a> {
    pub table_id: TableId,
    pub table_name: String,
    pub updates: UpdatesCow<'a>,
}

#[derive(Default, PartialEq, Debug)]
pub struct UpdatesCow<'a> {
    pub deletes: Vec<Cow<'a, ProductValue>>,
    pub inserts: Vec<Cow<'a, ProductValue>>,
}

impl UpdatesCow<'_> {
    /// Returns whether there are any updates.
    pub fn has_updates(&self) -> bool {
        !(self.deletes.is_empty() && self.inserts.is_empty())
    }

    /// Returns a combined iterator over both deletes and inserts.
    pub fn iter(&self) -> impl Iterator<Item = TableOpRef<'_>> {
        self.deletes
            .iter()
            .map(TableOpRef::delete)
            .chain(self.inserts.iter().map(TableOpRef::insert))
    }
}

pub struct TableOpRef<'a> {
    pub op_type: u8,
    pub row: &'a ProductValue,
}

impl<'a> TableOpRef<'a> {
    #[inline]
    fn new(op_type: u8, row: &'a Cow<'a, ProductValue>) -> Self {
        let row = &**row;
        Self { op_type, row }
    }

    #[inline]
    pub fn insert(row: &'a Cow<'a, ProductValue>) -> Self {
        Self::new(1, row)
    }

    #[inline]
    pub fn delete(row: &'a Cow<'a, ProductValue>) -> Self {
        Self::new(0, row)
    }
}

impl From<TableOpRef<'_>> for TableRowOperation {
    fn from(top: TableOpRef<'_>) -> Self {
        let row = to_vec(top.row).unwrap();
        let op = if top.op_type == 1 {
            OperationType::Insert.into()
        } else {
            OperationType::Delete.into()
        };
        Self { op, row }
    }
}

impl From<TableOpRef<'_>> for TableRowOperationJson {
    fn from(top: TableOpRef<'_>) -> Self {
        TableOp::from(top).into()
    }
}

impl From<TableOpRef<'_>> for TableOp {
    fn from(top: TableOpRef<'_>) -> Self {
        let row = top.row.clone();
        let op_type = top.op_type;
        Self { op_type, row }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOp {
    pub op_type: u8,
    pub row: ProductValue,
}

impl TableOp {
    #[inline]
    pub fn new(op_type: u8, row: ProductValue) -> Self {
        Self { op_type, row }
    }

    #[inline]
    pub fn insert(row: ProductValue) -> Self {
        Self::new(1, row)
    }

    #[inline]
    pub fn delete(row: ProductValue) -> Self {
        Self::new(0, row)
    }
}

impl From<&TableOp> for TableRowOperation {
    #[inline]
    fn from(TableOp { op_type, row }: &TableOp) -> Self {
        TableOpRef { row, op_type: *op_type }.into()
    }
}

impl From<TableOp> for TableRowOperationJson {
    fn from(top: TableOp) -> Self {
        let row = top.row.elements;
        let op = if top.op_type == 1 { "insert" } else { "delete" }.into();
        Self { op, row }
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

#[derive(Debug, Clone)]
pub struct ModuleFunctionCall {
    pub reducer: String,
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

#[derive(Debug)]
pub struct ModuleInfo {
    pub identity: Identity,
    pub address: Address,
    pub module_hash: Hash,
    pub typespace: Typespace,
    pub reducers: ReducersMap,
    pub catalog: HashMap<String, EntityDef>,
    pub log_tx: tokio::sync::broadcast::Sender<bytes::Bytes>,
    pub subscriptions: ModuleSubscriptions,
}

pub struct ReducersMap(pub IndexMap<String, ReducerDef>);

impl fmt::Debug for ReducersMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Index<ReducerId> for ReducersMap {
    type Output = ReducerDef;
    fn index(&self, index: ReducerId) -> &Self::Output {
        &self.0[index.0 as usize]
    }
}

impl ReducersMap {
    pub fn lookup_id(&self, reducer_name: &str) -> Option<ReducerId> {
        self.0.get_index_of(reducer_name).map(ReducerId::from)
    }

    pub fn lookup(&self, reducer_name: &str) -> Option<(ReducerId, &ReducerDef)> {
        self.0.get_full(reducer_name).map(|(id, _, def)| (id.into(), def))
    }
}

pub trait Module: Send + Sync + 'static {
    type Instance: ModuleInstance;
    type InitialInstances<'a>: IntoIterator<Item = Self::Instance> + 'a;
    fn initial_instances(&mut self) -> Self::InitialInstances<'_>;
    fn info(&self) -> Arc<ModuleInfo>;
    fn create_instance(&self) -> Self::Instance;
    fn inject_logs(&self, log_level: LogLevel, message: &str);
    fn close(self);
    fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<spacetimedb_vm::relation::MemTable>, DBError>;
    fn clear_table(&self, table_name: String) -> Result<(), anyhow::Error>;
    #[cfg(feature = "tracelogging")]
    fn get_trace(&self) -> Option<bytes::Bytes>;
    #[cfg(feature = "tracelogging")]
    fn stop_trace(&self) -> anyhow::Result<()>;
}

pub trait ModuleInstance: Send + 'static {
    fn trapped(&self) -> bool;

    // TODO(kim): The `fence` arg below is to thread through the fencing token
    // (see [`crate::db::datastore::traits::MutProgrammable`]). This trait
    // should probably be generic over the type of token, but that turns out a
    // bit unpleasant at the moment. So we just use the widest possible integer.

    fn init_database(&mut self, fence: u128, args: ArgsTuple) -> anyhow::Result<ReducerCallResult>;

    fn update_database(&mut self, fence: u128) -> anyhow::Result<UpdateDatabaseResult>;

    fn call_reducer(&mut self, params: CallReducerParams) -> ReducerCallResult;
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
    fn init_database(&mut self, fence: u128, args: ArgsTuple) -> anyhow::Result<ReducerCallResult> {
        let ret = self.inst.init_database(fence, args);
        self.check_trap();
        ret
    }
    fn update_database(&mut self, fence: u128) -> anyhow::Result<UpdateDatabaseResult> {
        let ret = self.inst.update_database(fence);
        self.check_trap();
        ret
    }
    fn call_reducer(&mut self, params: CallReducerParams) -> ReducerCallResult {
        let ret = self.inst.call_reducer(params);
        self.check_trap();
        ret
    }
}

#[derive(Clone)]
pub struct ModuleHost {
    info: Arc<ModuleInfo>,
    inner: Arc<dyn DynModuleHost>,
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
    fn inject_logs(&self, log_level: LogLevel, message: &str);
    fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<spacetimedb_vm::relation::MemTable>, DBError>;
    fn clear_table(&self, table_name: String) -> Result<(), anyhow::Error>;
    fn start(&self);
    fn exit(&self) -> Closed<'_>;
    fn exited(&self) -> Closed<'_>;
}

struct HostControllerActor<T: Module> {
    module: Arc<T>,
    instance_pool: LendingPool<T::Instance>,
    start: NotifyOnce,
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
        self.start.notified().await;
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

    fn inject_logs(&self, log_level: LogLevel, message: &str) {
        self.module.inject_logs(log_level, message)
    }

    fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<spacetimedb_vm::relation::MemTable>, DBError> {
        self.module.one_off_query(caller_identity, query)
    }

    fn clear_table(&self, table_name: String) -> Result<(), anyhow::Error> {
        self.module.clear_table(table_name)
    }

    fn start(&self) {
        self.start.notify();
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
}

pub type UpdateDatabaseResult = Result<UpdateDatabaseSuccess, UpdateDatabaseError>;

#[derive(Debug)]
pub struct UpdateDatabaseSuccess {
    /// Outcome of calling the module's __update__ reducer, `None` if none is
    /// defined.
    pub update_result: Option<ReducerCallResult>,
    /// Outcome of calling the module's pending __migrate__ reducers, empty if
    /// none are defined or pending.
    ///
    /// Currently always empty, as __migrate__ is not yet supported.
    pub migrate_results: Vec<ReducerCallResult>,
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
    pub fn new(mut module: impl Module) -> Self {
        let info = module.info();
        let instance_pool = LendingPool::new();
        instance_pool.add_multiple(module.initial_instances()).unwrap();
        let inner = Arc::new(HostControllerActor {
            module: Arc::new(module),
            instance_pool,
            start: NotifyOnce::new(),
        });
        ModuleHost { info, inner }
    }

    pub fn start(&self) {
        self.inner.start()
    }

    #[inline]
    pub fn info(&self) -> &ModuleInfo {
        &self.info
    }

    #[inline]
    pub fn subscriptions(&self) -> &ModuleSubscriptions {
        &self.info.subscriptions
    }

    async fn call<F, R>(&self, _reducer_name: &str, f: F) -> Result<R, NoSuchModule>
    where
        F: FnOnce(&mut dyn ModuleInstance) -> R + Send + 'static,
        R: Send + 'static,
    {
        let mut inst = self.inner.get_instance(self.info.address).await?;

        let result = tokio::task::spawn_blocking(move || f(&mut *inst))
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
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

    pub async fn call_identity_connected_disconnected(
        &self,
        caller_identity: Identity,
        caller_address: Address,
        connected: bool,
    ) -> Result<(), ReducerCallError> {
        match self
            .call_reducer_inner(
                caller_identity,
                Some(caller_address),
                None,
                None,
                None,
                if connected {
                    "__identity_connected__"
                } else {
                    "__identity_disconnected__"
                },
                ReducerArgs::Nullary,
            )
            .await
        {
            Ok(_) | Err(ReducerCallError::NoSuchReducer) => Ok(()),
            Err(e) => Err(e),
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
        let (reducer_id, schema) = self
            .info
            .reducers
            .lookup(reducer_name)
            .ok_or(ReducerCallError::NoSuchReducer)?;

        let args = args.into_tuple(self.info.typespace.with_type(schema))?;
        let caller_address = caller_address.unwrap_or(Address::__DUMMY);

        self.call(reducer_name, move |inst| {
            inst.call_reducer(CallReducerParams {
                timestamp: Timestamp::now(),
                caller_identity,
                caller_address,
                client,
                request_id,
                timer,
                reducer_id,
                args,
            })
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

    pub fn catalog(&self) -> Catalog {
        Catalog(self.info.clone())
    }

    pub fn subscribe_to_logs(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.info().log_tx.subscribe())
    }

    pub async fn init_database(&self, fence: u128, args: ReducerArgs) -> Result<ReducerCallResult, InitDatabaseError> {
        let args = match self.catalog().get_reducer("__init__") {
            Some(schema) => args.into_tuple(schema)?,
            _ => ArgsTuple::default(),
        };
        self.call("<init_database>", move |inst| inst.init_database(fence, args))
            .await?
            .map_err(InitDatabaseError::Other)
    }

    pub async fn update_database(&self, fence: u128) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.call("<update_database>", move |inst| inst.update_database(fence))
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
        self.inner.inject_logs(log_level, message)
    }

    pub async fn one_off_query(
        &self,
        caller_identity: Identity,
        query: String,
    ) -> Result<Vec<MemTable>, anyhow::Error> {
        let result = self.inner.one_off_query(caller_identity, query)?;
        Ok(result)
    }

    /// FIXME(jgilles): this is a temporary workaround for deleting not currently being supported
    /// for tables without primary keys. It is only used in the benchmarks.
    /// Note: this doesn't drop the table, it just clears it!
    pub async fn clear_table(&self, table_name: String) -> Result<(), anyhow::Error> {
        self.inner.clear_table(table_name)?;
        Ok(())
    }

    pub fn downgrade(&self) -> WeakModuleHost {
        WeakModuleHost {
            info: self.info.clone(),
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl WeakModuleHost {
    pub fn upgrade(&self) -> Option<ModuleHost> {
        let inner = self.inner.upgrade()?;
        Some(ModuleHost {
            info: self.info.clone(),
            inner,
        })
    }
}

#[derive(Debug)]
pub enum EntityDef {
    Reducer(ReducerDef),
    Table(TableDesc),
}
impl EntityDef {
    pub fn as_reducer(&self) -> Option<&ReducerDef> {
        match self {
            Self::Reducer(x) => Some(x),
            _ => None,
        }
    }
    pub fn as_table(&self) -> Option<&TableDesc> {
        match self {
            Self::Table(x) => Some(x),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Catalog(Arc<ModuleInfo>);
impl Catalog {
    pub fn typespace(&self) -> &Typespace {
        &self.0.typespace
    }

    pub fn get(&self, name: &str) -> Option<WithTypespace<'_, EntityDef>> {
        self.0.catalog.get(name).map(|ty| self.0.typespace.with_type(ty))
    }
    pub fn get_reducer(&self, name: &str) -> Option<WithTypespace<'_, ReducerDef>> {
        let schema = self.get(name)?;
        Some(schema.with(schema.ty().as_reducer()?))
    }
    pub fn get_table(&self, name: &str) -> Option<WithTypespace<'_, TableDesc>> {
        let schema = self.get(name)?;
        Some(schema.with(schema.ty().as_table()?))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, WithTypespace<'_, EntityDef>)> + '_ {
        self.0
            .catalog
            .iter()
            .map(|(name, e)| (&**name, self.0.typespace.with_type(e)))
    }
}
