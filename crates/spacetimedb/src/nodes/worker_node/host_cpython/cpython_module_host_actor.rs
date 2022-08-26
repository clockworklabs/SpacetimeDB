use std::collections::HashMap;
use std::format;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use ffi::PyObject_Call;
use pyo3::prelude::*;
use pyo3::types::{PyFunction, PyString, PyTuple};
use pyo3::{ffi, AsPyPointer};
use tokio::spawn;
use tokio::time::sleep;

use spacetimedb_bindings::args::{Arguments, RepeatingReducerArguments};
use spacetimedb_bindings::buffer::VectorBufWriter;

use crate::db::messages::transaction::Transaction;
use crate::db::transactional_db::Tx;
use crate::hash::Hash;
use crate::nodes::worker_node::client_api::client_connection::ClientActorId;
use crate::nodes::worker_node::client_api::module_subscription_actor::ModuleSubscription;
use crate::nodes::worker_node::host_cpython::cpython_bindings::STDBBindingsClass;
use crate::nodes::worker_node::host_cpython::cpython_instance_env::InstanceEnv;
use crate::nodes::worker_node::host_cpython::translate::translate_json_arguments;
use crate::nodes::worker_node::module_host::{
    EventStatus, ModuleEvent, ModuleFunctionCall, ModuleHost, ModuleHostActor, ModuleHostCommand,
};
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;

const REDUCE_DUNDER: &str = "__reducer__";
const REPEATING_REDUCER_DUNDER: &str = "__repeating_reducer__";
const CREATE_TABLE_DUNDER: &str = "__create_table__";
const MIGRATE_DATABASE_DUNDER: &str = "__migrate_database__";
const IDENTITY_CONNECTED_DUNDER: &str = "__identity_connected__";
const IDENTITY_DISCONNECTED_DUNDER: &str = "__identity_disconnected__";

// PyFunction::call does not allow us to just pass our own Tuple in.
// It only supports IntoPy<PyTuple>, and for some reason PyTuple can't be "Into" itself.
// So this is basically copy and paste from PyModule::call, to do what we need through the backdoor.
fn py_call_function<'a>(
    py: Python<'a>,
    func: &PyFunction,
    args: Py<PyTuple>,
    kwargs: Option<&pyo3::types::PyDict>,
) -> PyResult<&'a PyAny> {
    let kwargs = kwargs.into_ptr();

    unsafe {
        let return_value = PyObject_Call(func.as_ptr(), args.as_ptr(), kwargs);
        let ret = py.from_owned_ptr_or_err(return_value);
        ffi::Py_XDECREF(kwargs);
        ret
    }
}

fn empty_args() -> Py<PyTuple> {
    Python::with_gil(|py| PyTuple::empty(py).into())
}

pub(crate) struct CPythonModuleHostActor {
    worker_database_instance: WorkerDatabaseInstance,
    module_host: ModuleHost,
    instances: Vec<(u32, Py<PyModule>)>,
    instance_tx_map: Arc<Mutex<HashMap<u32, Tx>>>,
    subscription: ModuleSubscription,
}

impl CPythonModuleHostActor {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        module_hash: Hash,
        module_host: ModuleHost,
        program_bytes: Vec<u8>,
    ) -> Self {
        let relational_db = worker_database_instance.relational_db.clone();
        let subscription = ModuleSubscription::spawn(relational_db);
        let mut host = Self {
            worker_database_instance,
            module_host,
            instances: Vec::new(),
            instance_tx_map: Arc::new(Mutex::new(HashMap::new())),
            subscription,
        };
        host.create_instance(module_hash, program_bytes).unwrap();
        host
    }

    fn create_instance(&mut self, module_hash: Hash, program_bytes: Vec<u8>) -> Result<u32, anyhow::Error> {
        let instance_id = self.instances.len() as u32;

        // Compile the provided program code into a module.
        let program_code = String::from_utf8(program_bytes)?;

        let module_name = format!("instance_{}_module_{}", instance_id, module_hash.to_hex());
        let module_file_name = format!("{}.py", module_name);

        log::debug!("Creating instance {}...", module_name);
        let prg_module: Result<Py<PyModule>, PyErr> = Python::with_gil(|py| {
            let prg_module = PyModule::from_code(
                py,
                program_code.as_str(),
                module_file_name.as_str(),
                module_name.as_str(),
            )?;

            // Wrap the worker db instance into an instance of our custom bindings class so it can
            // be accessed inside our native functions called from python.
            let instance_module: Py<PyModule> = prg_module.into();
            let bindings = PyCell::new(
                py,
                STDBBindingsClass {
                    instance_env: InstanceEnv {
                        instance_id,
                        worker_database_instance: self.worker_database_instance.clone(),
                        instance_tx_map: Arc::new(Mutex::new(Default::default())),
                    },
                },
            )?;

            // Stick the bindings instance directly into the namespace of the instance module.
            // Python programs will refer to this via "SpacetimeDB"
            prg_module.add("SpacetimeDB", bindings)?;

            Ok(instance_module.clone())
        });
        let prg_module = match prg_module {
            Ok(prg_module) => prg_module,
            Err(e) => {
                return Err(anyhow!(
                    "Failure to create python module instance {}: {}",
                    module_hash.to_hex(),
                    e
                ));
            }
        };
        self.instances.push((instance_id, prg_module));

        let exported_functions = self.module_export_functions().join(", ");
        log::debug!(
            "Created instance {}; exported functions: {}",
            module_name,
            exported_functions
        );

        Ok(instance_id)
    }

    fn init_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module_export_functions() {
            if f.starts_with(CREATE_TABLE_DUNDER) {
                self.call_create_table(&f[CREATE_TABLE_DUNDER.len()..])?;
            }
        }

        // TODO: call __create_index__IndexName

        Ok(())
    }

    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        stdb.reset_hard()?;
        Ok(())
    }

    fn migrate_database(&mut self) -> Result<(), anyhow::Error> {
        for f in self.module_export_functions() {
            if !f.starts_with(MIGRATE_DATABASE_DUNDER) {
                continue;
            }
            self.call_migrate(&f[MIGRATE_DATABASE_DUNDER.len()..])?;
        }

        // TODO: call __create_index__IndexName
        Ok(())
    }

    fn add_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.subscription.add_subscriber(client_id)
    }

    fn remove_subscriber(&self, client_id: ClientActorId) -> Result<(), anyhow::Error> {
        self.subscription.remove_subscriber(client_id)
    }

    fn call_create_table(&self, create_table_name: &str) -> Result<(), anyhow::Error> {
        let create_table_symbol = format!("{}{}", CREATE_TABLE_DUNDER, create_table_name);
        let (_tx, _repeat_duration) = self.execute_reducer(&create_table_symbol, empty_args())?;
        Ok(())
    }

    fn call_migrate(&self, migrate_name: &str) -> Result<(), anyhow::Error> {
        let migrate_symbol = format!("{}{}", MIGRATE_DATABASE_DUNDER, migrate_name);
        let (_tx, _repeat_duration) = self.execute_reducer(&migrate_symbol, empty_args())?;
        Ok(())
    }

    fn call_identity_connected_disconnected(&self, identity: &Hash, connected: bool) -> Result<(), anyhow::Error> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;

        let reducer_symbol = if connected {
            IDENTITY_CONNECTED_DUNDER
        } else {
            IDENTITY_DISCONNECTED_DUNDER
        };

        let args = Python::with_gil(|py| (identity.data, timestamp).into_py(py));
        let result = self.execute_reducer(reducer_symbol, args);
        let tx = match result {
            Ok((tx, _repeat_duration)) => tx,
            Err(err) => {
                log::debug!("Error with connect/disconnect: {}", err);
                return Ok(());
            }
        };

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        // TODO(cloutiertyler): We need to think about how to handle this special
        // function. Is this just an autogenerated reducer? In the future if I call
        // a reducer from within a reducer should that generate a module event?
        // Good question, Tyler, good question.
        let event = ModuleEvent {
            timestamp,
            function_call: ModuleFunctionCall {
                reducer: reducer_symbol.to_string(),
                arg_bytes: Vec::new(),
            },
            status,
            caller_identity: *identity,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }

    fn call_repeating_reducer(&self, reducer_name: &str, prev_call_time: u64) -> Result<(u64, u64), anyhow::Error> {
        let reducer_symbol = format!("{}{}", REPEATING_REDUCER_DUNDER, reducer_name);
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let delta_time = timestamp - prev_call_time;

        let args = Python::with_gil(|py| (timestamp, delta_time).into_py(py));
        let (tx, repeat_duration) = self.execute_reducer(&reducer_symbol, args)?;

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        // TODO(ryan): we *only* use these bytes for shoving in the ModuleEvent. Dubious utility
        // until ModuleEvent can be refactored to hold something richer
        let arguments = RepeatingReducerArguments::new(timestamp, delta_time);
        let mut arg_bytes = Vec::with_capacity(arguments.encoded_size());
        let mut writer = VectorBufWriter::new(&mut arg_bytes);
        arguments.encode(&mut writer);

        let event = ModuleEvent {
            timestamp,
            caller_identity: self.worker_database_instance.identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok((repeat_duration.unwrap(), timestamp))
    }

    fn call_reducer(&self, caller_identity: Hash, reducer_name: &str, arg_bytes: &[u8]) -> Result<(), anyhow::Error> {
        // TODO: validate arg_bytes
        let reducer_symbol = format!("{}{}", REDUCE_DUNDER, reducer_name);

        log::info!("Calling python reducer {}", reducer_symbol);

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;

        let arguments: Result<Py<PyTuple>, anyhow::Error> = Python::with_gil(|py| {
            let mut arguments = vec![caller_identity.data.into_py(py), timestamp.into_py(py)];
            let mut user_arguments = translate_json_arguments(py, arg_bytes)?;
            arguments.append(&mut user_arguments);
            let arguments = PyTuple::new(py, arguments.iter());

            Ok(arguments.into())
        });

        let (tx, _repeat_duration) = self.execute_reducer(&reducer_symbol, arguments?)?;

        let status = if let Some(tx) = tx {
            EventStatus::Committed(tx.writes)
        } else {
            EventStatus::Failed
        };

        let event = ModuleEvent {
            timestamp,
            caller_identity,
            function_call: ModuleFunctionCall {
                reducer: reducer_name.to_string(),
                arg_bytes: arg_bytes.to_owned(),
            },
            status,
        };
        self.subscription.broadcast_event(event).unwrap();

        Ok(())
    }

    fn execute_reducer(
        &self,
        reducer_symbol: &str,
        arguments: Py<PyTuple>,
    ) -> Result<(Option<Transaction>, Option<u64>), anyhow::Error> {
        let tx = {
            let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
            stdb.begin_tx()
        };

        // TODO: choose one at random or whatever
        let (instance_id, instance) = &self.instances[0];
        self.instance_tx_map.lock().unwrap().insert(*instance_id, tx);

        let start = std::time::Instant::now();
        log::trace!("Start reducer \"{}\"...", reducer_symbol);

        let result: Result<Option<u64>, anyhow::Error> =
            Python::with_gil(|py| {
                let reducer_name = PyString::new(py, reducer_symbol);
                let reducer = match instance.getattr(py, reducer_name) {
                    Ok(reducer) => reducer,
                    Err(e) => {
                        return Err(anyhow::Error::new(e)
                            .context(format!("Unable to find reducer with name: {}", reducer_name)))
                    }
                };
                let reducer: PyResult<&PyFunction> = reducer.extract(py);
                let reducer = match reducer {
                    Ok(reducer) => reducer,
                    Err(e) => {
                        return Err(anyhow::Error::new(e)
                            .context(format!("Unable to extract reducer with name: {}", reducer_name)))
                    }
                };

                match py_call_function(py, reducer, arguments, None) {
                    Ok(result) => {
                        if result.is_none() {
                            Ok(None)
                        } else {
                            // TODO: blatant assumption that this is the only kind of result we can get
                            let result: u64 = result.extract()?;
                            Ok(Some(result))
                        }
                    }
                    Err(e) => {
                        return Err(anyhow::Error::new(e.clone_ref(py)).context(format!(
                            "Unable to call reducer with name: {}, error: {}",
                            reducer_name, e
                        )))
                    }
                }
            });

        let duration = start.elapsed();

        log::trace!("Reducer \"{}\" ran: {} us", reducer_symbol, duration.as_micros(),);

        match result {
            Err(err) => {
                let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
                let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
                let tx = instance_tx_map.remove(&instance_id).unwrap();
                stdb.rollback_tx(tx);

                // TODO(ryan): Make sure proper traceback is fully output here.
                log::error!("Reducer \"{}\" runtime error: {}", reducer_symbol, err);
                Ok((None, None))
            }
            Ok(repeat_duration) => {
                let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
                let mut instance_tx_map = self.instance_tx_map.lock().unwrap();
                let tx = instance_tx_map.remove(&instance_id).unwrap();
                if let Some(tx) = stdb.commit_tx(tx) {
                    stdb.txdb.message_log.sync_all().unwrap();
                    Ok((Some(tx), repeat_duration))
                } else {
                    todo!("Write skew, you need to implement retries my man, T-dawg.");
                }
            }
        }
    }

    fn module_export_functions(&self) -> Vec<String> {
        // Just pick the first instance to look for the module to inspect. In reality we should
        // probably have the compilation step compile a "canonical" module and then copy it
        // somehow for each instance instead of compiling afresh.
        // And we'd use that to look at exports etc.
        // But I'm not sure how to do this
        // with PyO3, so this is how it is for now.
        let mut functions = vec![];
        Python::with_gil(|py| {
            let module = self.instances[0].1.as_ref(py);
            let module_dict = module.dict();
            for x in module_dict {
                if x.1.is_callable() {
                    let fn_name = x.0.to_string();
                    functions.push(fn_name);
                }
            }
        });
        functions
    }

    fn start_repeating_reducers(&mut self) {
        for f in self.module_export_functions() {
            if f.starts_with(REPEATING_REDUCER_DUNDER) {
                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
                let prev_call_time = timestamp - 20;

                // TODO: We should really have another function inside of the module that we can use to get the initial repeat
                // duration. It doesn't make sense to just make up a random value here.
                let name = f[REPEATING_REDUCER_DUNDER.len()..].to_string();
                let result = self.call_repeating_reducer(&name, prev_call_time);
                let (repeat_duration, call_timestamp) = match result {
                    Ok((repeat_duration, call_timestamp)) => (repeat_duration, call_timestamp),
                    Err(err) => {
                        log::warn!("Error in repeating reducer: {}", err);
                        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
                        (20, timestamp)
                    }
                };
                let module_host = self.module_host.clone();
                let mut prev_call_time = call_timestamp;
                let mut cur_repeat_duration = repeat_duration;
                spawn(async move {
                    loop {
                        sleep(Duration::from_millis(cur_repeat_duration)).await;
                        let res = module_host.call_repeating_reducer(name.clone(), prev_call_time).await;
                        if let Err(err) = res {
                            // If we get an error trying to call this, then the module host has probably restarted
                            // just break out of the loop and end this task
                            log::debug!("Error calling repeating reducer: {}", err);
                            break;
                        }
                        if let Ok((repeat_duration, call_timestamp)) = res {
                            prev_call_time = call_timestamp;
                            cur_repeat_duration = repeat_duration;
                        }
                    }
                });
            }
        }
    }
}

impl ModuleHostActor for CPythonModuleHostActor {
    // TODO(ryan): For now this is 100% identical to WasmModuleHostActor, and that's suspicious.
    // In the long run this will likely speak over IPC to a child process.
    // When that happens, this will be replaced with a "parent process module host actor"
    // and a corresponding "child process module host actor" will use IPC and friends to proxy
    // between the two processes. And then this duplication will go away.
    fn handle_message(&mut self, message: ModuleHostCommand) -> bool {
        match message {
            ModuleHostCommand::CallConnectDisconnect {
                caller_identity,
                connected,
                respond_to,
            } => {
                respond_to
                    .send(self.call_identity_connected_disconnected(&caller_identity, connected))
                    .unwrap();
                false
            }
            ModuleHostCommand::CallReducer {
                caller_identity,
                reducer_name,
                arg_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.call_reducer(caller_identity, &reducer_name, &arg_bytes))
                    .unwrap();
                false
            }
            ModuleHostCommand::CallRepeatingReducer {
                reducer_name,
                prev_call_time,
                respond_to,
            } => {
                respond_to
                    .send(self.call_repeating_reducer(&reducer_name, prev_call_time))
                    .unwrap();
                false
            }
            ModuleHostCommand::InitDatabase { respond_to } => {
                respond_to.send(self.init_database()).unwrap();
                false
            }
            ModuleHostCommand::DeleteDatabase { respond_to } => {
                respond_to.send(self.delete_database()).unwrap();
                true
            }
            ModuleHostCommand::_MigrateDatabase { respond_to } => {
                respond_to.send(self.migrate_database()).unwrap();
                false
            }
            ModuleHostCommand::Exit {} => true,
            ModuleHostCommand::AddSubscriber { client_id, respond_to } => {
                respond_to.send(self.add_subscriber(client_id)).unwrap();
                false
            }
            ModuleHostCommand::RemoveSubscriber { client_id, respond_to } => {
                respond_to.send(self.remove_subscriber(client_id)).unwrap();
                false
            }
            ModuleHostCommand::StartRepeatingReducers => {
                self.start_repeating_reducers();
                false
            }
        }
    }
}
