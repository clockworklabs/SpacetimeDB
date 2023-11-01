use super::wasm_instance_env::WasmInstanceEnv;
use super::Mem;
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::module_host_actor::{DescribeError, InitializationError, ReducerOp};
use crate::host::wasm_common::*;
use crate::host::EnergyQuanta;
use bytes::Bytes;
use wasmer::{
    imports, AsStoreMut, Engine, ExternType, Function, FunctionEnv, Imports, Instance, Module, RuntimeError, Store,
    TypedFunction,
};
use wasmer_middlewares::metering as wasmer_metering;

fn get_remaining_points(ctx: &mut impl AsStoreMut, instance: &Instance) -> u64 {
    let remaining_points = wasmer_metering::get_remaining_points(ctx, instance);
    match remaining_points {
        wasmer_metering::MeteringPoints::Remaining(x) => x,
        wasmer_metering::MeteringPoints::Exhausted => 0,
    }
}

fn log_traceback(func_type: &str, func: &str, e: &RuntimeError) {
    let frames = e.trace();
    let frames_len = frames.len();

    log::info!("{} \"{}\" runtime error: {}", func_type, func, e.message());
    for (i, frame) in frames.iter().enumerate().take(frames_len) {
        log::info!(
            "  Frame #{}: {:?}::{}",
            frames_len - i,
            frame.module_name(),
            rustc_demangle::demangle(frame.function_name().unwrap_or("<func>"))
        );
    }
}

#[derive(Clone)]
pub struct WasmerModule {
    module: Module,
    engine: Engine,
}

impl WasmerModule {
    pub fn new(module: Module, engine: Engine) -> Self {
        WasmerModule { module, engine }
    }

    pub const IMPLEMENTED_ABI: abi::VersionTuple = abi::VersionTuple::new(7, 0);

    fn imports(&self, store: &mut Store, env: &FunctionEnv<WasmInstanceEnv>) -> Imports {
        #[allow(clippy::assertions_on_constants)]
        const _: () = assert!(WasmerModule::IMPLEMENTED_ABI.major == spacetimedb_lib::MODULE_ABI_MAJOR_VERSION);
        imports! {
            "spacetime_7.0" => {
                "_schedule_reducer" => Function::new_typed_with_env(store, env, WasmInstanceEnv::schedule_reducer),
                "_cancel_reducer" => Function::new_typed_with_env(store, env, WasmInstanceEnv::cancel_reducer),
                "_delete_by_col_eq" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_by_col_eq,
                ),
                "_delete_by_rel" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_by_rel,
                ),
                "_insert" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::insert,
                ),
                "_get_table_id" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::get_table_id,
                ),
                "_create_index" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::create_index,
                ),
                "_iter_by_col_eq" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter_by_col_eq,
                ),
                "_iter_start" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter_start
                ),
                "_iter_start_filtered" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter_start_filtered
                ),
                "_iter_next" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter_next
                ),
                "_iter_drop" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter_drop
                ),
                "_console_log" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::console_log
                ),
                "_buffer_len" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_len),
                "_buffer_consume" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_consume),
                "_buffer_alloc" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_alloc),
                "_span_start" => Function::new_typed_with_env(store, env, WasmInstanceEnv::span_start),
                "_span_end" => Function::new_typed_with_env(store, env, WasmInstanceEnv::span_end),
            }
        }
    }
}

impl module_host_actor::WasmModule for WasmerModule {
    type Instance = WasmerInstance;
    type InstancePre = Self;

    type ExternType = ExternType;

    fn get_export(&self, s: &str) -> Option<Self::ExternType> {
        self.module
            .exports()
            .find(|exp| exp.name() == s)
            .map(|exp| exp.ty().clone())
    }

    fn for_each_export<E>(&self, mut f: impl FnMut(&str, &Self::ExternType) -> Result<(), E>) -> Result<(), E> {
        self.module.exports().try_for_each(|exp| f(exp.name(), exp.ty()))
    }

    fn instantiate_pre(&self) -> Result<Self::InstancePre, InitializationError> {
        Ok(self.clone())
    }
}

impl module_host_actor::WasmInstancePre for WasmerModule {
    type Instance = WasmerInstance;

    fn instantiate(&self, env: InstanceEnv, func_names: &FuncNames) -> Result<Self::Instance, InitializationError> {
        let mut store = Store::new(self.engine.clone());
        let env = WasmInstanceEnv::new(env);
        let env = FunctionEnv::new(&mut store, env);
        let imports = self.imports(&mut store, &env);
        let instance = Instance::new(&mut store, &self.module, &imports)
            .map_err(|err| InitializationError::Instantiation(err.into()))?;

        let mem = Mem::extract(&instance.exports).unwrap();

        env.as_mut(&mut store).instantiate(mem);

        // Note: this budget is just for initializers
        let budget = EnergyQuanta::DEFAULT_BUDGET.as_points();
        wasmer_metering::set_remaining_points(&mut store, &instance, budget);

        for preinit in &func_names.preinits {
            let func = instance.exports.get_typed_function::<(), ()>(&store, preinit).unwrap();
            func.call(&mut store).map_err(|err| InitializationError::RuntimeError {
                err: err.into(),
                func: preinit.clone(),
            })?;
        }

        let init = instance.exports.get_typed_function::<(), u32>(&store, SETUP_DUNDER);
        if let Ok(init) = init {
            match init.call(&mut store).map(BufferIdx) {
                Ok(errbuf) if errbuf.is_invalid() => {}
                Ok(errbuf) => {
                    let errbuf = env
                        .as_mut(&mut store)
                        .take_buffer(errbuf)
                        .unwrap_or_else(|| "unknown error".as_bytes().into());
                    let errbuf = crate::util::string_from_utf8_lossy_owned(errbuf.into()).into();
                    // TODO: catch this and return the error message to the http client
                    return Err(InitializationError::Setup(errbuf));
                }
                Err(err) => {
                    return Err(InitializationError::RuntimeError {
                        err: err.into(),
                        func: SETUP_DUNDER.to_owned(),
                    });
                }
            }
        }

        let call_reducer = instance
            .exports
            .get_typed_function(&store, CALL_REDUCER_DUNDER)
            .expect("no call_reducer");

        Ok(WasmerInstance {
            store,
            env,
            instance,
            call_reducer,
        })
    }
}

pub struct WasmerInstance {
    store: Store,
    env: FunctionEnv<WasmInstanceEnv>,
    instance: Instance,
    call_reducer: TypedFunction<(u32, u32, u32, u64, u32), u32>,
}

impl module_host_actor::WasmInstance for WasmerInstance {
    fn extract_descriptions(&mut self) -> Result<Bytes, DescribeError> {
        let describer_func_name = DESCRIBE_MODULE_DUNDER;
        let describer = self.instance.exports.get_function(describer_func_name).unwrap();

        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let store = &mut self.store;
        let describer = describer
            .typed::<(), u32>(store)
            .map_err(|_| DescribeError::Signature)?;
        let result = describer.call(store).map(BufferIdx);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        let buf = result.map_err(|err| {
            log_traceback("describer", describer_func_name, &err);
            DescribeError::RuntimeError(err.into())
        })?;
        let bytes = self
            .env
            .as_mut(store)
            .take_buffer(buf)
            .ok_or(DescribeError::BadBuffer)?;

        // Clear all of the instance state associated to this describer call.
        self.env.as_mut(store).finish_reducer();

        Ok(bytes)
    }

    fn instance_env(&self) -> &InstanceEnv {
        self.env.as_ref(&self.store).instance_env()
    }

    type Trap = wasmer::RuntimeError;

    fn call_reducer(
        &mut self,
        op: ReducerOp<'_>,
        budget: EnergyQuanta,
    ) -> module_host_actor::ExecuteResult<Self::Trap> {
        let store = &mut self.store;
        let instance = &self.instance;
        let budget = budget.as_points();
        wasmer_metering::set_remaining_points(store, instance, budget);

        let mut make_buf = |data| self.env.as_mut(store).insert_buffer(data);

        let identity_buf = make_buf(op.caller_identity.as_bytes().to_vec().into());
        let address_buf = make_buf(op.caller_address.as_slice().to_vec().into());
        let args_buf = make_buf(op.arg_bytes);

        self.env.as_mut(store).start_reducer(op.name);

        let result = self
            .call_reducer
            .call(
                store,
                op.id.0,
                identity_buf.0,
                address_buf.0,
                op.timestamp.0,
                args_buf.0,
            )
            .and_then(|errbuf| {
                let errbuf = BufferIdx(errbuf);
                Ok(if errbuf.is_invalid() {
                    Ok(())
                } else {
                    let errmsg = self
                        .env
                        .as_mut(store)
                        .take_buffer(errbuf)
                        .ok_or_else(|| RuntimeError::new("invalid buffer handle"))?;
                    Err(crate::util::string_from_utf8_lossy_owned(errmsg.into()).into())
                })
            });

        // Signal that this reducer call is finished. This gets us the timings
        // associated to our reducer call, and clears all of the instance state
        // associated to the call.
        let timings = self.env.as_mut(store).finish_reducer();

        let remaining = get_remaining_points(store, instance);
        let energy = module_host_actor::EnergyStats {
            used: EnergyQuanta::from_points(budget) - EnergyQuanta::from_points(remaining),
            remaining: EnergyQuanta::from_points(remaining),
        };

        module_host_actor::ExecuteResult {
            energy,
            timings,
            call_result: result,
        }
    }

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap) {
        log_traceback(func_type, func, trap)
    }
}
