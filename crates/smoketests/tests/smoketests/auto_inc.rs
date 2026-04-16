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
