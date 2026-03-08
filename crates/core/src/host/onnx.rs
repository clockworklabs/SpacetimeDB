//! Host-side ONNX inference using tract-onnx.
//!
//! Provides [`OnnxModel`], which wraps a loaded and optimized tract model
//! and can run inference with tensors passed from WASM modules.
//!
//! Models are loaded from the host filesystem by name — the model bytes
//! never enter WASM memory. Only input/output tensor data crosses the boundary.

use crate::host::instance_env::InstanceEnv;
use spacetimedb_lib::onnx::Tensor as StdbTensor;
use tract_onnx::prelude::*;

/// A loaded and optimized ONNX model, ready for inference.
pub struct OnnxModel {
    model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
}

impl OnnxModel {
    /// Load an ONNX model by name from the host's model storage.
    ///
    /// Resolves the name to `{models_dir}/{name}.onnx` on the host filesystem,
    /// reads the file, parses, optimizes, and compiles it into a runnable plan.
    /// The model bytes never enter WASM memory.
    pub fn load_by_name(name: &str, instance_env: &InstanceEnv) -> Result<Self, OnnxError> {
        // Validate the model name to prevent path traversal.
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.is_empty() {
            return Err(OnnxError(format!("Invalid model name: {name:?}")));
        }

        let models_dir = instance_env
            .models_dir
            .as_ref()
            .ok_or_else(|| OnnxError("ONNX models directory not configured".into()))?;

        let model_path = models_dir.join(format!("{name}.onnx"));

        if !model_path.exists() {
            return Err(OnnxError(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        let model_bytes = std::fs::read(&model_path)
            .map_err(|e| OnnxError(format!("Failed to read model file {}: {e}", model_path.display())))?;

        Self::load_from_bytes(&model_bytes)
    }

    /// Load an ONNX model from raw bytes.
    fn load_from_bytes(model_bytes: &[u8]) -> Result<Self, OnnxError> {
        let model = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(model_bytes))
            .map_err(|e| OnnxError(format!("Failed to parse ONNX model: {e}")))?
            .into_optimized()
            .map_err(|e| OnnxError(format!("Failed to optimize ONNX model: {e}")))?
            .into_runnable()
            .map_err(|e| OnnxError(format!("Failed to compile ONNX model: {e}")))?;

        Ok(OnnxModel { model })
    }

    /// Run inference with the given input tensors.
    ///
    /// Returns the output tensors from the model.
    pub fn run(&self, inputs: &[StdbTensor]) -> Result<Vec<StdbTensor>, OnnxError> {
        let tract_inputs: Vec<TValue> = inputs
            .iter()
            .map(|t| {
                let shape: Vec<usize> = t.shape.iter().map(|&d| d as usize).collect();
                let tensor = tract_ndarray::Array::from_shape_vec(
                    tract_ndarray::IxDyn(&shape),
                    t.data.clone(),
                )
                .map_err(|e| OnnxError(format!("Invalid tensor shape: {e}")))?;
                Ok(tensor.into_tvalue())
            })
            .collect::<Result<Vec<_>, OnnxError>>()?;

        let result = self
            .model
            .run(tract_inputs.into())
            .map_err(|e| OnnxError(format!("Inference failed: {e}")))?;

        let outputs: Vec<StdbTensor> = result
            .iter()
            .map(|t| {
                let shape: Vec<u32> = t.shape().iter().map(|&d| d as u32).collect();
                let data: Vec<f32> = t
                    .as_slice::<f32>()
                    .map_err(|e| OnnxError(format!("Output tensor is not f32: {e}")))?
                    .to_vec();
                Ok(StdbTensor { shape, data })
            })
            .collect::<Result<Vec<_>, OnnxError>>()?;

        Ok(outputs)
    }
}

/// An error from ONNX model loading or inference.
#[derive(Debug)]
pub struct OnnxError(pub String);

impl std::fmt::Display for OnnxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OnnxError {}
