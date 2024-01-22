use super::wasm_instance_env::WasmInstanceEnv;
use super::{Mem, WasmtimeFuel};
use crate::energy::ReducerBudget;
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::module_host_actor::{DescribeError, InitializationError, ReducerOp};
use crate::host::wasm_common::*;
use crate::util::ResultInspectExt;
use anyhow::anyhow;
use bytes::Bytes;
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

    pub const IMPLEMENTED_ABI: abi::VersionTuple = abi::VersionTuple::new(7, 0);

    pub(super) fn link_imports(linker: &mut Linker<WasmInstanceEnv>) -> anyhow::Result<()> {
        #[allow(clippy::assertions_on_constants)]
        const _: () = assert!(WasmtimeModule::IMPLEMENTED_ABI.major == spacetimedb_lib::MODULE_ABI_MAJOR_VERSION);
        linker
            .func_wrap("spacetime_7.0", "_schedule_reducer", WasmInstanceEnv::schedule_reducer)?
            .func_wrap("spacetime_7.0", "_cancel_reducer", WasmInstanceEnv::cancel_reducer)?
            .func_wrap("spacetime_7.0", "_delete_by_col_eq", WasmInstanceEnv::delete_by_col_eq)?
            .func_wrap("spacetime_7.0", "_delete_by_rel", WasmInstanceEnv::delete_by_rel)?
            .func_wrap("spacetime_7.0", "_insert", WasmInstanceEnv::insert)?
            .func_wrap("spacetime_7.0", "_get_table_id", WasmInstanceEnv::get_table_id)?
            .func_wrap("spacetime_7.0", "_create_index", WasmInstanceEnv::create_index)?
            .func_wrap("spacetime_7.0", "_iter_by_col_eq", WasmInstanceEnv::iter_by_col_eq)?
            .func_wrap("spacetime_7.0", "_iter_start", WasmInstanceEnv::iter_start)?
            .func_wrap(
                "spacetime_7.0",
                "_iter_start_filtered",
                WasmInstanceEnv::iter_start_filtered,
            )?
            .func_wrap("spacetime_7.0", "_iter_next", WasmInstanceEnv::iter_next)?
            .func_wrap("spacetime_7.0", "_iter_drop", WasmInstanceEnv::iter_drop)?
            .func_wrap("spacetime_7.0", "_console_log", WasmInstanceEnv::console_log)?
            .func_wrap("spacetime_7.0", "_buffer_len", WasmInstanceEnv::buffer_len)?
            .func_wrap("spacetime_7.0", "_buffer_consume", WasmInstanceEnv::buffer_consume)?
            .func_wrap("spacetime_7.0", "_buffer_alloc", WasmInstanceEnv::buffer_alloc)?
            .func_wrap("spacetime_7.0", "_span_start", WasmInstanceEnv::span_start)?
            .func_wrap("spacetime_7.0", "_span_end", WasmInstanceEnv::span_end)?;
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

        let init = instance.get_typed_func::<(), u32>(&mut store, SETUP_DUNDER);
        if let Ok(init) = init {
            match init.call(&mut store, ()).map(BufferIdx) {
                Ok(errbuf) if errbuf.is_invalid() => {}
                Ok(errbuf) => {
                    let errbuf = store
                        .data_mut()
                        .take_buffer(errbuf)
                        .unwrap_or_else(|| "unknown error".as_bytes().into());
                    let errbuf = crate::util::string_from_utf8_lossy_owned(errbuf.into()).into();
                    // TODO: catch this and return the error message to the http client
                    return Err(InitializationError::Setup(errbuf));
                }
                Err(err) => {
                    return Err(InitializationError::RuntimeError {
                        err,
                        func: SETUP_DUNDER.to_owned(),
                    });
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

pub struct WasmtimeInstance {
    store: Store<WasmInstanceEnv>,
    instance: Instance,
    call_reducer: TypedFunc<(u32, u32, u32, u64, u32), u32>,
}

impl module_host_actor::WasmInstance for WasmtimeInstance {
    fn extract_descriptions(&mut self) -> Result<Bytes, DescribeError> {
        let describer_func_name = DESCRIBE_MODULE_DUNDER;
        let describer = self.instance.get_func(&mut self.store, describer_func_name).unwrap();

        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let store = &mut self.store;
        let describer = describer
            .typed::<(), u32>(&mut *store)
            .map_err(|_| DescribeError::Signature)?;
        let result = describer.call(&mut *store, ()).map(BufferIdx);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        let buf = result
            .inspect_err_(|err| log_traceback("describer", describer_func_name, err))
            .map_err(DescribeError::RuntimeError)?;
        let bytes = store.data_mut().take_buffer(buf).ok_or(DescribeError::BadBuffer)?;

        // Clear all of the instance state associated to this describer call.
        store.data_mut().finish_reducer();

        Ok(bytes)
    }

    fn instance_env(&self) -> &InstanceEnv {
        self.store.data().instance_env()
    }

    type Trap = anyhow::Error;

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

        let mut make_buf = |data| store.data_mut().insert_buffer(data);

        let identity_buf = make_buf(op.caller_identity.as_bytes().to_vec().into());
        let address_buf = make_buf(op.caller_address.as_slice().to_vec().into());
        let args_buf = make_buf(op.arg_bytes);

        store.data_mut().start_reducer(op.name);

        let call_result = self
            .call_reducer
            .call(
                &mut *store,
                (op.id.0, identity_buf.0, address_buf.0, op.timestamp.0, args_buf.0),
            )
            .and_then(|errbuf| {
                let errbuf = BufferIdx(errbuf);
                Ok(if errbuf.is_invalid() {
                    Ok(())
                } else {
                    let errmsg = store
                        .data_mut()
                        .take_buffer(errbuf)
                        .ok_or_else(|| anyhow!("invalid buffer handle"))?;
                    Err(crate::util::string_from_utf8_lossy_owned(errmsg.into()).into())
                })
            });

        // Signal that this reducer call is finished. This gets us the timings
        // associated to our reducer call, and clears all of the instance state
        // associated to the call.
        let timings = store.data_mut().finish_reducer();

        let remaining: ReducerBudget = get_store_fuel(store).into();
        let energy = module_host_actor::EnergyStats {
            used: (budget - remaining).into(),
            remaining,
        };

        module_host_actor::ExecuteResult {
            energy,
            timings,
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
