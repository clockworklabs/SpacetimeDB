use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}

// Unit tests run the module against an in-memory database, with no server.
// See https://spacetimedb.com/docs/how-to/unit-testing
#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb::test_utils::{TestAuth, TestContext};
    use spacetimedb::ConnectionId;

    fn client_auth() -> TestAuth {
        TestAuth::from_jwt_payload(r#"{"iss":"test-issuer","sub":"alice"}"#, ConnectionId::from_u128(1))
            .expect("valid test claims")
    }

    #[test]
    fn adds_a_person() {
        let test = TestContext::new().expect("test database");

        test.with_reducer_tx(client_auth(), |ctx| {
            add(ctx, "Alice".to_string());
            Ok::<(), String>(())
        })
        .expect("reducer succeeds");

        let names: Vec<String> = test.db.person().iter().map(|person| person.name).collect();
        assert_eq!(names, ["Alice"]);
    }

    #[test]
    fn rolls_back_all_writes_on_error() {
        let test = TestContext::new().expect("test database");

        let result = test.with_reducer_tx(client_auth(), |ctx| {
            add(ctx, "Alice".to_string());
            Err::<(), String>("Something went wrong".to_string())
        });

        assert_eq!(result, Err("Something went wrong".to_string()));
        assert_eq!(test.db.person().count(), 0);
    }
}
