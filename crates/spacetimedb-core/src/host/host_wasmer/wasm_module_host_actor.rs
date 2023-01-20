use super::wasm_instance_env::WasmInstanceEnv;
use super::{Buffer, Mem};
use crate::host::host_controller::{DescribedEntityType, ReducerBudget};
use crate::host::instance_env::InstanceEnv;
use crate::host::timestamp::Timestamp;
use crate::host::wasm_common::*;
use anyhow::{anyhow, Context};
use spacetimedb_lib::{EntityDef, ReducerDef, TableDef};
use std::cmp::max;
use std::collections::HashMap;
use wasmer::{
    imports, AsStoreMut, Engine, ExternType, Function, FunctionEnv, Imports, Instance, Module, RuntimeError, Store,
    TypedFunction,
};
use wasmer_middlewares::metering::{get_remaining_points, set_remaining_points, MeteringPoints};

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
            "  Frame #{}: {:?}::{}",
            frames_len - i,
            frame.module_name(),
            rustc_demangle::demangle(frame.function_name().unwrap_or("<func>"))
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
                "_schedule_reducer" => Function::new_typed_with_env(store, env, WasmInstanceEnv::schedule_reducer),
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
                "_buffer_len" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_len),
                "_buffer_consume" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_consume),
                "_buffer_alloc" => Function::new_typed_with_env(store, env, WasmInstanceEnv::buffer_alloc),
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

    fn create_instance(&mut self, func_names: &FuncNames, env: InstanceEnv) -> anyhow::Result<Self::Instance> {
        let mut store = Store::new(&self.engine);
        let env = WasmInstanceEnv {
            instance_env: env,
            mem: None,
            buffers: Default::default(),
        };
        let env = FunctionEnv::new(&mut store, env);
        let import_object = self.imports(&mut store, &env);

        let instance = Instance::new(&mut store, &self.module, &import_object)?;

        let mem = Mem::extract(&instance.exports).context("couldn't access memory exports")?;
        env.as_mut(&mut store).mem = Some(mem);

        // Note: this budget is just for initializers
        let points = DEFAULT_EXECUTION_BUDGET;
        set_remaining_points(&mut store, &instance, points as u64);

        for preinit in &func_names.preinits {
            let func = instance.exports.get_typed_function::<(), ()>(&store, preinit).unwrap();
            func.call(&mut store)
                .with_context(|| format!("preinit {preinit:?} trapped"))?;
        }

        let init = instance.exports.get_typed_function::<(), u32>(&store, SETUP_DUNDER);
        if let Ok(init) = init {
            match init.call(&mut store) {
                Ok(errbuf) => {
                    let errbuf = Buffer { raw: errbuf };
                    if !errbuf.is_invalid() {
                        let errbuf = env
                            .as_mut(&mut store)
                            .take_buf(errbuf)
                            .unwrap_or_else(|| "unknown error".as_bytes().into());
                        let errbuf = String::from_utf8_lossy(&errbuf);
                        // TODO: catch this and return the error message to the http client
                        anyhow::bail!("Error returned from __setup__: {}", errbuf)
                    }
                }
                Err(err) => {
                    anyhow::bail!("Trap while running __setup__: {}", err)
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
        let describer = describer.typed::<(), u32>(store)?;
        let result = describer.call(store);
        let duration = start.elapsed();
        log::trace!("Describer \"{}\" ran: {} us", describer_func_name, duration.as_micros(),);
        match result {
            Err(err) => {
                log_traceback("describer", describer_func_name, &err);
                Err(anyhow!("Could not invoke describer function {}", describer_func_name))
            }
            Ok(buf) => {
                let bytes = self
                    .env
                    .as_mut(store)
                    .take_buf(Buffer { raw: buf })
                    .context("invalid buffer")?;
                self.env.as_mut(store).clear_bufs();

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
    ) -> (host_actor::EnergyStats, host_actor::ExecuteResult<Self::Trap>) {
        self.call_tx_function::<(), 0>(&func_names.migrates[id], budget, [], |func, store, []| func.call(store))
    }

    fn call_reducer(
        &mut self,
        reducer_symbol: &str,
        points: ReducerBudget,
        sender: &[u8; 32],
        timestamp: Timestamp,
        arg_bytes: Vec<u8>,
    ) -> (host_actor::EnergyStats, host_actor::ExecuteResult<Self::Trap>) {
        self.call_tx_function::<(u32, u64, u32), 2>(
            reducer_symbol,
            points,
            [sender.to_vec(), arg_bytes],
            |func, store, [sender, args]| func.call(store, sender.raw, timestamp.0, args.raw),
        )
    }

    fn call_connect_disconnect(
        &mut self,
        connect: bool,
        budget: ReducerBudget,
        sender: &[u8; 32],
        timestamp: Timestamp,
    ) -> (host_actor::EnergyStats, host_actor::ExecuteResult<Self::Trap>) {
        self.call_tx_function::<(u32, u64), 1>(
            if connect {
                IDENTITY_CONNECTED_DUNDER
            } else {
                IDENTITY_DISCONNECTED_DUNDER
            },
            budget,
            [sender.to_vec()],
            |func, store, [sender]| func.call(store, sender.raw, timestamp.0),
        )
    }

    fn log_traceback(func_type: &str, func: &str, trap: &Self::Trap) {
        log_traceback(func_type, func, trap)
    }
}

impl WasmerInstance {
    fn call_tx_function<Args: wasmer::WasmTypeList, const N_BUFS: usize>(
        &mut self,
        reducer_symbol: &str,
        points: ReducerBudget,
        bufs: [Vec<u8>; N_BUFS],
        // would be nicer if there was a TypedFunction::call_tuple(&self, store, ArgsTuple)
        call: impl FnOnce(TypedFunction<Args, u32>, &mut Store, [Buffer; N_BUFS]) -> Result<u32, RuntimeError>,
    ) -> (host_actor::EnergyStats, host_actor::ExecuteResult<RuntimeError>) {
        let store = &mut self.store;
        let instance = &self.instance;
        set_remaining_points(store, instance, max(points.0, 0) as u64);

        let reduce = instance
            .exports
            .get_typed_function::<Args, u32>(store, reducer_symbol)
            .expect("invalid reducer");

        let bufs = bufs.map(|data| self.env.as_mut(store).alloc_buf(data));

        // let guard = pprof::ProfilerGuardBuilder::default().frequency(2500).build().unwrap();

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);
        // pass ownership of the `ptr` allocation into the reducer
        let result = call(reduce, store, bufs).and_then(|errbuf| {
            let errbuf = Buffer { raw: errbuf };
            Ok(if errbuf.is_invalid() {
                Ok(())
            } else {
                let errmsg = self
                    .env
                    .as_mut(store)
                    .take_buf(errbuf)
                    .ok_or_else(|| RuntimeError::new("invalid buffer handle"))?;
                Err(string_from_utf8_lossy_owned(errmsg))
            })
        });
        self.env.as_mut(store).clear_bufs();
        // .call(store, sender_buf.ptr.cast(), timestamp, args_buf.ptr, args_buf.len)
        // .and_then(|_| {});
        let duration = start.elapsed();
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
            host_actor::ExecuteResult {
                execution_time: duration,
                call_result: result,
            },
        )
    }
}

fn string_from_utf8_lossy_owned(v: Box<[u8]>) -> Box<str> {
    match String::from_utf8_lossy(&v) {
        // SAFETY: from_utf8_lossy() returned Borrowed, which means the original buffer is valid utf8
        std::borrow::Cow::Borrowed(_) => unsafe { Box::<str>::from_raw(Box::into_raw(v) as *mut str) },
        std::borrow::Cow::Owned(s) => s.into_boxed_str(),
    }
}
