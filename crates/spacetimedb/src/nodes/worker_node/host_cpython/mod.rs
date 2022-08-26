use pyo3::prepare_freethreaded_python;

use crate::hash::Hash;
use crate::nodes::worker_node::host_cpython::cpython_module_host_actor::CPythonModuleHostActor;
use crate::nodes::worker_node::module_host::ModuleHost;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use lazy_static::lazy_static;
use std::sync::Mutex;

mod cpython_bindings;
mod cpython_instance_env;
mod cpython_module_host_actor;
mod translate;

lazy_static! {
    static ref IS_PYTHON_INITIALIZED: Mutex<bool> = Mutex::new(false);
}

pub fn make_cpython_module_host_actor(
    worker_database_instance: WorkerDatabaseInstance,
    module_hash: Hash,
    program_bytes: Vec<u8>,
) -> Result<ModuleHost, anyhow::Error> {
    // Initialize the Python runtime.
    // We can only do this once per process instance.
    let mut is_inited = IS_PYTHON_INITIALIZED.lock().unwrap();
    if !*is_inited {
        prepare_freethreaded_python();
        *is_inited = true;
    }
    Ok(ModuleHost::spawn(|module_host| {
        Ok(Box::from(CPythonModuleHostActor::new(
            worker_database_instance,
            module_hash,
            module_host,
            program_bytes,
        )))
    }))
}
