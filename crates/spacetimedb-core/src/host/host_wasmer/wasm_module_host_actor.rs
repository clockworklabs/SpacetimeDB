use super::wasm_instance_env::WasmInstanceEnv;
use super::Mem;
use crate::host::host_controller::{DescribedEntityType, ReducerBudget};
use crate::host::instance_env::InstanceEnv;
use crate::host::wasm_common::*;
use anyhow::{anyhow, Context};
use spacetimedb_lib::{EntityDef, ReducerDef, RepeaterDef, TableDef};
use std::cmp::max;
use std::collections::HashMap;
use wasmer::{
    imports, AsStoreMut, Engine, ExternType, Function, FunctionEnv, Imports, Instance, Module, RuntimeError, Store,
    WasmPtr,
};
use wasmer_middlewares::metering::{get_remaining_points, set_remaining_points, MeteringPoints};

pub const DEFAULT_EXECUTION_BUDGET: i64 = 1_000_000_000_000_000;

fn get_remaining_points_value(ctx: &mut impl AsStoreMut, instance: &Instance) -> i64 {
    let remaining_points = get_remaining_points(ctx, instance);
    match remaining_points {
        MeteringPoints::Remaining(x) => x as i64,
        MeteringPoints::Exhausted => 0,
    }
}

fn entity_from_function_name(fn_name: &str) -> Option<(DescribedEntityType, &str)> {
    for (prefix, ty) in [
        (DESCRIBE_TABLE_DUNDER, DescribedEntityType::Table),
        (DESCRIBE_REDUCER_DUNDER, DescribedEntityType::Reducer),
        (DESCRIBE_REPEATING_REDUCER_DUNDER, DescribedEntityType::RepeatingReducer),
    ] {
        if let Some(name) = fn_name.strip_prefix(prefix) {
            return Some((ty, name));
        }
    }
    None
}

fn log_traceback(func_type: &str, func: &str, e: &RuntimeError) {
    let frames = e.trace();
    let frames_len = frames.len();

    log::info!("{} \"{}\" runtime error: {}", func_type, func, e.message());
    for (i, frame) in frames.iter().enumerate().take(frames_len) {
        log::info!(
            "  Frame #{}: {:?}::{:?}",
            frames_len - i,
            frame.module_name(),
            frame.function_name().unwrap_or("<func>")
        );
    }
}

pub struct WasmerModule {
    module: Module,
    engine: Engine,
    abi: abi::SpacetimeAbiVersion,
}

impl WasmerModule {
    pub fn new(module: Module, engine: Engine, abi: abi::SpacetimeAbiVersion) -> Self {
        WasmerModule { module, engine, abi }
    }

    fn imports(&self, store: &mut Store, env: &FunctionEnv<WasmInstanceEnv>) -> Imports {
        let abi::SpacetimeAbiVersion::V0 = self.abi;
        imports! {
            "spacetime_v0" => {
                "_delete_pk" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_pk,
                ),
                "_delete_value" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_value,
                ),
                "_delete_eq" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_eq,
                ),
                "_delete_range" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::delete_range,
                ),
                "_insert" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::insert,
                ),
                "_create_table" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::create_table,
                ),
                "_get_table_id" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::get_table_id,
                ),
                "_iter" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::iter
                ),
                "_console_log" => Function::new_typed_with_env(
                    store,
                    env,
                    WasmInstanceEnv::console_log
                ),
            }
        }
    }
}

impl host_actor::WasmModule for WasmerModule {
    type Instance = WasmerInstance;

    type ExternType = ExternType;

    fn get_export(&self, s: &str) -> Option<Self::ExternType> {
        self.module
            .exports()
            .find(|exp| exp.name() == s)
            .map(|exp| exp.ty().clone())
    }

    fn fill_general_funcnames(&self, func_names: &mut FuncNames) -> anyhow::Result<()> {
        self.module
            .exports()
            .try_for_each(|exp| func_names.update_from_general(exp.name(), exp.ty()))
    }

    fn create_instance(&mut self, env: InstanceEnv) -> anyhow::Result<Self::Instance> {
        let mut store = Store::new(&self.engine);
        let env = WasmInstanceEnv {
            instance_env: env,
            mem: None,
        };
        let env = FunctionEnv::new(&mut store, env);
        let import_object = self.imports(&mut store, &env);

        let instance = Instance::new(&mut store, &self.module, &import_object)?;

        let mem = Mem::extract(&store, &instance.exports).context("couldn't access memory exports")?;
        env.as_mut(&mut store).mem = Some(mem);

        // Note: this budget is just for INIT_PANIC_DUNDER.
        let points = DEFAULT_EXECUTION_BUDGET;
        set_remaining_points(&mut store, &instance, points as u64);

        // Init panic if available
        let init_panic = instance.exports.get_typed_function::<(), ()>(&store, INIT_PANIC_DUNDER);
        if let Ok(init_panic) = init_panic {
            match init_panic.call(&mut store) {
                Ok(_) => {}
                Err(err) => {
                    log::warn!("Error initializing panic: {}", err);
                }
            }
        }

        Ok(WasmerInstance { store, env, instance })
    }
}

pub struct WasmerInstance {
    store: Store,
    env: FunctionEnv<WasmInstanceEnv>,
    instance: Instance,
}

impl WasmerInstance {
    fn call_describer(
        &mut self,
        describer: &Function,
        describer_func_name: &str,
        descr_type: DescribedEntityType,
    ) -> Result<Option<EntityDef>, anyhow::Error> {
        let start = std::time::Instant::now();
        log::trace!("Start describer \"{}\"...", describer_func_name);

        let store = &mut self.store;
        let result = describer.call(store, &[]);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        match result {
            Err(err) => {
                log_traceback("describer", describer_func_name, &err);
                Err(anyhow!("Could not invoke describer function {}", describer_func_name))
            }
            Ok(ret) => {
                if ret.is_empty() {
                    return Err(anyhow!("Invalid return buffer arguments from {}", describer_func_name));
                }

                // The return value of the describer is a pointer to a vector.
                // The upper 32 bits of the 64-bit result is the offset into memory.
                // The lower 32 bits is its length
                let return_value = ret.first().unwrap().i64().unwrap() as u64;
                let offset = WasmPtr::new((return_value >> 32) as u32);
                let length = (return_value & 0xffffffff) as u32;

                // We have to copy all the memory out in order to use this.
                // This would be nice to avoid... and just somehow pass the memory contents directly
                // through to the TupleDef decode, but Wasmer's use of Cell prevents us from getting
                // a nice contiguous block of bytes?
                let mem = self.env.as_ref(store).mem().clone();
                let bytes = mem.read_output_bytes(store, offset, length).context("invalid ptr")?;
                mem.dealloc(store, offset, length).context("failed to dealloc")?;

                // Decode the memory as EntityDef.
                let result = match descr_type {
                    DescribedEntityType::Table => {
                        let table = TableDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Table(table)
                    }
                    DescribedEntityType::Reducer => {
                        let reducer = ReducerDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Reducer(reducer)
                    }
                    DescribedEntityType::RepeatingReducer => {
                        let repeater = RepeaterDef::decode(&mut &bytes[..])
                            .with_context(|| format!("argument tuples has invalid schema: {}", describer_func_name))?;
                        EntityDef::Repeater(repeater)
                    }
                };

                Ok(Some(result))
            }
        }
    }
}

impl host_actor::WasmInstance for WasmerInstance {
    fn extract_descriptions(&mut self) -> anyhow::Result<HashMap<String, EntityDef>> {
        let mut map = HashMap::new();
        let functions = self.instance.exports.iter().functions();
        let describes = functions.filter_map(|(func_name, func)| {
            entity_from_function_name(func_name).map(|(descr_type, entity_name)| {
                (func_name.to_owned(), func.clone(), descr_type, entity_name.to_owned())
            })
        });
        let describes = describes.collect::<Vec<_>>();
        for (func_name, func, descr_type, entity_name) in describes {
            let description = self
                .call_describer(&func, &func_name, descr_type)?
                .ok_or_else(|| anyhow!("Bad describe function returned None; {}", func_name))?;

            map.insert(entity_name, description);
        }
        Ok(map)
    }

    type Trap = wasmer::RuntimeError;

    fn call_migrate(
        &mut self,
        func_names: &FuncNames,
        id: usize,
        budget: ReducerBudget,
    ) -> (host_actor::EnergyStats, Option<host_actor::ExecuteResult<Self::Trap>>) {
        self.call_reducer(&func_names.migrates[id], budget, b"")
    }

    fn call_reducer(
        &mut self,
        reducer_symbol: &str,
        points: ReducerBudget,
        arg_bytes: &[u8],
    ) -> (host_actor::EnergyStats, Option<host_actor::ExecuteResult<Self::Trap>>) {
        let store = &mut self.store;
        let instance = &self.instance;
        set_remaining_points(store, instance, max(points.0, 0) as u64);

        let mem = self.env.as_ref(store).mem().clone();

        let (ptr, len) = match mem.alloc_slice(store, arg_bytes) {
            Ok(ptr) => ptr,
            Err(e) => {
                if let Some(e) = e.downcast_ref() {
                    log_traceback("allocation", "alloc", e);
                }
                let remaining_points = get_remaining_points_value(store, instance);
                let used_points = points.0 - remaining_points;
                return (
                    host_actor::EnergyStats {
                        used: used_points,
                        remaining: remaining_points,
                    },
                    None,
                );
            }
        };

        let reduce = instance.exports.get_function(reducer_symbol).expect("invalid reducer");

        // let guard = pprof::ProfilerGuardBuilder::default().frequency(2500).build().unwrap();

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);
        // pass ownership of the `ptr` allocation into the reducer
        let result = reduce.call(store, &[ptr.into(), len.into()]);
        let duration = start.elapsed();
        let result = result.map(|ret| ret.get(0).map(|x| x.clone().try_into().unwrap()));
        let remaining_points = get_remaining_points_value(store, instance);
        log::trace!(
            "Reducer \"{}\" ran: {} us, {} eV",
            reducer_symbol,
            duration.as_micros(),
            points.0 - remaining_points
        );
        let used_energy = points.0 - remaining_points;
        (
            host_actor::EnergyStats {
                used: used_energy,
                remaining: remaining_points,
            },
            Some(host_actor::ExecuteResult {
                execution_time: duration,
                call_result: result,
            }),
        )
    }

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap) {
        log_traceback(func_type, func, trap)
    }
}
