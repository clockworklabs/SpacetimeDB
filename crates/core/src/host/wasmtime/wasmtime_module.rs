use self::module_host_actor::ReducerOp;

use super::wasm_instance_env::WasmInstanceEnv;
use super::{Mem, WasmtimeFuel, EPOCH_TICKS_PER_SECOND};
use crate::energy::ReducerBudget;
use crate::host::instance_env::InstanceEnv;
use crate::host::module_common::run_describer;
use crate::host::wasm_common::module_host_actor::{DescribeError, InitializationError};
use crate::host::wasm_common::*;
use crate::util::string_from_utf8_lossy_owned;
use futures_util::FutureExt;
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_primitives::errno::HOST_CALL_FAILURE;
use wasmtime::{
    AsContext, AsContextMut, ExternType, Instance, InstancePre, Linker, Store, TypedFunc, WasmBacktrace, WasmParams,
    WasmResults,
};

fn log_traceback(func_type: &str, func: &str, e: &wasmtime::Error) {
    log::info!("{func_type} \"{func}\" runtime error: {e}");
    if let Some(bt) = e.downcast_ref::<WasmBacktrace>() {
        let frames_len = bt.frames().len();
        for (i, frame) in bt.frames().iter().enumerate() {
            log::info!(
                "  Frame #{}: {}",
                frames_len - i,
                rustc_demangle::demangle(frame.func_name().unwrap_or("<unknown>"))
            );
        }
    }
}

#[derive(Clone)]
pub struct WasmtimeModule {
    module: InstancePre<WasmInstanceEnv>,
}

impl WasmtimeModule {
    pub(super) fn new(module: InstancePre<WasmInstanceEnv>) -> Self {
        WasmtimeModule { module }
    }

    pub const IMPLEMENTED_ABI: abi::VersionTuple = abi::VersionTuple::new(10, 3);

    pub(super) fn link_imports(linker: &mut Linker<WasmInstanceEnv>) -> anyhow::Result<()> {
        const { assert!(WasmtimeModule::IMPLEMENTED_ABI.major == spacetimedb_lib::MODULE_ABI_MAJOR_VERSION) };
        macro_rules! link_functions {
            ($($module:literal :: $func:ident,)*) => {
                #[allow(deprecated)]
                linker$(.func_wrap($module, stringify!($func), WasmInstanceEnv::$func)?)*;
            }
        }
        macro_rules! link_async_functions {
            ($($module:literal :: $func:ident,)*) => {
                #[allow(deprecated)]
                linker$(.func_wrap_async($module, stringify!($func), WasmInstanceEnv::$func)?)*;
            }
        }
        abi_funcs!(link_functions, link_async_functions);
        Ok(())
    }
}

impl module_host_actor::WasmModule for WasmtimeModule {
    type Instance = WasmtimeInstance;
    type InstancePre = Self;

    type ExternType = ExternType;

    fn get_export(&self, s: &str) -> Option<Self::ExternType> {
        self.module
            .module()
            .exports()
            .find(|exp| exp.name() == s)
            .map(|exp| exp.ty())
    }

    fn for_each_export<E>(&self, mut f: impl FnMut(&str, &Self::ExternType) -> Result<(), E>) -> Result<(), E> {
        self.module
            .module()
            .exports()
            .try_for_each(|exp| f(exp.name(), &exp.ty()))
    }

    fn instantiate_pre(&self) -> Result<Self::InstancePre, InitializationError> {
        Ok(self.clone())
    }
}

fn handle_error_sink_code(code: i32, error: Vec<u8>) -> Result<(), Box<str>> {
    match code {
        0 => Ok(()),
        CALL_FAILURE => Err(string_from_utf8_lossy_owned(error).into()),
        _ => Err("unknown return code".into()),
    }
}

const CALL_FAILURE: i32 = HOST_CALL_FAILURE.get() as i32;

/// Invoke `typed_func` and assert that it doesn't yield.
///
/// Our Wasmtime is configured for `async` execution, and will panic if we use the non-async [`TypedFunc::call`].
/// The `async` config is necessary to allow procedures to suspend, e.g. when making HTTP calls or acquiring transactions.
/// However, most of the WASM we execute, incl. reducers and startup functions, should never block/yield.
/// Rather than crossing our fingers and trusting, we run [`TypedFunc::call_async`] in [`FutureExt::now_or_never`],
/// an "async executor" which invokes [`std::task::Future::poll`] exactly once.
fn call_sync_typed_func<Args: WasmParams, Ret: WasmResults>(
    typed_func: &TypedFunc<Args, Ret>,
    store: &mut Store<WasmInstanceEnv>,
    args: Args,
) -> anyhow::Result<Ret> {
    let fut = typed_func.call_async(store, args);
    fut.now_or_never()
        .expect("`call_async` of supposedly synchronous WASM function returned `Poll::Pending`")
}

impl module_host_actor::WasmInstancePre for WasmtimeModule {
    type Instance = WasmtimeInstance;

    fn instantiate(&self, env: InstanceEnv, func_names: &FuncNames) -> Result<Self::Instance, InitializationError> {
        let env = WasmInstanceEnv::new(env);
        let mut store = Store::new(self.module.module().engine(), env);
        let instance_fut = self.module.instantiate_async(&mut store);

        let instance = instance_fut
            .now_or_never()
            .expect("`instantiate_async` did not immediately return `Ready`")
            .map_err(InitializationError::Instantiation)?;

        let mem = Mem::extract(&instance, &mut store).unwrap();
        store.data_mut().instantiate(mem);

        store.epoch_deadline_callback(|store| {
            let env = store.data();
            let database = env.instance_env().replica_ctx.database_identity;
            let funcall = env.funcall_name();
            let dur = env.funcall_start().elapsed();
            // TODO(procedure-timing): This measurement is not super meaningful for procedures,
            // which may (will) suspend execution and therefore may not have been continuously running since `env.funcall_start`.
            tracing::warn!(funcall, ?database, "Wasm has been running for {dur:?}");
            Ok(wasmtime::UpdateDeadline::Continue(EPOCH_TICKS_PER_SECOND))
        });

        // Note: this budget is just for initializers
        set_store_fuel(&mut store, ReducerBudget::DEFAULT_BUDGET.into());
        store.set_epoch_deadline(EPOCH_TICKS_PER_SECOND);

        for preinit in &func_names.preinits {
            let func = instance.get_typed_func::<(), ()>(&mut store, preinit).unwrap();
            call_sync_typed_func(&func, &mut store, ()).map_err(|err| InitializationError::RuntimeError {
                err,
                func: preinit.clone(),
            })?;
        }

        if let Ok(init) = instance.get_typed_func::<u32, i32>(&mut store, SETUP_DUNDER) {
            let setup_error = store.data_mut().setup_standard_bytes_sink();
            let res = call_sync_typed_func(&init, &mut store, setup_error);
            let error = store.data_mut().take_standard_bytes_sink();
            match res {
                // TODO: catch this and return the error message to the http client
                Ok(code) => handle_error_sink_code(code, error).map_err(InitializationError::Setup)?,
                Err(err) => {
                    let func = SETUP_DUNDER.to_owned();
                    return Err(InitializationError::RuntimeError { err, func });
                }
            }
        }

        let call_reducer = instance
            .get_typed_func(&mut store, CALL_REDUCER_DUNDER)
            .expect("no call_reducer");

        let call_procedure = get_call_procedure(&mut store, &instance);

        Ok(WasmtimeInstance {
            store,
            instance,
            call_reducer,
            call_procedure,
        })
    }
}

/// Look up the `instance`'s export named by [`CALL_PROCEDURE_DUNDER`].
///
/// Return `None` if the `instance` has no such export.
/// Modules from before the introduction of procedures will not have a `__call_procedure__` export,
/// which is fine because they also won't define any procedures.
///
/// Panics if the `instance` has an export at the expected name,
/// but it is not a function or is a function of an inappropriate type.
/// For new modules, this will be caught during publish.
/// Old modules from before the introduction of procedures might have an export at that name,
/// but it follows the double-underscore pattern of reserved names,
/// so we're fine to break those modules.
fn get_call_procedure(store: &mut Store<WasmInstanceEnv>, instance: &Instance) -> Option<CallProcedureType> {
    // Wasmtime uses `anyhow` for error reporting, vexing library users the world over.
    // This means we can't distinguish between the failure modes of `Instance::get_typed_func`.
    // Instead, we type out the body of that method ourselves,
    // but with error handling appropriate to our needs.
    let export = instance.get_export(store.as_context_mut(), CALL_PROCEDURE_DUNDER)?;

    Some(
        export
            .into_func()
            .unwrap_or_else(|| panic!("{CALL_PROCEDURE_DUNDER} export is not a function"))
            .typed(store)
            .unwrap_or_else(|err| panic!("{CALL_PROCEDURE_DUNDER} export is a function with incorrect type: {err}")),
    )
}

type CallReducerType = TypedFunc<
    (
        // Reducer ID,
        u32,
        // Sender `Identity`
        u64,
        u64,
        u64,
        u64,
        // Sender `ConnectionId`, or 0 for none.
        u64,
        u64,
        // Start timestamp.
        u64,
        // Args byte source.
        u32,
        // Errors byte sink.
        u32,
    ),
    // Errno.
    i32,
>;
// `__call_procedure__` takes the same arguments as `__call_reducer__`.
type CallProcedureType = CallReducerType;

pub struct WasmtimeInstance {
    store: Store<WasmInstanceEnv>,
    instance: Instance,
    call_reducer: CallReducerType,
    call_procedure: Option<CallProcedureType>,
}

#[async_trait::async_trait]
impl module_host_actor::WasmInstance for WasmtimeInstance {
    fn extract_descriptions(&mut self) -> Result<Vec<u8>, DescribeError> {
        let describer_func_name = DESCRIBE_MODULE_DUNDER;

        let describer = self
            .instance
            .get_typed_func::<u32, ()>(&mut self.store, describer_func_name)
            .map_err(DescribeError::Signature)?;

        let sink = self.store.data_mut().setup_standard_bytes_sink();

        run_describer(log_traceback, || {
            call_sync_typed_func(&describer, &mut self.store, sink)
        })?;

        // Fetch the bsatn returned by the describer call.
        let bytes = self.store.data_mut().take_standard_bytes_sink();

        Ok(bytes)
    }

    fn instance_env(&self) -> &InstanceEnv {
        self.store.data().instance_env()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn call_reducer(&mut self, op: ReducerOp<'_>, budget: ReducerBudget) -> module_host_actor::ExecuteResult {
        let store = &mut self.store;

        prepare_store_for_call(store, budget);

        // Prepare sender identity and connection ID, as LITTLE-ENDIAN byte arrays.
        let [sender_0, sender_1, sender_2, sender_3] = prepare_identity_for_call(*op.caller_identity);
        let [conn_id_0, conn_id_1] = prepare_connection_id_for_call(*op.caller_connection_id);

        // Prepare arguments to the reducer + the error sink & start timings.
        let args_bytes = op.args.get_bsatn().clone();

        let (args_source, errors_sink) = store.data_mut().start_funcall(op.name, args_bytes, op.timestamp);

        let call_result = call_sync_typed_func(
            &self.call_reducer,
            &mut *store,
            (
                op.id.0,
                sender_0,
                sender_1,
                sender_2,
                sender_3,
                conn_id_0,
                conn_id_1,
                op.timestamp.to_micros_since_unix_epoch() as u64,
                args_source.0,
                errors_sink,
            ),
        );

        // Signal that this reducer call is finished. This gets us the timings
        // associated to our reducer call, and clears all of the instance state
        // associated to the call.
        let (timings, error) = store.data_mut().finish_funcall();

        let call_result = call_result.map(|code| handle_error_sink_code(code, error));

        // Compute fuel and heap usage.
        let remaining_fuel = get_store_fuel(store);
        let remaining: ReducerBudget = remaining_fuel.into();
        let energy = module_host_actor::EnergyStats { budget, remaining };
        let memory_allocation = store.data().get_mem().memory.data_size(&store);

        module_host_actor::ExecuteResult {
            energy,
            timings,
            memory_allocation,
            call_result,
        }
    }

    fn log_traceback(func_type: &str, func: &str, trap: &anyhow::Error) {
        log_traceback(func_type, func, trap)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn call_procedure(
        &mut self,
        op: module_host_actor::ProcedureOp,
        budget: ReducerBudget,
    ) -> module_host_actor::ProcedureExecuteResult {
        let store = &mut self.store;
        prepare_store_for_call(store, budget);

        // Prepare sender identity and connection ID, as LITTLE-ENDIAN byte arrays.
        let [sender_0, sender_1, sender_2, sender_3] = prepare_identity_for_call(op.caller_identity);
        let [conn_id_0, conn_id_1] = prepare_connection_id_for_call(op.caller_connection_id);

        // Prepare arguments to the reducer + the error sink & start timings.
        let (args_source, result_sink) = store.data_mut().start_funcall(&op.name, op.arg_bytes, op.timestamp);

        let Some(call_procedure) = self.call_procedure.as_ref() else {
            return module_host_actor::ProcedureExecuteResult {
                energy: module_host_actor::EnergyStats::ZERO,
                timings: module_host_actor::ExecutionTimings::zero(),
                memory_allocation: get_memory_size(store),
                call_result: Err(anyhow::anyhow!(
                    "Module defines procedure {} but does not export `{}`",
                    op.name,
                    CALL_PROCEDURE_DUNDER,
                )),
            };
        };
        let call_result = call_procedure
            .call_async(
                &mut *store,
                (
                    op.id.0,
                    sender_0,
                    sender_1,
                    sender_2,
                    sender_3,
                    conn_id_0,
                    conn_id_1,
                    op.timestamp.to_micros_since_unix_epoch() as u64,
                    args_source.0,
                    result_sink,
                ),
            )
            .await;

        // Close the timing span for this procedure and get the BSATN bytes of its result.
        let (timings, result_bytes) = store.data_mut().finish_funcall();

        let call_result = call_result.and_then(|code| {
            (code == 0).then_some(result_bytes.into()).ok_or_else(|| {
                anyhow::anyhow!(
                    "{CALL_PROCEDURE_DUNDER} returned unexpected code {code}. Procedures should return code 0 or trap."
                )
            })
        });

        let remaining_fuel = get_store_fuel(store);
        let remaining = ReducerBudget::from(remaining_fuel);

        let energy = module_host_actor::EnergyStats { budget, remaining };
        let memory_allocation = get_memory_size(store);

        module_host_actor::ProcedureExecuteResult {
            energy,
            timings,
            memory_allocation,
            call_result,
        }
    }
}

fn set_store_fuel(store: &mut impl AsContextMut, fuel: WasmtimeFuel) {
    store.as_context_mut().set_fuel(fuel.0).unwrap();
}

fn get_store_fuel(store: &impl AsContext) -> WasmtimeFuel {
    WasmtimeFuel(store.as_context().get_fuel().unwrap())
}

fn prepare_store_for_call(store: &mut Store<WasmInstanceEnv>, budget: ReducerBudget) {
    // note that ReducerBudget being a u64 is load-bearing here - although we convert budget right back into
    // EnergyQuanta at the end of this function, from_energy_quanta clamps it to a u64 range.
    // otherwise, we'd return something like `used: i128::MAX - u64::MAX`, which is inaccurate.
    set_store_fuel(store, budget.into());

    // We enable epoch interruption only to log on long-running WASM functions.
    // Our epoch interrupt callback logs and then immediately resumes execution.
    store.set_epoch_deadline(EPOCH_TICKS_PER_SECOND);
}

/// Convert `caller_identity` to the format used by `__call_reducer__` and `__call_procedure__`,
/// i.e. an array of 4 `u64`s.
///
/// Callers should destructure this like:
/// ```ignore
/// # let identity = Identity::ZERO;
/// let [sender_0, sender_1, sender_2, sender_3] = prepare_identity_for_call(identity);
/// ```
fn prepare_identity_for_call(caller_identity: Identity) -> [u64; 4] {
    // Encode this as a LITTLE-ENDIAN byte array
    bytemuck::must_cast(caller_identity.to_byte_array())
}

/// Convert `caller_connection_id` to the format used by `__call_reducer` and `__call_procedure__`,
/// i.e. an array of 2 `u64`s.
///
/// Callers should destructure this like:
/// ```ignore
/// # let connection_id = ConnectionId::ZERO;
/// let [conn_id_0, conn_id_1] = prepare_connection_id_for_call(connection_id);
/// ```
///
fn prepare_connection_id_for_call(caller_connection_id: ConnectionId) -> [u64; 2] {
    // Encode this as a LITTLE-ENDIAN byte array
    bytemuck::must_cast(caller_connection_id.as_le_byte_array())
}

fn get_memory_size(store: &Store<WasmInstanceEnv>) -> usize {
    store.data().get_mem().memory.data_size(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::EnergyQuanta;

    #[test]
    fn test_fuel() {
        let mut store = wasmtime::Store::new(
            &wasmtime::Engine::new(wasmtime::Config::new().consume_fuel(true)).unwrap(),
            (),
        );
        let budget = ReducerBudget::DEFAULT_BUDGET;
        set_store_fuel(&mut store, budget.into());
        store.set_fuel(store.get_fuel().unwrap() - 10).unwrap();
        let remaining: EnergyQuanta = get_store_fuel(&store).into();
        let used = EnergyQuanta::from(budget) - remaining;
        assert_eq!(used, EnergyQuanta::new(10_000));
    }
}
