use spacetimedb::spacetimedb_lib::RawModuleDef;
use spacetimedb::test_utils::TestAuth;
use spacetimedb::{reducer, table, ReducerContext, Table, Timestamp};

#[table(accessor = test_utils_user, public)]
#[derive(Debug, PartialEq, Eq)]
pub struct TestUtilsUser {
    #[primary_key]
    id: u64,
    name: String,
}

#[reducer]
pub fn add_test_utils_user(_ctx: &ReducerContext, id: u64, name: String) {
    let _ = (id, name);
}

// You can run these with `cargo test -p spacetimedb --features test-utils --test test_utils`

#[test]
fn module_def_includes_native_test_registrations() {
    let mut table_names = spacetimedb::test_utils::all_table_names();
    table_names.sort_unstable();
    assert!(table_names.contains(&"test_utils_user"));

    let RawModuleDef::V10(module) = spacetimedb::test_utils::module_def() else {
        panic!("test utils should return a v10 raw module def");
    };

    let tables = module.tables().expect("tables section should be present");
    assert!(tables
        .iter()
        .any(|table| table.source_name.as_ref() == "test_utils_user"));

    let reducers = module.reducers().expect("reducers section should be present");
    assert!(reducers
        .iter()
        .any(|reducer| reducer.source_name.as_ref() == "add_test_utils_user"));
}

#[test]
fn test_datastore_initializes_from_native_test_registrations() {
    let datastore = spacetimedb::test_utils::TestDatastore::from_module_def(spacetimedb::test_utils::module_def())
        .expect("test datastore should initialize");

    assert!(datastore.table_id("test_utils_user").is_ok());
}

#[test]
fn test_context_supports_basic_table_insert_and_iter() {
    let ctx = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let table = ctx.db.test_utils_user();

    let row = TestUtilsUser {
        id: 1,
        name: "Ada".to_owned(),
    };

    assert_eq!(table.count(), 0);
    assert_eq!(
        table.insert(row),
        TestUtilsUser {
            id: 1,
            name: "Ada".to_owned(),
        }
    );
    assert_eq!(table.count(), 1);
    assert_eq!(
        table.iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 1,
            name: "Ada".to_owned(),
        }]
    );
}

#[test]
fn reducer_context_uses_test_clock_and_internal_auth() {
    let mut test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.identity = spacetimedb::Identity::from_claims("module-issuer", "module-subject");
    let timestamp = Timestamp::from_micros_since_unix_epoch(42);
    test.clock.set(timestamp);

    let ctx = test.reducer_context(TestAuth::internal());

    assert_eq!(ctx.timestamp, timestamp);
    assert_eq!(ctx.identity(), test.identity);
    assert_eq!(ctx.sender(), test.identity);
    assert_eq!(ctx.connection_id(), None);
    assert!(ctx.sender_auth().is_internal());
}

#[test]
fn reducer_context_derives_sender_from_jwt_auth() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let payload = r#"{"iss":"issuer","sub":"subject","iat":0}"#;
    let expected_sender = spacetimedb::Identity::from_claims("issuer", "subject");
    let connection_id = spacetimedb::ConnectionId::from_u128(7);

    let ctx = test.reducer_context(
        TestAuth::from_jwt_payload(payload, connection_id).expect("JWT payload should be valid for tests"),
    );

    assert_eq!(ctx.sender(), expected_sender);
    assert_eq!(ctx.identity(), test.identity);
    assert_eq!(ctx.connection_id(), Some(connection_id));
    assert!(!ctx.sender_auth().is_internal());
    assert_eq!(ctx.sender_auth().jwt().unwrap().identity(), expected_sender);
}

#[test]
fn test_auth_rejects_invalid_jwt_payload() {
    let connection_id = spacetimedb::ConnectionId::from_u128(7);

    assert!(TestAuth::from_jwt_payload(r#"{"iss":"issuer","sub":"subject"}"#, connection_id).is_err());
    assert!(TestAuth::from_jwt_payload(r#"{"iss":"","sub":"subject","iat":0}"#, connection_id).is_err());

    let mismatched_identity = spacetimedb::Identity::ONE;
    let payload = format!(r#"{{"hex_identity":"{mismatched_identity}","iss":"issuer","sub":"subject","iat":0}}"#);
    assert!(TestAuth::from_jwt_payload(payload, connection_id).is_err());
}

#[cfg(feature = "rand08")]
#[test]
fn reducer_context_clones_test_rng_seed() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.rng.set_seed(123);

    let first = test.reducer_context(TestAuth::internal());
    test.clock.set(Timestamp::from_micros_since_unix_epoch(999));
    let second = test.reducer_context(TestAuth::internal());

    assert_eq!(first.random::<u64>(), second.random::<u64>());
    assert_eq!(first.random::<u64>(), second.random::<u64>());

    test.rng.set_seed(456);
    let third = test.reducer_context(TestAuth::internal());

    assert_ne!(first.random::<u64>(), third.random::<u64>());
}

#[cfg(feature = "rand08")]
#[test]
fn reducer_context_rng_defaults_to_timestamp_seed() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.clock.set(Timestamp::from_micros_since_unix_epoch(123));
    let first = test.reducer_context(TestAuth::internal());
    let second = test.reducer_context(TestAuth::internal());

    assert_eq!(first.random::<u64>(), second.random::<u64>());

    test.clock.set(Timestamp::from_micros_since_unix_epoch(456));
    let third = test.reducer_context(TestAuth::internal());

    assert_ne!(first.random::<u64>(), third.random::<u64>());

    test.rng.set_seed(789);
    let seeded = test.reducer_context(TestAuth::internal());
    test.rng.clear_seed();
    test.clock.set(Timestamp::from_micros_since_unix_epoch(789));
    let timestamp_seeded = test.reducer_context(TestAuth::internal());

    assert_eq!(seeded.random::<u64>(), timestamp_seeded.random::<u64>());
}

#[test]
fn reducer_context_uses_test_backed_db() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let ctx = test.reducer_context(TestAuth::internal());

    ctx.db.test_utils_user().insert(TestUtilsUser {
        id: 10,
        name: "Grace".to_owned(),
    });

    assert_eq!(test.db.test_utils_user().count(), 1);
    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 10,
            name: "Grace".to_owned(),
        }]
    );
}

#[test]
fn with_reducer_tx_commits_on_ok() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_user().insert(TestUtilsUser {
            id: 12,
            name: "Committed".to_owned(),
        });
        assert_eq!(ctx.db.test_utils_user().count(), 1);
        Ok(())
    })
    .expect("transaction should commit");

    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 12,
            name: "Committed".to_owned(),
        }]
    );
}

#[test]
fn with_reducer_tx_rolls_back_on_err() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    let res: Result<(), &'static str> = test.with_reducer_tx(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_user().insert(TestUtilsUser {
            id: 13,
            name: "Rolled back".to_owned(),
        });
        assert_eq!(ctx.db.test_utils_user().count(), 1);
        Err("rollback")
    });

    assert_eq!(res, Err("rollback"));
    assert_eq!(test.db.test_utils_user().count(), 0);
}

#[test]
fn with_reducer_tx_rolls_back_on_panic() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), ()> = test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.db.test_utils_user().insert(TestUtilsUser {
                id: 14,
                name: "Panicked".to_owned(),
            });
            assert_eq!(ctx.db.test_utils_user().count(), 1);
            panic!("force rollback");
        });
    }));

    assert!(panic.is_err());
    assert_eq!(test.db.test_utils_user().count(), 0);
}

#[test]
fn test_context_supports_unique_index_find_update_and_delete() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.db.test_utils_user().insert(TestUtilsUser {
        id: 11,
        name: "Alice".to_owned(),
    });

    assert_eq!(
        test.db.test_utils_user().id().find(11),
        Some(TestUtilsUser {
            id: 11,
            name: "Alice".to_owned(),
        })
    );

    assert_eq!(
        test.db.test_utils_user().id().update(TestUtilsUser {
            id: 11,
            name: "Alice2".to_owned(),
        }),
        TestUtilsUser {
            id: 11,
            name: "Alice2".to_owned(),
        }
    );

    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 11,
            name: "Alice2".to_owned(),
        }]
    );

    assert!(test.db.test_utils_user().id().delete(11));
    assert_eq!(test.db.test_utils_user().count(), 0);
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_try_with_tx_commits() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let mut procedure = test.procedure_context(TestAuth::internal());

    procedure
        .try_with_tx::<_, ()>(|tx| {
            tx.db.test_utils_user().insert(TestUtilsUser {
                id: 21,
                name: "Committed".to_owned(),
            });
            assert_eq!(tx.db.test_utils_user().count(), 1);
            Ok(())
        })
        .expect("transaction should commit");

    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 21,
            name: "Committed".to_owned(),
        }]
    );
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_try_with_tx_rolls_back_on_err() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let mut procedure = test.procedure_context(TestAuth::internal());

    let res: Result<(), &'static str> = procedure.try_with_tx(|tx| {
        tx.db.test_utils_user().insert(TestUtilsUser {
            id: 22,
            name: "Rolled back".to_owned(),
        });
        assert_eq!(tx.db.test_utils_user().count(), 1);
        Err("rollback")
    });

    assert_eq!(res, Err("rollback"));
    assert_eq!(test.db.test_utils_user().count(), 0);
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_transactions_can_be_interleaved() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let mut procedure = test.procedure_context(TestAuth::internal());

    procedure
        .try_with_tx::<_, ()>(|tx| {
            tx.db.test_utils_user().insert(TestUtilsUser {
                id: 23,
                name: "First tx".to_owned(),
            });
            Ok(())
        })
        .expect("first transaction should commit");

    test.db.test_utils_user().insert(TestUtilsUser {
        id: 24,
        name: "Between txs".to_owned(),
    });

    procedure
        .try_with_tx::<_, ()>(|tx| {
            assert_eq!(tx.db.test_utils_user().count(), 2);
            tx.db.test_utils_user().id().update(TestUtilsUser {
                id: 23,
                name: "Second tx".to_owned(),
            });
            Ok(())
        })
        .expect("second transaction should commit");

    let mut rows = test.db.test_utils_user().iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id);
    assert_eq!(
        rows,
        vec![
            TestUtilsUser {
                id: 23,
                name: "Second tx".to_owned(),
            },
            TestUtilsUser {
                id: 24,
                name: "Between txs".to_owned(),
            },
        ]
    );
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_uses_test_http_responder() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let seen_request = std::rc::Rc::new(std::cell::RefCell::new(None));

    test.set_http_responder({
        let seen_request = seen_request.clone();
        move |request| {
            seen_request.replace(Some((request.method().as_str().to_owned(), request.uri().to_string())));
            Ok(spacetimedb::http::Response::builder()
                .status(201)
                .header("x-test", "yes")
                .body(spacetimedb::http::Body::from("created"))
                .expect("test response should be valid"))
        }
    });

    let procedure = test.procedure_context(TestAuth::internal());
    let response = procedure
        .http
        .get("https://example.invalid/create")
        .expect("test HTTP responder should return a response");

    assert_eq!(response.status(), 201);
    assert_eq!(response.headers().get("x-test").unwrap(), "yes");
    assert_eq!(response.into_body().into_string().unwrap(), "created");
    assert_eq!(
        seen_request.borrow().as_ref(),
        Some(&("GET".to_owned(), "https://example.invalid/create".to_owned()))
    );
}
