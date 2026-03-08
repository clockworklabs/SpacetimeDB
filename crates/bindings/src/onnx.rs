//! ONNX inference support for SpacetimeDB modules.
//!
//! Run ONNX model inference from within reducers or procedures.
//! Models are stored on the host filesystem — the model bytes never enter WASM memory.
//! Models are cached on the host after first load.
//!
//! # Example
//!
//! ```no_run
//! # use spacetimedb::{reducer, ReducerContext, onnx::{OnnxClient, Tensor}};
//! // In a reducer:
//! # #[reducer]
//! # fn my_reducer(ctx: &ReducerContext) {
//! let input = vec![Tensor {
//!     shape: vec![1, 10],
//!     data: vec![0.0; 10],
//! }];
//! let output = ctx.onnx.run("bot_brain", &input).expect("Inference failed");
//! log::info!("Output: {:?}", output[0].data);
//! # }
//! ```

use crate::rt::read_bytes_source_as;
use spacetimedb_lib::bsatn;

pub use spacetimedb_lib::onnx::Tensor;

/// Client for performing ONNX inference.
///
/// Access from within reducers via [`ReducerContext::onnx`](crate::ReducerContext)
/// or from procedures via [`ProcedureContext::onnx`](crate::ProcedureContext).
#[non_exhaustive]
pub struct OnnxClient {}

impl OnnxClient {
    /// Run inference on a named ONNX model.
    ///
    /// The host resolves `model_name` to a `.onnx` file on its filesystem,
    /// loads and caches it on first use, then runs inference with the given inputs.
    /// Model bytes never enter WASM memory — only tensor data crosses the boundary.
    ///
    /// `inputs` are the input tensors for the model, in the order expected by the model's input nodes.
    /// Returns the output tensors from the model.
    pub fn run(&self, model_name: &str, inputs: &[Tensor]) -> Result<Vec<Tensor>, Error> {
        let input_bsatn = bsatn::to_vec(inputs).expect("Failed to BSATN-serialize input tensors");

        match spacetimedb_bindings_sys::onnx::run(model_name, &input_bsatn) {
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
