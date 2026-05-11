use spacetimedb::{Identity, ReducerContext, Table, Timestamp};

#[spacetimedb::table(accessor = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

#[spacetimedb::table(accessor = message, public)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}

fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() {
        Err("Names must not be empty".to_string())
    } else {
        Ok(name)
    }
}

#[spacetimedb::reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?;
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        log::info!("User {} sets name to {name}", ctx.sender());
        ctx.db.user().identity().update(User {
            name: Some(name),
            ..user
        });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}

fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() {
        Err("Messages must not be empty".to_string())
    } else {
        Ok(text)
    }
}

#[spacetimedb::reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    // Things to consider:
    // - Rate-limit messages per-user.
    // - Reject messages from unnamed user.
    let text = validate_message(text)?;
    log::info!("User {}: {text}", ctx.sender());
    ctx.db.message().insert(Message {
        sender: ctx.sender(),
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}

#[spacetimedb::reducer(init)]
// Called when the module is initially published
pub fn init(_ctx: &ReducerContext) {}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        // If this is a returning user, i.e. we already have a `User` with this `Identity`,
        // set `online: true`, but leave `name` and `identity` unchanged.
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        // If this is a new user, create a `User` row for the `Identity`,
        // which is online, but hasn't set a name.
        ctx.db.user().insert(User {
            name: None,
            identity: ctx.sender(),
            online: true,
        });
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        // This branch should be unreachable,
        // as it doesn't make sense for a client to disconnect without connecting first.
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name() {
        assert_eq!(validate_name("Alice".to_string()), Ok("Alice".to_string()));
        assert_eq!(
            validate_name("".to_string()),
            Err("Names must not be empty".to_string())
        );
    }

    /// Verify that all expected tables are registered in this module.
    ///
    /// `ALL_TABLE_NAMES` is a distributed slice populated at link time by the
    /// `#[table]` macro — no setup or initialization required.
    #[test]
    fn all_tables_are_registered() {
        let names = spacetimedb::test_utils::all_table_names();
        assert!(names.contains(&"user"), "expected table 'user', got: {names:?}");
        assert!(names.contains(&"message"), "expected table 'message', got: {names:?}");
        assert_eq!(names.len(), 2, "unexpected extra tables: {names:?}");
    }

    #[test]
    fn module_def_is_registered() {
        let spacetimedb::spacetimedb_lib::RawModuleDef::V10(module) = spacetimedb::test_utils::module_def() else {
            panic!("expected v10 module definition");
        };

        let tables = module.tables().expect("expected tables section");
        assert!(tables.iter().any(|table| table.source_name.as_ref() == "user"));
        assert!(tables.iter().any(|table| table.source_name.as_ref() == "message"));
        assert_eq!(tables.len(), 2, "unexpected extra tables: {tables:?}");

        let reducers = module.reducers().expect("expected reducers section");
        for expected in [
            "set_name",
            "send_message",
            "init",
            "identity_connected",
            "identity_disconnected",
        ] {
            assert!(
                reducers.iter().any(|reducer| reducer.source_name.as_ref() == expected),
                "expected reducer '{expected}', got: {reducers:?}",
            );
        }
    }

    #[test]
    fn test_context_can_insert_and_read_chat_rows() {
        let ctx = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let sender = Identity::ZERO;

        ctx.db.user().insert(User {
            identity: sender,
            name: Some("Alice".to_string()),
            online: true,
        });
        ctx.db.message().insert(Message {
            sender,
            sent: Timestamp::UNIX_EPOCH,
            text: "Hello, SpacetimeDB!".to_string(),
        });

        assert_eq!(ctx.db.user().count(), 1);
        assert_eq!(ctx.db.message().count(), 1);

        let users = ctx.db.user().iter().collect::<Vec<_>>();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].identity, sender);
        assert_eq!(users[0].name.as_deref(), Some("Alice"));
        assert!(users[0].online);

        let messages = ctx.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, sender);
        assert_eq!(messages[0].sent, Timestamp::UNIX_EPOCH);
        assert_eq!(messages[0].text, "Hello, SpacetimeDB!");
    }

    #[test]
    fn test_context_can_make_two_independent_dbs() {
        let ctx1 = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let ctx2 = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let sender = Identity::ZERO;

        ctx1.db.user().insert(User {
            identity: sender,
            name: Some("Alice".to_string()),
            online: true,
        });
        ctx2.db.message().insert(Message {
            sender,
            sent: Timestamp::UNIX_EPOCH,
            text: "Hello, SpacetimeDB!".to_string(),
        });

        assert_eq!(ctx1.db.user().count(), 1);
        assert_eq!(ctx2.db.user().count(), 0);
        assert_eq!(ctx1.db.message().count(), 0);
        assert_eq!(ctx2.db.message().count(), 1);

        let users = ctx1.db.user().iter().collect::<Vec<_>>();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].identity, sender);
        assert_eq!(users[0].name.as_deref(), Some("Alice"));
        assert!(users[0].online);

        let messages = ctx2.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, sender);
        assert_eq!(messages[0].sent, Timestamp::UNIX_EPOCH);
        assert_eq!(messages[0].text, "Hello, SpacetimeDB!");

        assert_eq!(ctx2.db.user().iter().count(), 0);
        assert_eq!(ctx1.db.message().iter().count(), 0);
    }

    #[test]
    fn test_context_can_call_reducer_with_clock_timestamp() {
        let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let timestamp = Timestamp::from_micros_since_unix_epoch(123);
        test.clock.set(timestamp);

        test.with_reducer_tx::<_, String>(spacetimedb::test_utils::TestAuth::internal(), |ctx| {
            send_message(ctx, "Hello from a reducer".to_string())
        })
        .expect("send_message should succeed");

        let messages = test.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, Identity::ZERO);
        assert_eq!(messages[0].sent, timestamp);
        assert_eq!(messages[0].text, "Hello from a reducer");
    }

    #[test]
    fn test_reducer_with_jwt() {
        // These are minimal claims to construct a valid JWT payload for tests.
        let payload = r#"{"iss":"issuer","sub":"subject","iat":0}"#;
        let expected_sender = spacetimedb::Identity::from_claims("issuer", "subject");
        let connection_id = spacetimedb::ConnectionId::from_u128(7);

        let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let timestamp = Timestamp::from_micros_since_unix_epoch(123);
        test.clock.set(timestamp);

        let auth = spacetimedb::test_utils::TestAuth::from_jwt_payload(payload, connection_id)
            .expect("JWT payload should be valid for tests");
        test.with_reducer_tx::<_, String>(auth, |ctx| send_message(ctx, "Hello from a reducer".to_string()))
            .expect("send_message should succeed");

        let messages = test.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, expected_sender);
        assert_eq!(messages[0].sent, timestamp);
        assert_eq!(messages[0].text, "Hello from a reducer");
    }

    #[test]
    fn test_procedure_context_try_with_tx_commits_chat_rows() {
        let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");
        let timestamp = Timestamp::from_micros_since_unix_epoch(456);
        test.clock.set(timestamp);

        let mut ctx = test.procedure_context(spacetimedb::test_utils::TestAuth::internal());
        ctx.try_with_tx::<_, String>(|tx| {
            identity_connected(tx);
            set_name(tx, "Alice".to_string())?;
            send_message(tx, "Hello from a procedure tx".to_string())?;
            Ok(())
        })
        .expect("procedure transaction should commit");

        let user = test
            .db
            .user()
            .identity()
            .find(Identity::ZERO)
            .expect("user should exist");
        assert_eq!(user.name.as_deref(), Some("Alice"));
        assert!(user.online);

        let messages = test.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, Identity::ZERO);
        assert_eq!(messages[0].sent, timestamp);
        assert_eq!(messages[0].text, "Hello from a procedure tx");
    }

    fn fetch_status_message(ctx: &mut spacetimedb::ProcedureContext) -> Result<String, String> {
        let response = ctx
            .http
            .get("https://example.invalid/status")
            .map_err(|err| err.to_string())?;

        if !response.status().is_success() {
            return Err(format!("status endpoint returned {}", response.status()));
        }

        response.into_body().into_string().map_err(|err| err.to_string())
    }

    #[test]
    fn test_procedure_context_can_mock_http() {
        let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

        test.set_http_responder(|_, request| {
            assert_eq!(request.method().as_str(), "GET");
            assert_eq!(request.uri().to_string(), "https://example.invalid/status");

            Ok(spacetimedb::http::Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(spacetimedb::http::Body::from("chat service is healthy"))
                .expect("test response should be valid"))
        });

        let mut ctx = test.procedure_context(spacetimedb::test_utils::TestAuth::internal());
        let message = fetch_status_message(&mut ctx).expect("mock HTTP call should succeed");

        assert_eq!(message, "chat service is healthy");
    }

    #[test]
    fn test_update() {
        let test = spacetimedb::test_utils::TestContext::new().expect("test context should initialize");

        let sender = Identity::ZERO;

        test.db.user().insert(User {
            identity: sender,
            name: Some("Alice".to_string()),
            online: true,
        });
        assert_eq!(test.db.user().iter().count(), 1);

        test.db.user().identity().update(User {
            identity: sender,
            name: Some("Alice2".to_string()),
            online: true,
        });
        assert_eq!(test.db.user().iter().count(), 1);
        let user = test.db.user().identity().find(sender).expect("user should exist");
        assert_eq!(user.identity, sender);
        assert_eq!(user.name.as_deref(), Some("Alice2"));
        assert!(user.online);
    }
}
