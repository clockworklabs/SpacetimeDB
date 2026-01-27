//! Auto-increment tests translated from smoketests/tests/auto_inc.py

use spacetimedb_smoketests::Smoketest;

const INT_TYPES: &[&str] = &["u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128"];

#[test]
fn test_autoinc_basic() {
    let test = Smoketest::builder().precompiled_module("autoinc-basic").build();

    for int_ty in INT_TYPES {
        test.call(&format!("add_{int_ty}"), &[r#""Robert""#, "1"]).unwrap();
        test.call(&format!("add_{int_ty}"), &[r#""Julie""#, "2"]).unwrap();
        test.call(&format!("add_{int_ty}"), &[r#""Samantha""#, "3"]).unwrap();
        test.call(&format!("say_hello_{int_ty}"), &[]).unwrap();

        let logs = test.logs(4).unwrap();
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
            "[{int_ty}] Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
            "[{int_ty}] Expected 'Hello, 2:Julie!' in logs, got: {:?}",
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
            "[{int_ty}] Expected 'Hello, 1:Robert!' in logs, got: {:?}",
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, World!")),
            "[{int_ty}] Expected 'Hello, World!' in logs, got: {:?}",
            logs
        );
    }
}

#[test]
fn test_autoinc_unique() {
    let test = Smoketest::builder().precompiled_module("autoinc-unique").build();

    for int_ty in INT_TYPES {
        // Insert Robert with explicit id 2
        test.call(&format!("update_{int_ty}"), &[r#""Robert""#, "2"]).unwrap();

        // Auto-inc should assign id 1 to Success
        test.call(&format!("add_new_{int_ty}"), &[r#""Success""#]).unwrap();

        // Auto-inc tries to assign id 2, but Robert already has it - should fail
        let result = test.call(&format!("add_new_{int_ty}"), &[r#""Failure""#]);
        assert!(
            result.is_err(),
            "[{int_ty}] Expected add_new to fail due to unique constraint violation"
        );

        test.call(&format!("say_hello_{int_ty}"), &[]).unwrap();

        let logs = test.logs(4).unwrap();
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 2:Robert!")),
            "[{int_ty}] Expected 'Hello, 2:Robert!' in logs, got: {:?}",
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 1:Success!")),
            "[{int_ty}] Expected 'Hello, 1:Success!' in logs, got: {:?}",
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, World!")),
            "[{int_ty}] Expected 'Hello, World!' in logs, got: {:?}",
            logs
        );
    }
}
