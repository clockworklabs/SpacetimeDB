use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use pyo3::prelude::*;
use pyo3::{PyRef, PyResult, Python};

#[pyclass]
pub(crate) struct STDBBindingsClass {
    pub worker_database_instance: WorkerDatabaseInstance,
}
#[pymethods]
impl STDBBindingsClass {
    fn console_log(self_: PyRef<'_, Self>, level: u8, s: &str) -> PyResult<()> {
        self_
            .worker_database_instance
            .logger
            .lock()
            .unwrap()
            .write(level, String::from(s));
        log::debug!("MOD: {}", s);
        Ok(())
    }
}

#[pymodule]
pub fn stdb(_py: Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<STDBBindingsClass>()?;
    Ok(())
}
