//! ONNX inference support for SpacetimeDB modules.
//!
//! Load an ONNX model by name and run inference from within reducers or procedures.
//! Models are stored on the host filesystem — the model bytes never enter WASM memory.
//!
//! # Example
//!
//! ```no_run
//! # use spacetimedb::{reducer, ReducerContext, onnx::{OnnxClient, Tensor, ModelHandle}};
//! // In a reducer:
//! # #[reducer]
//! # fn my_reducer(ctx: &ReducerContext) {
//! // Load a model by name — the host resolves "bot_brain" to a .onnx file on disk.
//! let model = ctx.onnx.load("bot_brain").expect("Failed to load model");
//! let input = vec![Tensor {
//!     shape: vec![1, 10],
//!     data: vec![0.0; 10],
//! }];
//! let output = ctx.onnx.run(&model, &input).expect("Inference failed");
//! log::info!("Output: {:?}", output[0].data);
//! # }
//! ```

use crate::rt::read_bytes_source_as;
use spacetimedb_lib::bsatn;

pub use spacetimedb_lib::onnx::Tensor;

/// An opaque handle to a loaded ONNX model on the host.
///
/// Obtained via [`OnnxClient::load`] and used with [`OnnxClient::run`].
/// The model is freed when this handle is dropped.
pub struct ModelHandle(u32);

impl Drop for ModelHandle {
    fn drop(&mut self) {
        spacetimedb_bindings_sys::onnx::close_model(self.0);
    }
}

/// Client for performing ONNX inference.
///
/// Access from within reducers via [`ReducerContext::onnx`](crate::ReducerContext)
/// or from procedures via [`ProcedureContext::onnx`](crate::ProcedureContext).
#[non_exhaustive]
pub struct OnnxClient {}

impl OnnxClient {
    /// Load an ONNX model by name from the host's model storage.
    ///
    /// The host resolves the name to a `.onnx` file on its filesystem
    /// (e.g. in the database's `models/` directory), then loads and optimizes it
    /// entirely on the host side. The model bytes never enter WASM memory.
    ///
    /// The returned [`ModelHandle`] can be used with [`OnnxClient::run`] for inference.
    /// The model is automatically freed when the handle is dropped.
    pub fn load(&self, model_name: &str) -> Result<ModelHandle, Error> {
        match spacetimedb_bindings_sys::onnx::load_model(model_name) {
            Ok(handle) => Ok(ModelHandle(handle)),
            Err(err_source) => {
                let message = read_bytes_source_as::<String>(err_source);
                Err(Error { message })
            }
        }
    }

    /// Run inference on a loaded model.
    ///
    /// `inputs` are the input tensors for the model, in the order expected by the model's input nodes.
    /// Returns the output tensors from the model.
    ///
    /// Inference runs entirely on the host in native Rust — only the input/output tensor data
    /// crosses the WASM boundary.
    pub fn run(&self, model: &ModelHandle, inputs: &[Tensor]) -> Result<Vec<Tensor>, Error> {
        let input_bsatn = bsatn::to_vec(inputs).expect("Failed to BSATN-serialize input tensors");

        match spacetimedb_bindings_sys::onnx::run_inference(model.0, &input_bsatn) {
            Ok(output_source) => {
                let output = read_bytes_source_as::<Vec<Tensor>>(output_source);
                Ok(output)
            }
            Err(err_source) => {
                let message = read_bytes_source_as::<String>(err_source);
                Err(Error { message })
            }
        }
    }
}

/// An error from ONNX model loading or inference.
#[derive(Clone, Debug)]
pub struct Error {
    message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
