use serde_json::Value;
use serial_test::serial;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};

// The tests MUST be run in sequence because they read the OS environment
// and can cause a race when run in parallel.

fn test_calling_a_reducer_in_module(module_name: &'static str) {
    CompiledModule::compile(module_name, CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json = r#"{"call": {"fn": "add", "args": ["Tyrion"]}}"#.to_string();
            module.send(json).await.unwrap();
            let json = r#"{"call": {"fn": "say_hello", "args": []}}"#.to_string();
            module.send(json).await.unwrap();

            let lines = module.read_log(Some(10)).await;
            let lines: Vec<Value> = lines.trim().split('\n').map(serde_json::from_str).collect::<serde_json::Result<_>>().unwrap();

            assert!(lines.len() >= 4);

            assert_eq!(lines[0], serde_json::json!({"level":"Info","filename":"spacetimedb","message":"Creating table `Person`"}));

            assert_eq!(lines[lines.len() - 2]["level"], "Info");
            assert_eq!(lines[lines.len() - 2]["message"], "Hello, Tyrion!");

            assert_eq!(lines[lines.len() - 1]["level"], "Info");
            assert_eq!(lines[lines.len() - 1]["message"], "Hello, World!");
        },
    );
}

#[test]
#[serial]
fn test_calling_a_reducer() {
    test_calling_a_reducer_in_module("spacetimedb-quickstart");
}

#[test]
#[serial]
fn test_calling_a_reducer_csharp() {
    test_calling_a_reducer_in_module("spacetimedb-quickstart-cs");
}

#[test]
#[serial]
fn test_calling_a_reducer_with_private_table() {
    CompiledModule::compile("rust-wasm-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let json = r#"{"call": {"fn": "add_private", "args": ["Tyrion"]}}"#.to_string();
            module.send(json).await.unwrap();
            let json = r#"{"call": {"fn": "query_private", "args": []}}"#.to_string();
            module.send(json).await.unwrap();

            let lines = module.read_log(Some(10)).await;
            let lines: Vec<&str> = lines.trim().split('\n').collect();

            assert_eq!(lines.len(), 8);

            let json: Value = serde_json::from_str(lines[6]).unwrap();
            assert_eq!(json["message"], Value::String("Private, Tyrion!".to_string()));
            let json: Value = serde_json::from_str(lines[7]).unwrap();
            assert_eq!(json["message"], Value::String("Private, World!".to_string()));
        },
    );
}
