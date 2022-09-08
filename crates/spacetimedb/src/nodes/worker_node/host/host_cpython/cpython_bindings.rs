use pyo3::{PyRef, PyResult, Python};
use pyo3::prelude::*;

use crate::nodes::worker_node::host::instance_env::InstanceEnv;

#[pyclass]
pub(crate) struct STDBBindingsClass {
    pub instance_env: InstanceEnv,
}

// Note these methods all take byte buffers and do serialization/deserialization of TypeValue and
// friends, with the implication of a corresponding TypeValue etc implementations on the Python side
// because that's how the WASM implementation works.
// But the "right" way to do this -- given that the python process is local -- is probably to do a
// direct translation between PyObjects and TypeValue etc. and vice versa.
// However, as this code will, in the long run, likely live on the other side of an IPC boundary,
// we will probably end up sticking with byte buffer usage here.

#[pymethods]
impl STDBBindingsClass {
    fn console_log(self_: PyRef<'_, Self>, level: u8, s: &str) -> PyResult<()> {
        self_.instance_env.console_log(level, &String::from(s));
        Ok(())
    }

    fn insert(self_: PyRef<'_, Self>, table_id: u32, buffer: Vec<u8>) {
        self_.instance_env.insert(table_id, bytes::Bytes::from(buffer));
    }

    pub fn delete_pk(self_: PyRef<'_, Self>, table_id: u32, buffer: Vec<u8>) -> u8 {
        self_.instance_env.delete_pk(table_id, bytes::Bytes::from(buffer))
    }

    pub fn delete_value(self_: PyRef<'_, Self>, table_id: u32, buffer: Vec<u8>) -> u8 {
        self_.instance_env.delete_value(table_id, bytes::Bytes::from(buffer))
    }

    pub fn delete_eq(self_: PyRef<'_, Self>, table_id: u32, col_id: u32, buffer: Vec<u8>) -> i32 {
        self_
            .instance_env
            .delete_eq(table_id, col_id, bytes::Bytes::from(buffer))
    }

    pub fn delete_range(self_: PyRef<'_, Self>, table_id: u32, col_id: u32, buffer: Vec<u8>) -> i32 {
        self_
            .instance_env
            .delete_range(table_id, col_id, bytes::Bytes::from(buffer))
    }

    pub fn create_table(self_: PyRef<'_, Self>, buffer: Vec<u8>) -> u32 {
        self_.instance_env.create_table(bytes::Bytes::from(buffer))
    }

    pub fn iter(self_: PyRef<'_, Self>, table_id: u32) -> Vec<u8> {
        self_.instance_env.iter(table_id)
    }
}

#[pymodule]
pub(crate) fn stdb(_py: Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<STDBBindingsClass>()?;
    Ok(())
}
