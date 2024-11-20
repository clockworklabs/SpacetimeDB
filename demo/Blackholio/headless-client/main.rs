use spacetimedb_sdk::{credentials, DbContext, Event, Identity, ReducerEvent, Status, Table, TableWithPrimaryKey};

mod module_bindings;
use module_bindings::*;

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(_conn: &DbConnection, identity: Identity, token: &str) {
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_conn: &DbConnection, err: Option<&anyhow::Error>) {
    if let Some(err) = err {
        panic!("Disconnected abnormally: {err}")
    } else {
        println!("Disconnected normally.");
        std::process::exit(0)
    }
}

fn creds_store(name: &String) -> credentials::File {
    credentials::File::new(format!("circle-game-client-{}", name))
}

/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(conn: &DbConnection) {
//    conn.subscription_builder()
//        .on_applied(on_sub_applied)
//        .subscribe(["SELECT * FROM user;", "SELECT * FROM message;"]);
}

/// Register our row and reducer callbacks.
fn register_callbacks(ctx: &DbConnection) {
//    // When a new user joins, print a notification.
//    ctx.db.user().on_insert(on_user_inserted);
//
//    // When a user's status changes, print a notification.
//    ctx.db.user().on_update(on_user_updated);
//
//    // When a new message is received, print it.
//    ctx.db.message().on_insert(on_message_inserted);
//
//    // When we fail to set our name, print a warning.
//    ctx.reducers.on_set_name(on_name_set);
//
//    // When we fail to send a message, print a warning.
//    ctx.reducers.on_send_message(on_message_sent);
}

fn connect_to_db(name: &String) -> DbConnection {
    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(|err| panic!("Error while connecting: {err}"))
        .on_disconnect(on_disconnected)
        .with_credentials(creds_store(name).load().expect("Error loading credentials"))
        .with_module_name("untitled-circle-game")
        .with_uri("http://localhost:3000")
        .build()
        .expect("Failed to connect")
}

fn main() -> anyhow::Result<()> {
    // get the first command line arg
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <username>", args[0]);
        std::process::exit(1);
    }
    let name = &args[1];

    let conn = connect_to_db(name);
    register_callbacks(&conn);
    subscribe_to_tables(&conn);
    conn.run_threaded();

    conn.reducers.create_player(name.clone())?;
    loop {
        conn.reducers.update_player_input(
            Vector2 {
                x: rand::random::<f32>() - 0.5,
                y: rand::random::<f32>() - 0.5,
            },
            1.0
        )?;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    conn.disconnect().unwrap();
}