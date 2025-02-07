use self::module_host_actor::ReducerOp;

use super::wasm_instance_env::WasmInstanceEnv;
use super::{Mem, WasmtimeFuel};
use crate::energy::ReducerBudget;
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::module_host_actor::{DescribeError, InitializationError};
use crate::host::wasm_common::*;
use crate::util::string_from_utf8_lossy_owned;
use spacetimedb_primitives::errno::HOST_CALL_FAILURE;
use wasmtime::{AsContext, AsContextMut, ExternType, Instance, InstancePre, Linker, Store, TypedFunc, WasmBacktrace};

fn log_traceback(func_type: &str, func: &str, e: &wasmtime::Error) {
    log::info!("{} \"{}\" runtime error: {}", func_type, func, e);
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

    pub const IMPLEMENTED_ABI: abi::VersionTuple = abi::VersionTuple::new(10, 0);

    pub(super) fn link_imports(linker: &mut Linker<WasmInstanceEnv>) -> anyhow::Result<()> {
        const { assert!(WasmtimeModule::IMPLEMENTED_ABI.major == spacetimedb_lib::MODULE_ABI_MAJOR_VERSION) };
        macro_rules! link_functions {
            ($($module:literal :: $func:ident,)*) => {
                #[allow(deprecated)]
                linker$(.func_wrap($module, stringify!($func), WasmInstanceEnv::$func)?)*;
            }
        }
        abi_funcs!(link_functions);
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

impl module_host_actor::WasmInstancePre for WasmtimeModule {
    type Instance = WasmtimeInstance;

    fn instantiate(&self, env: InstanceEnv, func_names: &FuncNames) -> Result<Self::Instance, InitializationError> {
        let env = WasmInstanceEnv::new(env);
        let mut store = Store::new(self.module.module().engine(), env);
        let instance = self
            .module
            .instantiate(&mut store)
            .map_err(InitializationError::Instantiation)?;

        let mem = Mem::extract(&instance, &mut store).unwrap();
        store.data_mut().instantiate(mem);

        // Note: this budget is just for initializers
        set_store_fuel(&mut store, ReducerBudget::DEFAULT_BUDGET.into());

        for preinit in &func_names.preinits {
            let func = instance.get_typed_func::<(), ()>(&mut store, preinit).unwrap();
            func.call(&mut store, ())
                .map_err(|err| InitializationError::RuntimeError {
                    err,
                    func: preinit.clone(),
                })?;
        }

        if let Ok(init) = instance.get_typed_func::<u32, i32>(&mut store, SETUP_DUNDER) {
            let setup_error = store.data_mut().setup_standard_bytes_sink();
            let res = init.call(&mut store, setup_error);
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

        Ok(WasmtimeInstance {
            store,
            instance,
            call_reducer,
        })
    }
}

type CallReducerType = TypedFunc<(u32, u64, u64, u64, u64, u64, u64, u64, u32, u32), i32>;

pub struct WasmtimeInstance {
    store: Store<WasmInstanceEnv>,
    instance: Instance,
    call_reducer: CallReducerType,
}

impl module_host_actor::WasmInstance for WasmtimeInstance {
    fn extract_descriptions(&mut self) -> Result<Vec<u8>, DescribeError> {
        let describer_func_name = DESCRIBE_MODULE_DUNDER;
        let store = &mut self.store;

        let describer = self.instance.get_func(&mut *store, describer_func_name).unwrap();
        let describer = describer
            .typed::<u32, ()>(&mut *store)
            .map_err(|_| DescribeError::Signature)?;

        let sink = store.data_mut().setup_standard_bytes_sink();

        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let result = describer.call(&mut *store, sink);

        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros());

        result
            .inspect_err(|err| log_traceback("describer", describer_func_name, err))
            .map_err(DescribeError::RuntimeError)?;

        // Fetch the bsatn returned by the describer call.
        let bytes = store.data_mut().take_standard_bytes_sink();

        Ok(bytes)
    }

    fn instance_env(&self) -> &InstanceEnv {
        self.store.data().instance_env()
    }

    type Trap = anyhow::Error;

    #[tracing::instrument(level = "trace", skip_all)]
    fn call_reducer(
        &mut self,
        op: ReducerOp<'_>,
        budget: ReducerBudget,
    ) -> module_host_actor::ExecuteResult<Self::Trap> {
        let store = &mut self.store;
        // note that ReducerBudget being a u64 is load-bearing here - although we convert budget right back into
        // EnergyQuanta at the end of this function, from_energy_quanta clamps it to a u64 range.
        // otherwise, we'd return something like `used: i128::MAX - u64::MAX`, which is inaccurate.
        set_store_fuel(store, budget.into());
        let original_fuel = get_store_fuel(store);

        // Prepare sender identity and address, as LITTLE-ENDIAN byte arrays.
        let [sender_0, sender_1, sender_2, sender_3] = bytemuck::must_cast(op.caller_identity.to_byte_array());
        let [address_0, address_1] = bytemuck::must_cast(op.caller_address.as_byte_array());

        // Prepare arguments to the reducer + the error sink & start timings.
        let (args_source, errors_sink) = store.data_mut().start_reducer(op.name, op.arg_bytes);

        let call_result = self.call_reducer.call(
            &mut *store,
            (
                op.id.0,
                sender_0,
                sender_1,
                sender_2,
                sender_3,
                address_0,
                address_1,
                op.timestamp.microseconds,
                args_source,
                errors_sink,
            ),
        );

        // Signal that this reducer call is finished. This gets us the timings
        // associated to our reducer call, and clears all of the instance state
        // associated to the call.
        let (timings, error) = store.data_mut().finish_reducer();

        let call_result = call_result.map(|code| handle_error_sink_code(code, error));

        let remaining_fuel = get_store_fuel(store);

        let remaining: ReducerBudget = remaining_fuel.into();
        let energy = module_host_actor::EnergyStats {
            used: (budget - remaining).into(),
            wasmtime_fuel_used: original_fuel.0 - remaining_fuel.0,
            remaining,
        };
        let memory_allocation = store.data().get_mem().memory.data_size(&store);

        module_host_actor::ExecuteResult {
            energy,
            timings,
            memory_allocation,
            call_result,
        }
    }

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap) {
        log_traceback(func_type, func, trap)
    }
}

fn set_store_fuel(store: &mut impl AsContextMut, fuel: WasmtimeFuel) {
    store.as_context_mut().set_fuel(fuel.0).unwrap();
}

fn get_store_fuel(store: &impl AsContext) -> WasmtimeFuel {
    WasmtimeFuel(store.as_context().get_fuel().unwrap())
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
