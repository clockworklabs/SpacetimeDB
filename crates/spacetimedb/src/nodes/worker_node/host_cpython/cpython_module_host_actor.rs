use crate::nodes::worker_node::module_host::{ModuleHostActor, ModuleHostCommand};
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use anyhow::anyhow;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFunction, PyList, PyString};
use serde_json::Value;
use std::sync::{Arc, Mutex};

// Turn serde_json arguments into PyObjects.
fn translate_json(py: Python<'_>, v: &Value) -> PyObject {
    match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Number(n) => {
            if n.is_f64() {
                n.as_f64().unwrap().into_py(py)
            } else {
                n.as_i64().unwrap().into_py(py)
            }
        }
        Value::String(s) => PyObject::from(PyString::new(py, s)),
        Value::Array(a) => PyObject::from(PyList::new(py, a.iter().map(|vv| translate_json(py, vv)))),
        Value::Object(o) => {
            let dict = PyDict::new(py);
            for kv in o {
                dict.setattr(kv.0.as_str(), translate_json(py, kv.1))
                    .expect("Unable to set dict key")
            }
            PyObject::from(dict)
        }
    }
}

// Perform argument translation from JSON.
fn translate_arguments(py: Python<'_>, argument_bytes_json: Vec<u8>) -> Result<Py<PyAny>, anyhow::Error> {
    let v: Value = serde_json::from_slice(argument_bytes_json.as_slice())?;
    Ok(translate_json(py, &v))
}

pub(crate) struct CPythonModuleHostActor {
    pub prg_module: PyObject,
    pub worker_database_instance: Arc<Mutex<WorkerDatabaseInstance>>,
}

impl CPythonModuleHostActor {
    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let worker_db_inst = self.worker_database_instance.lock().unwrap();
        let mut stdb = worker_db_inst.relational_db.lock().unwrap();
        stdb.reset_hard()?;
        Ok(())
    }
}

impl ModuleHostActor for CPythonModuleHostActor {
    fn handle_message(&mut self, message: ModuleHostCommand) -> bool {
        match message {
            ModuleHostCommand::CallConnectDisconnect { .. } => {
                log::debug!("CallConnectDisconnect");
                false
            }
            ModuleHostCommand::CallReducer {
                caller_identity,
                reducer_name,
                arg_bytes,
                respond_to,
            } => {
                log::debug!("CallReducer: {}/{}", caller_identity.to_hex(), reducer_name);
                let result: Result<(), anyhow::Error> = Python::with_gil(|py| {
                    let reducer_name = PyString::new(py, reducer_name.as_str());
                    let reducer = match self.prg_module.getattr(py, reducer_name) {
                        Ok(reducer) => reducer,
                        Err(e) => return Err(anyhow!("Unable to find reducer with name: {}: {}", reducer_name, e)),
                    };
                    let reducer: PyResult<&PyFunction> = reducer.extract(py);
                    let reducer = match reducer {
                        Ok(reducer) => reducer,
                        Err(e) => return Err(anyhow!("Unable to extract reducer with name: {}: {}", reducer_name, e)),
                    };

                    let arguments = translate_arguments(py, arg_bytes)?;
                    match reducer.call((caller_identity.to_hex(), arguments), None) {
                        // TODO(ryan): What do we do with the results?
                        Ok(_result) => Ok(()),
                        Err(e) => Err(anyhow!("Unable to call reducer with name: {}: {}", reducer_name, e)),
                    }
                });
                respond_to.send(result).unwrap();
                false
            }
            ModuleHostCommand::CallRepeatingReducer { .. } => {
                log::debug!("CallRepeatingReducer");
                false
            }
            ModuleHostCommand::StartRepeatingReducers => {
                log::debug!("StartRepeatingReducers");
                false
            }
            ModuleHostCommand::InitDatabase { respond_to } => {
                log::debug!("InitDatabase");
                respond_to.send(Ok(())).unwrap();
                false
            }
            ModuleHostCommand::DeleteDatabase { respond_to } => {
                respond_to.send(self.delete_database()).unwrap();
                true
            }
            ModuleHostCommand::_MigrateDatabase { .. } => {
                log::debug!("_MigrateDatabase");
                false
            }
            ModuleHostCommand::AddSubscriber { .. } => {
                log::debug!("AddSubscriber");
                false
            }
            ModuleHostCommand::RemoveSubscriber { .. } => {
                log::debug!("RemoveSubscriber");
                false
            }
            ModuleHostCommand::Exit { .. } => {
                log::debug!("Exit");
                false
            }
        }
    }
}
