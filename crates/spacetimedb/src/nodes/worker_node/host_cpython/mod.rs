mod cpython_module_host_actor;

use crate::hash::Hash;
use crate::nodes::worker_node::host_cpython::cpython_module_host_actor::CPythonModuleHostActor;
use crate::nodes::worker_node::module_host::ModuleHost;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use pyo3::{prelude::*, prepare_freethreaded_python, types::PyModule, wrap_pymodule};
use std::sync::{Arc, Mutex};

#[pyclass]
struct STDBBindingsClass {
    worker_database_instance: Arc<Mutex<WorkerDatabaseInstance>>,
}
#[pymethods]
impl STDBBindingsClass {
    fn console_log(self_: PyRef<'_, Self>, level: u8, s: &str) -> PyResult<()> {
        let worker_db_instance = self_.worker_database_instance.lock().unwrap();
        worker_db_instance.logger.lock().unwrap().write(level, String::from(s));
        log::debug!("MOD: {}", s);
        Ok(())
    }
}

#[pymodule]
fn stdb(_py: Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<STDBBindingsClass>()?;
    Ok(())
}

pub fn make_cpython_module_host_actor(
    worker_database_instance: WorkerDatabaseInstance,
    _module_hash: Hash,
    program_bytes: Vec<u8>,
) -> Result<ModuleHost, anyhow::Error> {
    prepare_freethreaded_python();
    let worker_database_instance = Arc::new(Mutex::new(worker_database_instance));

    Ok(ModuleHost::spawn(|_module_host| {
        let prg_module = Python::with_gil(|py| {
            // Compile the provided program code into a module.
            let program_code = String::from_utf8(program_bytes)?;

            // TODO(ryan): Support recompilation, detecting if we already have this existing module,
            // because creating the same module twice makes PyO3 unhappy.
            // TODO(ryan): more thinking about file name and module name.
            let prg_module = PyModule::from_code(py, program_code.as_str(), "reducers.py", "reducers")?;

            // Wrap the worker db instance into an instance of our custom bindings class so it can
            // be accessed inside our native functions called from python.
            let bindings = PyCell::new(
                py,
                STDBBindingsClass {
                    worker_database_instance: worker_database_instance.clone(),
                },
            )?;

            // Instantiate our custom 'stdb' module, and stick the bindings as an instance on
            // there.
            // Python programs will see this as 'stdb.bindings'
            let extensions_module: PyObject = wrap_pymodule!(stdb)(py).into();
            extensions_module.setattr(py, "bindings", bindings)?;
            prg_module.add("stdb", extensions_module)?;
            let prg_module: PyObject = prg_module.into();
            Ok(prg_module)
        });
        match prg_module {
            Ok(prg_module) => Ok(Box::new(CPythonModuleHostActor {
                prg_module,
                worker_database_instance: worker_database_instance.clone(),
            })),
            Err(e) => Err(e),
        }
    }))
}
