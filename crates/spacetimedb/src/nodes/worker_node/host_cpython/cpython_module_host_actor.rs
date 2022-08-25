use crate::nodes::worker_node::host_cpython::translate::translate_arguments;
use crate::nodes::worker_node::module_host::{ModuleHostActor, ModuleHostCommand};
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use anyhow::anyhow;
use pyo3::prelude::*;
use pyo3::types::{PyFunction, PyString};

pub(crate) struct CPythonModuleHostActor {
    pub prg_module: PyObject,
    pub worker_database_instance: WorkerDatabaseInstance,
}

impl CPythonModuleHostActor {
    fn delete_database(&mut self) -> Result<(), anyhow::Error> {
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
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
