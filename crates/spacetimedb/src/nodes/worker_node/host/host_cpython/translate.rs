use pyo3::{IntoPy, PyObject, Python};
use pyo3::types::{PyDict, PyList, PyString};
use serde_json::Value;

// Recursively translate JSON into PyObjects, doing a naive translation of the fundamental types to
// their Python equivalents.
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
                dict.set_item(kv.0.as_str(), translate_json(py, kv.1))
                    .expect("Unable to set dict key")
            }
            PyObject::from(dict)
        }
    }
}

// Perform argument translation from JSON.
pub fn translate_json_arguments(
    py: Python<'_>,
    argument_bytes_json: impl AsRef<[u8]>,
) -> Result<Vec<PyObject>, anyhow::Error> {
    let args_json: Vec<Value> = serde_json::from_slice(argument_bytes_json.as_ref())?;
    let mut values = Vec::with_capacity(args_json.len());
    for x in args_json {
        values.push(translate_json(py, &x))
    }
    Ok(values)
}
