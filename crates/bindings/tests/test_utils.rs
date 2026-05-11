use spacetimedb::spacetimedb_lib::RawModuleDef;
use spacetimedb::test_utils::{TestAuth, TestQueryError};
use spacetimedb::{reducer, table, Query, ReducerContext, Table, Timestamp};

#[table(accessor = test_utils_user, public)]
#[derive(Debug, PartialEq, Eq)]
pub struct TestUtilsUser {
    #[primary_key]
    id: u64,
    name: String,
}

#[table(accessor = test_utils_event, public, event)]
#[derive(Debug, PartialEq, Eq)]
pub struct TestUtilsEvent {
    #[primary_key]
    id: u64,
    #[index(btree)]
    message: String,
}

#[reducer]
pub fn add_test_utils_user(_ctx: &ReducerContext, id: u64, name: String) {
    let _ = (id, name);
}

fn query_test_utils_users_by_name(from: spacetimedb::QueryBuilder, name: &str) -> impl Query<TestUtilsUser> {
    from.test_utils_user()
        .r#where(|user| user.name.eq(name.to_owned()))
        .build()
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
    assert!(tables
        .iter()
        .any(|table| table.source_name.as_ref() == "test_utils_event"));

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
    assert!(datastore.table_id("test_utils_event").is_ok());
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
fn test_context_run_query_returns_typed_rows() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.db.test_utils_user().insert(TestUtilsUser {
        id: 1,
        name: "Ada".to_owned(),
    });
    test.db.test_utils_user().insert(TestUtilsUser {
        id: 2,
        name: "Grace".to_owned(),
    });

    let rows: Vec<TestUtilsUser> = test
        .run_query(spacetimedb::QueryBuilder {}.test_utils_user().build())
        .expect("query should execute");

    assert_eq!(
        rows,
        vec![
            TestUtilsUser {
                id: 1,
                name: "Ada".to_owned(),
            },
            TestUtilsUser {
                id: 2,
                name: "Grace".to_owned(),
            },
        ]
    );
}

#[test]
fn test_context_run_query_supports_query_returning_view_pattern() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.db.test_utils_user().insert(TestUtilsUser {
        id: 1,
        name: "Ada".to_owned(),
    });
    test.db.test_utils_user().insert(TestUtilsUser {
        id: 2,
        name: "Grace".to_owned(),
    });

    let query = query_test_utils_users_by_name(spacetimedb::QueryBuilder {}, "Ada");
    let rows: Vec<TestUtilsUser> = test.run_query(query).expect("query should execute");

    assert_eq!(
        rows,
        vec![TestUtilsUser {
            id: 1,
            name: "Ada".to_owned(),
        }]
    );
}

#[test]
fn test_context_run_query_decode_mismatch_returns_error() {
    #[derive(Debug, spacetimedb::SpacetimeType)]
    struct WrongRow {
        id: u64,
        name: u64,
    }

    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.db.test_utils_user().insert(TestUtilsUser {
        id: 1,
        name: "Ada".to_owned(),
    });

    let query = spacetimedb::RawQuery::<WrongRow>::new(r#"SELECT * FROM "test_utils_user""#.to_owned());
    let err = test.run_query(query).unwrap_err();

    assert!(matches!(err, TestQueryError::Decode(_)));
}

#[test]
fn test_context_run_query_sees_reducer_committed_state() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_user().insert(TestUtilsUser {
            id: 3,
            name: "Reducer committed".to_owned(),
        });
        Ok(())
    })
    .expect("transaction should commit");

    let rows: Vec<TestUtilsUser> = test
        .run_query(spacetimedb::QueryBuilder {}.test_utils_user().build())
        .expect("query should execute");

    assert_eq!(
        rows,
        vec![TestUtilsUser {
            id: 3,
            name: "Reducer committed".to_owned(),
        }]
    );
}

#[test]
fn with_reducer_tx_uses_test_clock_and_internal_auth() {
    let mut test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.identity = spacetimedb::Identity::from_claims("module-issuer", "module-subject");
    let timestamp = Timestamp::from_micros_since_unix_epoch(42);
    test.clock.set(timestamp);

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        assert_eq!(ctx.timestamp, timestamp);
        assert_eq!(ctx.identity(), test.identity);
        assert_eq!(ctx.sender(), test.identity);
        assert_eq!(ctx.connection_id(), None);
        assert!(ctx.sender_auth().is_internal());
        Ok(())
    })
    .expect("transaction should commit");
}

#[test]
fn with_reducer_tx_derives_sender_from_jwt_auth() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let payload = r#"{"iss":"issuer","sub":"subject"}"#;
    let expected_sender = spacetimedb::Identity::from_claims("issuer", "subject");
    let connection_id = spacetimedb::ConnectionId::from_u128(7);

    test.with_reducer_tx::<_, ()>(
        TestAuth::from_jwt_payload(payload, connection_id).expect("JWT payload should be valid for tests"),
        |ctx| {
            assert_eq!(ctx.sender(), expected_sender);
            assert_eq!(ctx.identity(), test.identity);
            assert_eq!(ctx.connection_id(), Some(connection_id));
            assert!(!ctx.sender_auth().is_internal());
            assert_eq!(ctx.sender_auth().jwt().unwrap().identity(), expected_sender);
            Ok(())
        },
    )
    .expect("transaction should commit");
}

#[test]
fn test_auth_rejects_invalid_jwt_payload() {
    let connection_id = spacetimedb::ConnectionId::from_u128(7);

    assert!(TestAuth::from_jwt_payload(r#"{"iss":"issuer","sub":"subject"}"#, connection_id).is_ok());
    assert!(TestAuth::from_jwt_payload(r#"{"sub":"subject"}"#, connection_id).is_err());
    assert!(TestAuth::from_jwt_payload(r#"{"iss":"","sub":"subject","iat":0}"#, connection_id).is_err());

    let mismatched_identity = spacetimedb::Identity::ONE;
    let payload = format!(r#"{{"hex_identity":"{mismatched_identity}","iss":"issuer","sub":"subject","iat":0}}"#);
    assert!(TestAuth::from_jwt_payload(payload, connection_id).is_err());
}

#[cfg(feature = "rand08")]
#[test]
fn with_reducer_tx_clones_test_rng_seed() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.rng.set_seed(123);

    let first = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");
    test.clock.set(Timestamp::from_micros_since_unix_epoch(999));
    let second = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");

    assert_eq!(first, second);

    test.rng.set_seed(456);
    let third = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");

    assert_ne!(first, third);
}

#[cfg(feature = "rand08")]
#[test]
fn with_reducer_tx_rng_defaults_to_timestamp_seed() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.clock.set(Timestamp::from_micros_since_unix_epoch(123));
    let first = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");
    let second = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");

    assert_eq!(first, second);

    test.clock.set(Timestamp::from_micros_since_unix_epoch(456));
    let third = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");

    assert_ne!(first, third);

    test.rng.set_seed(789);
    let seeded = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");
    test.rng.clear_seed();
    test.clock.set(Timestamp::from_micros_since_unix_epoch(789));
    let timestamp_seeded = test
        .with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
            Ok((ctx.random::<u64>(), ctx.random::<u64>()))
        })
        .expect("transaction should commit");

    assert_eq!(seeded, timestamp_seeded);
}

#[test]
fn with_reducer_tx_uses_test_backed_db() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_user().insert(TestUtilsUser {
            id: 10,
            name: "Grace".to_owned(),
        });
        Ok(())
    })
    .expect("transaction should commit");

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
fn with_reducer_tx_event_table_rows_are_transaction_scoped() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_event().insert(TestUtilsEvent {
            id: 1,
            message: "event".to_owned(),
        });

        assert_eq!(ctx.db.test_utils_event().count(), 1);
        assert_eq!(
            ctx.db.test_utils_event().iter().collect::<Vec<_>>(),
            vec![TestUtilsEvent {
                id: 1,
                message: "event".to_owned(),
            }]
        );
        assert_eq!(
            ctx.db.test_utils_event().id().find(1),
            Some(TestUtilsEvent {
                id: 1,
                message: "event".to_owned(),
            })
        );
        assert_eq!(
            ctx.db.test_utils_event().message().filter("event").collect::<Vec<_>>(),
            vec![TestUtilsEvent {
                id: 1,
                message: "event".to_owned(),
            }]
        );
        Ok(())
    })
    .expect("transaction should commit");

    let table_id = test.datastore().table_id("test_utils_event").unwrap();
    assert_eq!(test.datastore().table_row_count(table_id).unwrap(), 0);
    assert!(test.datastore().table_rows(table_id).unwrap().is_empty());
}

#[test]
fn with_reducer_tx_event_table_rows_are_isolated_between_transactions() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_event().insert(TestUtilsEvent {
            id: 1,
            message: "first".to_owned(),
        });
        assert_eq!(
            ctx.db.test_utils_event().iter().collect::<Vec<_>>(),
            vec![TestUtilsEvent {
                id: 1,
                message: "first".to_owned(),
            }]
        );
        Ok(())
    })
    .expect("first transaction should commit");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        assert_eq!(ctx.db.test_utils_event().count(), 0);
        ctx.db.test_utils_event().insert(TestUtilsEvent {
            id: 2,
            message: "second".to_owned(),
        });
        assert_eq!(
            ctx.db.test_utils_event().iter().collect::<Vec<_>>(),
            vec![TestUtilsEvent {
                id: 2,
                message: "second".to_owned(),
            }]
        );
        Ok(())
    })
    .expect("second transaction should commit");
}

#[test]
fn with_reducer_tx_event_table_constraints_are_transaction_scoped() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_event().insert(TestUtilsEvent {
            id: 1,
            message: "first".to_owned(),
        });
        assert!(ctx
            .db
            .test_utils_event()
            .try_insert(TestUtilsEvent {
                id: 1,
                message: "duplicate".to_owned(),
            })
            .is_err());
        Ok(())
    })
    .expect("first transaction should commit");

    test.with_reducer_tx::<_, ()>(TestAuth::internal(), |ctx| {
        ctx.db.test_utils_event().insert(TestUtilsEvent {
            id: 1,
            message: "second".to_owned(),
        });
        assert_eq!(
            ctx.db.test_utils_event().id().find(1),
            Some(TestUtilsEvent {
                id: 1,
                message: "second".to_owned(),
            })
        );
        Ok(())
    })
    .expect("second transaction should commit");
}

#[test]
fn event_table_access_outside_reducer_transaction_panics() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    let insert = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        test.db.test_utils_event().insert(TestUtilsEvent {
            id: 1,
            message: "outside".to_owned(),
        });
    }));
    assert!(insert.is_err());

    let count = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        test.db.test_utils_event().count();
    }));
    assert!(count.is_err());

    let iter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = test.db.test_utils_event().iter().collect::<Vec<_>>();
    }));
    assert!(iter.is_err());
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
fn procedure_context_after_tx_commit_hook_can_interleave_reducer() {
    fn procedure_with_two_transactions(ctx: &mut spacetimedb::ProcedureContext) -> Result<(), ()> {
        ctx.try_with_tx::<_, ()>(|tx| {
            tx.db.test_utils_user().insert(TestUtilsUser {
                id: 25,
                name: "First procedure tx".to_owned(),
            });
            Ok(())
        })?;

        ctx.try_with_tx::<_, ()>(|tx| {
            assert_eq!(tx.db.test_utils_user().count(), 2);
            tx.db.test_utils_user().id().update(TestUtilsUser {
                id: 25,
                name: "Second procedure tx".to_owned(),
            });
            Ok(())
        })?;

        Ok(())
    }

    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let ran = std::rc::Rc::new(std::cell::Cell::new(false));
    let hooks = spacetimedb::test_utils::ProcedureTestHooks::new().after_tx_commit({
        let ran = ran.clone();
        move |hook_ctx| {
            if ran.replace(true) {
                return Ok(());
            }

            hook_ctx
                .with_reducer_tx(TestAuth::internal(), |ctx| {
                    ctx.db.test_utils_user().insert(TestUtilsUser {
                        id: 26,
                        name: "Interleaved reducer".to_owned(),
                    });
                    Ok::<_, ()>(())
                })
                .expect("interleaved reducer should commit");
            Ok(())
        }
    });
    let mut procedure = test
        .procedure_context_builder(TestAuth::internal())
        .hooks(hooks)
        .build();

    procedure_with_two_transactions(&mut procedure).expect("procedure should succeed");

    let mut rows = test.db.test_utils_user().iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id);
    assert_eq!(
        rows,
        vec![
            TestUtilsUser {
                id: 25,
                name: "Second procedure tx".to_owned(),
            },
            TestUtilsUser {
                id: 26,
                name: "Interleaved reducer".to_owned(),
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
        move |_test, request| {
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

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_http_responder_can_interleave_reducer() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

    test.set_http_responder(|test, _request| {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.db.test_utils_user().insert(TestUtilsUser {
                id: 27,
                name: "HTTP interleaved reducer".to_owned(),
            });
            Ok::<_, ()>(())
        })
        .expect("interleaved reducer should commit");

        Ok(spacetimedb::http::Response::builder()
            .status(200)
            .body(spacetimedb::http::Body::empty())
            .expect("test response should be valid"))
    });

    let procedure = test.procedure_context(TestAuth::internal());
    procedure
        .http
        .get("https://example.invalid/interleave")
        .expect("test HTTP responder should return a response");

    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 27,
            name: "HTTP interleaved reducer".to_owned(),
        }]
    );
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_sleep_advances_test_clock() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let start = Timestamp::from_micros_since_unix_epoch(100);
    let wake_time = Timestamp::from_micros_since_unix_epoch(500);
    test.clock.set(start);
    let mut procedure = test.procedure_context(TestAuth::internal());

    procedure.sleep_until(wake_time);

    assert_eq!(procedure.timestamp, wake_time);
    assert_eq!(test.clock.now(), wake_time);
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_on_sleep_hook_can_interleave_reducer() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let wake_time = Timestamp::from_micros_since_unix_epoch(500);
    let seen_wake_time = std::rc::Rc::new(std::cell::Cell::new(None));
    let hooks = spacetimedb::test_utils::ProcedureTestHooks::new().on_sleep({
        let seen_wake_time = seen_wake_time.clone();
        move |test, wake_time| {
            seen_wake_time.set(Some(wake_time));
            test.with_reducer_tx(TestAuth::internal(), |ctx| {
                ctx.db.test_utils_user().insert(TestUtilsUser {
                    id: 28,
                    name: "Sleep interleaved reducer".to_owned(),
                });
                Ok::<_, ()>(())
            })
            .expect("interleaved reducer should commit");
            Ok(())
        }
    });
    let mut procedure = test
        .procedure_context_builder(TestAuth::internal())
        .hooks(hooks)
        .build();

    procedure.sleep_until(wake_time);

    assert_eq!(seen_wake_time.get(), Some(wake_time));
    assert_eq!(procedure.timestamp, wake_time);
    assert_eq!(
        test.db.test_utils_user().iter().collect::<Vec<_>>(),
        vec![TestUtilsUser {
            id: 28,
            name: "Sleep interleaved reducer".to_owned(),
        }]
    );
}

#[cfg(feature = "unstable")]
#[test]
fn procedure_context_sleep_does_not_move_clock_back_after_hook() {
    let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
    let wake_time = Timestamp::from_micros_since_unix_epoch(500);
    let later_time = Timestamp::from_micros_since_unix_epoch(900);
    let hooks = spacetimedb::test_utils::ProcedureTestHooks::new().on_sleep(move |test, _wake_time| {
        test.clock.set(later_time);
        Ok(())
    });
    let mut procedure = test
        .procedure_context_builder(TestAuth::internal())
        .hooks(hooks)
        .build();

    procedure.sleep_until(wake_time);

    assert_eq!(procedure.timestamp, later_time);
    assert_eq!(test.clock.now(), later_time);
}
