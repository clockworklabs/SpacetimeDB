//! Types for ONNX inference, used in the ABI between
//! SpacetimeDB host and guest WASM modules.
//!
//! These types are BSATN-encoded for interchange across the WASM boundary.

use spacetimedb_sats::SpacetimeType;

/// A tensor for ONNX inference, with shape metadata and flattened f32 data.
///
/// Data is stored in row-major order (C-order).
/// For example, a 2x3 matrix `[[1,2,3],[4,5,6]]` would have:
/// - `shape: [2, 3]`
/// - `data: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]`
#[derive(Clone, Debug, SpacetimeType)]
#[sats(crate = crate, name = "OnnxTensor")]
pub struct Tensor {
    /// The dimensions of the tensor, e.g. `[1, 10]` for a 1x10 matrix.
    pub shape: Vec<u32>,
    /// Flattened f32 data in row-major order.
    pub data: Vec<f32>,
}

/// An opaque handle to a loaded ONNX model on the host.
///
/// Returned by `onnx_load_model` and passed to `onnx_run_inference`.
pub type ModelHandle = u32;
