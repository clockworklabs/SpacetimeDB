use spacetimedb_smoketests::Smoketest;
use std::fs;

/// Minimal ONNX "Add" model (2 inputs → 1 output, shape [1,4], f32).
/// Generated from `tract_onnx::pb` protobuf types with opset 13.
const ADD_MODEL_ONNX: &[u8] = &[
    8, 7, 18, 16, 115, 112, 97, 99, 101, 116, 105, 109, 101, 100, 98, 45, 116, 101, 115, 116,
    58, 133, 1, 10, 39, 10, 7, 105, 110, 112, 117, 116, 95, 48, 10, 7, 105, 110, 112, 117, 116,
    95, 49, 18, 6, 111, 117, 116, 112, 117, 116, 26, 6, 110, 111, 100, 101, 95, 48, 34, 3, 65,
    100, 100, 18, 10, 116, 101, 115, 116, 95, 103, 114, 97, 112, 104, 90, 25, 10, 7, 105, 110,
    112, 117, 116, 95, 48, 18, 14, 10, 12, 8, 1, 18, 8, 10, 2, 8, 1, 10, 2, 8, 4, 90, 25, 10,
    7, 105, 110, 112, 117, 116, 95, 49, 18, 14, 10, 12, 8, 1, 18, 8, 10, 2, 8, 1, 10, 2, 8, 4,
    98, 24, 10, 6, 111, 117, 116, 112, 117, 116, 18, 14, 10, 12, 8, 1, 18, 8, 10, 2, 8, 1, 10,
    2, 8, 4, 66, 2, 16, 13,
];

const ONNX_MODULE: &str = r#"
use spacetimedb::{log, ReducerContext, onnx::Tensor};

#[spacetimedb::reducer]
pub fn run_add(ctx: &ReducerContext) {
    let a = vec![Tensor { shape: vec![1, 4], data: vec![1.0, 2.0, 3.0, 4.0] }];
    let b = vec![Tensor { shape: vec![1, 4], data: vec![10.0, 20.0, 30.0, 40.0] }];

    let inputs = vec![a[0].clone(), b[0].clone()];
    let output = ctx.onnx.run("test_add", &inputs).expect("run failed");
    log::info!("add_result: {:?}", output[0].data);
}

#[spacetimedb::reducer]
pub fn run_add_multi(ctx: &ReducerContext) {
    let batches = vec![
        vec![
            Tensor { shape: vec![1, 4], data: vec![1.0, 2.0, 3.0, 4.0] },
            Tensor { shape: vec![1, 4], data: vec![10.0, 20.0, 30.0, 40.0] },
        ],
        vec![
            Tensor { shape: vec![1, 4], data: vec![5.0, 5.0, 5.0, 5.0] },
            Tensor { shape: vec![1, 4], data: vec![1.0, 1.0, 1.0, 1.0] },
        ],
    ];
    let results = ctx.onnx.run_multi("test_add", &batches).expect("run_multi failed");
    log::info!("multi_result_0: {:?}", results[0][0].data);
    log::info!("multi_result_1: {:?}", results[1][0].data);
}
"#;

/// Place the test ONNX model in the server's models directory.
fn setup_model(test: &Smoketest) {
    let guard = test.guard.as_ref().expect("ONNX tests require a local server");
    let models_dir = guard.data_dir.join("models");
    fs::create_dir_all(&models_dir).expect("Failed to create models directory");
    fs::write(models_dir.join("test_add.onnx"), ADD_MODEL_ONNX).expect("Failed to write test model");
}

/// Test single ONNX inference from a WASM module reducer.
#[test]
fn test_onnx_run() {
    let test = Smoketest::builder()
        .module_code(ONNX_MODULE)
        .bindings_features(&["unstable", "onnx"])
        .build();

    setup_model(&test);

    test.call("run_add", &[]).unwrap();

    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("[11.0, 22.0, 33.0, 44.0]")),
        "Expected add result in logs, got: {logs:?}"
    );
}

/// Test batched ONNX inference (run_multi) from a WASM module reducer.
#[test]
fn test_onnx_run_multi() {
    let test = Smoketest::builder()
        .module_code(ONNX_MODULE)
        .bindings_features(&["unstable", "onnx"])
        .build();

    setup_model(&test);

    test.call("run_add_multi", &[]).unwrap();

    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("[11.0, 22.0, 33.0, 44.0]")),
        "Expected first batch result in logs, got: {logs:?}"
    );
    assert!(
        logs.iter().any(|l| l.contains("[6.0, 6.0, 6.0, 6.0]")),
        "Expected second batch result in logs, got: {logs:?}"
    );
}
