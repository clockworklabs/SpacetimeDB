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
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_from_bytes(model_bytes: &[u8]) -> Result<Self, OnnxError> {
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

    /// Run inference on multiple batches of input tensors.
    ///
    /// Each element of `batches` is one set of input tensors (one inference invocation).
    /// Returns one `Vec<StdbTensor>` of outputs per batch, in the same order.
    /// This amortizes the overhead of crossing the WASM boundary for many inferences.
    pub fn run_multi(&self, batches: &[Vec<StdbTensor>]) -> Result<Vec<Vec<StdbTensor>>, OnnxError> {
        batches.iter().map(|inputs| self.run(inputs)).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use tract_onnx::pb;

    /// Build a minimal ONNX model as raw bytes using protobuf types.
    /// `op_type` is the ONNX operator (e.g. "Add", "Relu", "Identity").
    /// `n_inputs` is the number of inputs the operator expects.
    fn build_onnx_model(op_type: &str, n_inputs: usize) -> Vec<u8> {
        let input_names: Vec<String> = (0..n_inputs).map(|i| format!("input_{i}")).collect();
        let inputs: Vec<pb::ValueInfoProto> = input_names
            .iter()
            .map(|name| pb::ValueInfoProto {
                name: name.clone(),
                r#type: Some(pb::TypeProto {
                    denotation: String::new(),
                    value: Some(pb::type_proto::Value::TensorType(pb::type_proto::Tensor {
                        elem_type: 1, // FLOAT
                        shape: Some(pb::TensorShapeProto {
                            dim: vec![
                                pb::tensor_shape_proto::Dimension {
                                    denotation: String::new(),
                                    value: Some(pb::tensor_shape_proto::dimension::Value::DimValue(1)),
                                },
                                pb::tensor_shape_proto::Dimension {
                                    denotation: String::new(),
                                    value: Some(pb::tensor_shape_proto::dimension::Value::DimValue(4)),
                                },
                            ],
                        }),
                    })),
                }),
                doc_string: String::new(),
            })
            .collect();

        let output = pb::ValueInfoProto {
            name: "output".into(),
            r#type: Some(pb::TypeProto {
                denotation: String::new(),
                value: Some(pb::type_proto::Value::TensorType(pb::type_proto::Tensor {
                    elem_type: 1,
                    shape: Some(pb::TensorShapeProto {
                        dim: vec![
                            pb::tensor_shape_proto::Dimension {
                                denotation: String::new(),
                                value: Some(pb::tensor_shape_proto::dimension::Value::DimValue(1)),
                            },
                            pb::tensor_shape_proto::Dimension {
                                denotation: String::new(),
                                value: Some(pb::tensor_shape_proto::dimension::Value::DimValue(4)),
                            },
                        ],
                    }),
                })),
            }),
            doc_string: String::new(),
        };

        let node = pb::NodeProto {
            input: input_names,
            output: vec!["output".into()],
            name: "node_0".into(),
            op_type: op_type.into(),
            domain: String::new(),
            attribute: vec![],
            doc_string: String::new(),
        };

        let graph = pb::GraphProto {
            name: "test_graph".into(),
            node: vec![node],
            input: inputs.clone(),
            output: vec![output],
            initializer: vec![],
            sparse_initializer: vec![],
            doc_string: String::new(),
            value_info: vec![],
            quantization_annotation: vec![],
        };

        let model = pb::ModelProto {
            ir_version: 7,
            opset_import: vec![pb::OperatorSetIdProto {
                domain: String::new(),
                version: 13,
            }],
            producer_name: "spacetimedb-test".into(),
            graph: Some(graph),
            ..Default::default()
        };

        model.encode_to_vec()
    }

    #[test]
    fn load_and_run_add_model() {
        let model_bytes = build_onnx_model("Add", 2);
        let model = OnnxModel::load_from_bytes(&model_bytes).expect("Failed to load model");

        let a = StdbTensor {
            shape: vec![1, 4],
            data: vec![1.0, 2.0, 3.0, 4.0],
        };
        let b = StdbTensor {
            shape: vec![1, 4],
            data: vec![10.0, 20.0, 30.0, 40.0],
        };

        let outputs = model.run(&[a, b]).expect("Inference failed");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape, vec![1, 4]);
        assert_eq!(outputs[0].data, vec![11.0, 22.0, 33.0, 44.0]);
    }

    #[test]
    fn load_and_run_relu_model() {
        let model_bytes = build_onnx_model("Relu", 1);
        let model = OnnxModel::load_from_bytes(&model_bytes).expect("Failed to load model");

        let input = StdbTensor {
            shape: vec![1, 4],
            data: vec![-2.0, -1.0, 0.0, 3.0],
        };

        let outputs = model.run(&[input]).expect("Inference failed");
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].shape, vec![1, 4]);
        assert_eq!(outputs[0].data, vec![0.0, 0.0, 0.0, 3.0]);
    }

    #[test]
    fn invalid_model_bytes() {
        let result = OnnxModel::load_from_bytes(b"not a valid onnx model");
        assert!(result.is_err());
    }

    #[test]
    fn run_multi_batches() {
        let model_bytes = build_onnx_model("Add", 2);
        let model = OnnxModel::load_from_bytes(&model_bytes).expect("Failed to load model");

        let batches = vec![
            vec![
                StdbTensor { shape: vec![1, 4], data: vec![1.0, 2.0, 3.0, 4.0] },
                StdbTensor { shape: vec![1, 4], data: vec![10.0, 20.0, 30.0, 40.0] },
            ],
            vec![
                StdbTensor { shape: vec![1, 4], data: vec![5.0, 5.0, 5.0, 5.0] },
                StdbTensor { shape: vec![1, 4], data: vec![1.0, 1.0, 1.0, 1.0] },
            ],
        ];

        let results = model.run_multi(&batches).expect("run_multi failed");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0][0].data, vec![11.0, 22.0, 33.0, 44.0]);
        assert_eq!(results[1][0].data, vec![6.0, 6.0, 6.0, 6.0]);
    }

    #[test]
    fn run_multi_empty_batches() {
        let model_bytes = build_onnx_model("Relu", 1);
        let model = OnnxModel::load_from_bytes(&model_bytes).expect("Failed to load model");

        let results = model.run_multi(&[]).expect("run_multi on empty batches failed");
        assert!(results.is_empty());
    }

    #[test]
    fn shape_mismatch_errors() {
        let model_bytes = build_onnx_model("Relu", 1);
        let model = OnnxModel::load_from_bytes(&model_bytes).expect("Failed to load model");

        // Wrong number of elements for the declared shape.
        let bad_input = StdbTensor {
            shape: vec![1, 4],
            data: vec![1.0, 2.0], // only 2 elements for a 1x4 tensor
        };

        let result = model.run(&[bad_input]);
        assert!(result.is_err());
    }
}
