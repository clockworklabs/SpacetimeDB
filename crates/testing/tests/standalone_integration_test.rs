use serde_json::Value;
use spacetimedb_testing::modules::{compile, with_module_async};

#[test]
fn test_calling_a_reducer() {
    compile("spacetimedb-quickstart");
    with_module_async("spacetimedb-quickstart", |module| async move {
        let json = r#"{"call": {"fn": "add", "args": ["Tyrion"]}}"#.to_string();
        module.send(json).await.unwrap();
        let json = r#"{"call": {"fn": "say_hello", "args": []}}"#.to_string();
        module.send(json).await.unwrap();

        let json = r#"{"call": {"fn": "add_private", "args": ["Tyrion"]}}"#.to_string();
        module.send(json).await.unwrap();
        let json = r#"{"call": {"fn": "query_private", "args": []}}"#.to_string();
        module.send(json).await.unwrap();

        let lines = module.read_log(Some(10)).await;
        let lines: Vec<&str> = lines.trim().split('\n').collect();

        assert_eq!(lines.len(), 4);

        let json: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(json["message"], Value::String("Hello, Tyrion!".to_string()));
        let json: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(json["message"], Value::String("Hello, World!".to_string()));

        let json: Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(json["message"], Value::String("Private, Tyrion!".to_string()));
        let json: Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(json["message"], Value::String("Private, World!".to_string()));
    });
}
