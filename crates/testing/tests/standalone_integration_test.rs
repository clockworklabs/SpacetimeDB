use serde_json::Value;
use serial_test::serial;
use spacetimedb_testing::modules::CompiledModule;

// The tests MUST be run in sequence because they read the OS environment
// and can cause a race when run in parallel.

fn test_calling_a_reducer_in_module(module_name: &'static str) {
    CompiledModule::compile(module_name).with_module_async(|module| async move {
        let json = r#"{"call": {"fn": "add", "args": ["Tyrion"]}}"#.to_string();
        module.send(json).await.unwrap();
        let json = r#"{"call": {"fn": "say_hello", "args": []}}"#.to_string();
        module.send(json).await.unwrap();

        let lines = module.read_log(Some(10)).await;
        let lines: Vec<&str> = lines.trim().split('\n').collect();

        assert_eq!(lines.len(), 2);

        let json: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(json["message"], Value::String("Hello, Tyrion!".to_string()));
        let json: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(json["message"], Value::String("Hello, World!".to_string()));
    });
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
    CompiledModule::compile("rust-wasm-test").with_module_async(|module| async move {
        let json = r#"{"call": {"fn": "add_private", "args": ["Tyrion"]}}"#.to_string();
        module.send(json).await.unwrap();
        let json = r#"{"call": {"fn": "query_private", "args": []}}"#.to_string();
        module.send(json).await.unwrap();

        let lines = module.read_log(Some(10)).await;
        let lines: Vec<&str> = lines.trim().split('\n').collect();

        assert_eq!(lines.len(), 2);

        let json: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(json["message"], Value::String("Private, Tyrion!".to_string()));
        let json: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(json["message"], Value::String("Private, World!".to_string()));
    });
}
