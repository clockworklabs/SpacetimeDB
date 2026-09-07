use spacetimedb_smoketests::Smoketest;

struct IntTy {
    ty: &'static str,
    name: &'static str,
}

const INT_TYPES: &[IntTy] = &[
    IntTy { ty: "u8", name: "u_8" },
    IntTy {
        ty: "u16",
        name: "u_16",
    },
    IntTy {
        ty: "u32",
        name: "u_32",
    },
    IntTy {
        ty: "u64",
        name: "u_64",
    },
    IntTy {
        ty: "u128",
        name: "u_128",
    },
    IntTy { ty: "i8", name: "i_8" },
    IntTy {
        ty: "i16",
        name: "i_16",
    },
    IntTy {
        ty: "i32",
        name: "i_32",
    },
    IntTy {
        ty: "i64",
        name: "i_64",
    },
    IntTy {
        ty: "i128",
        name: "i_128",
    },
];

#[test]
fn test_autoinc_basic() {
    let test = Smoketest::builder().precompiled_module("autoinc-basic").build();

    for int in INT_TYPES {
        test.call(&format!("add_{}", int.name), &[r#""Robert""#, "1"]).unwrap();
        test.call(&format!("add_{}", int.name), &[r#""Julie""#, "2"]).unwrap();
        test.call(&format!("add_{}", int.name), &[r#""Samantha""#, "3"])
            .unwrap();
        test.call(&format!("say_hello_{}", int.name), &[]).unwrap();

        let logs = test.logs(4).unwrap();
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
            "[{}] Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
            int.ty,
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
            "[{}] Expected 'Hello, 2:Julie!' in logs, got: {:?}",
            int.ty,
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
            "[{}] Expected 'Hello, 1:Robert!' in logs, got: {:?}",
            int.ty,
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, World!")),
            "[{}] Expected 'Hello, World!' in logs, got: {:?}",
            int.ty,
            logs
        );
    }
}

#[test]
fn test_autoinc_unique() {
    let test = Smoketest::builder().precompiled_module("autoinc-unique").build();

    for int in INT_TYPES {
        test.call(&format!("update_{}", int.name), &[r#""Robert""#, "2"])
            .unwrap();
        test.call(&format!("add_new_{}", int.name), &[r#""Success""#]).unwrap();

        let result = test.call(&format!("add_new_{}", int.name), &[r#""Failure""#]);
        assert!(
            result.is_err(),
            "[{}] Expected add_new to fail due to unique constraint violation",
            int.ty
        );

        test.call(&format!("say_hello_{}", int.name), &[]).unwrap();

        let logs = test.logs(4).unwrap();
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 2:Robert!")),
            "[{}] Expected 'Hello, 2:Robert!' in logs, got: {:?}",
            int.ty,
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, 1:Success!")),
            "[{}] Expected 'Hello, 1:Success!' in logs, got: {:?}",
            int.ty,
            logs
        );
        assert!(
            logs.iter().any(|msg| msg.contains("Hello, World!")),
            "[{}] Expected 'Hello, World!' in logs, got: {:?}",
            int.ty,
            logs
        );
    }
}

/// A rolled-back auto-inc insert must leave durable sequence metadata consistent with later
/// committed auto-inc values.
///
/// This is a regression test for a bug which we fixed in [PR 5880](https://github.com/clockworklabs/SpacetimeDB/pull/5880).
/// Prior to that PR, sequences kept a non-transactional and non-persistent in-memory side table
/// as an optimization rather than updating `st_sequence` rows on each sequence read.
/// A bug in the implementation of that optimization caused the in-memory state to remain updated
/// even when the persistent `st_sequence` change rolled back.
#[test]
fn autoinc_sequence_allocation_remains_consistent_after_rollback() {
    let test = Smoketest::builder().precompiled_module("autoinc-unique").build();

    assert!(
        test.call("add_and_fail_u_64", &[]).is_err(),
        "Reducer that intentionally fails after an auto-inc insert should fail"
    );
    test.call("add_new_u_64", &[r#""committed""#]).unwrap();

    let inserted_value = parse_single_u64(
        &test
            .sql("SELECT key_col FROM person_u_64 WHERE name = 'committed'")
            .unwrap(),
    );
    let persisted_allocation = parse_single_u64(
        &test
            .sql("SELECT allocated FROM st_sequence WHERE sequence_name = 'person_u_64_key_col_seq'")
            .unwrap(),
    );

    assert!(
        inserted_value <= persisted_allocation,
        "st_sequence allocation is inconsistent after a rolled-back auto-inc insert: \
         committed value {inserted_value}, persisted allocation {persisted_allocation}"
    );
}

fn parse_single_u64(output: &str) -> u64 {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.parse().ok())
        .unwrap_or_else(|| panic!("SQL query should return one unsigned integer, got:\n{output}"))
}
