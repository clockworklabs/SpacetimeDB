use pyo3::prepare_freethreaded_python;

use crate::hash::Hash;
use crate::nodes::worker_node::host_cpython::cpython_module_host_actor::CPythonModuleHostActor;
use crate::nodes::worker_node::module_host::ModuleHost;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;

mod cpython_bindings;
mod cpython_instance_env;
mod cpython_module_host_actor;
mod translate;

pub fn make_cpython_module_host_actor(
    worker_database_instance: WorkerDatabaseInstance,
    module_hash: Hash,
    program_bytes: Vec<u8>,
) -> Result<ModuleHost, anyhow::Error> {
    prepare_freethreaded_python();

    Ok(ModuleHost::spawn(|module_host| {
        Ok(Box::from(CPythonModuleHostActor::new(
            worker_database_instance,
            module_hash,
            module_host,
            program_bytes,
        )))
    }))
}
