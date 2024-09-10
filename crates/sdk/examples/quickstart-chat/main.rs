mod module_bindings;
use module_bindings::*;

use spacetimedb_sdk::{DbContext, Event, Identity, ReducerEvent, Status};

// # Our main function

fn main() {
    let ctx = connect_to_db();
    println!("connect_to_db returned");
    register_callbacks(&ctx);
    println!("register_callbacks returned");
    subscribe_to_tables(&ctx);
    println!("subscribe_to_tables returned");
    ctx.run_threaded();
    println!("run_threaded returned");
    user_input_loop(&ctx);
    println!("user_input_loop returned");
    ctx.disconnect().unwrap();
    println!("disconnect returned");
}

// # Register callbacks

/// Register our row and reducer callbacks.
fn register_callbacks(ctx: &DbConnection) {
    // When a new user joins, print a notification.
    ctx.db.user().on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    ctx.db.user().on_update(on_user_updated);

    // When a new message is received, print it.
    ctx.db.message().on_insert(on_message_inserted);

    // When we fail to set our name, print a warning.
    ctx.reducers.on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    ctx.reducers.on_send_message(on_message_sent);
}

// ## Save credentials to a file

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(_identity: Identity, _token: &str) {
    // todo!("Save credentials")
    // if let Err(e) = save_credentials(CREDS_DIR, creds) {
    //     eprintln!("Failed to save credentials: {:?}", e);
    // }
}

#[allow(unused)]
const CREDS_DIR: &str = ".spacetime_chat";

// ## Notify about new users

/// Our `User::on_insert` callback: if the user is online, print a notification.
fn on_user_inserted(_ctx: &EventContext, user: &User) {
    if user.online {
        println!("User {} connected.", user_name_or_identity(user));
    }
}

fn user_name_or_identity(user: &User) -> String {
    user.name
        .clone()
        .unwrap_or_else(|| user.identity.to_abbreviated_hex().to_string())
}

// ## Notify about updated users

/// Our `User::on_update` callback:
/// print a notification about name and status changes.
fn on_user_updated(_ctx: &EventContext, old: &User, new: &User) {
    if old.name != new.name {
        println!(
            "User {} renamed to {}.",
            user_name_or_identity(old),
            user_name_or_identity(new)
        );
    }
    if old.online && !new.online {
        println!("User {} disconnected.", user_name_or_identity(new));
    }
    if !old.online && new.online {
        println!("User {} connected.", user_name_or_identity(new));
    }
}

// ## Display incoming messages

/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(ctx: &EventContext, message: &Message) {
    if !matches!(ctx.event, Event::SubscribeApplied) {
        print_message(ctx, message);
    }
}

fn print_message(ctx: &EventContext, message: &Message) {
    let sender = ctx
        .db
        .user()
        .identity()
        .find(message.sender)
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    println!("{}: {}", sender, message.text);
}

// ## Print message backlog

/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
#[allow(unused)]
fn on_sub_applied(ctx: &EventContext) {
    let mut messages = ctx.db.message().iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(ctx, &message);
    }
}

// ## Warn if set_name failed

/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(ctx: &EventContext, name: &String) {
    if let Event::Reducer(ReducerEvent {
        status: Status::Failed(err),
        ..
    }) = &ctx.event
    {
        eprintln!("Failed to change name to {:?}: {}", name, err);
    }
}

// ## Warn if a message was rejected

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(ctx: &EventContext, text: &String) {
    if let Event::Reducer(ReducerEvent {
        status: Status::Failed(err),
        ..
    }) = &ctx.event
    {
        eprintln!("Failed to send message {:?}: {}", text, err);
    }
}

// ## Exit when disconnected

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &DbConnection, err: Option<anyhow::Error>) {
    if let Some(err) = err {
        panic!("Disconnected abnormally: {err}")
    } else {
        println!("Disconnected normally.");
        std::process::exit(0)
    }
}

// # Connect to the database

/// The URL of the SpacetimeDB instance hosting our chat module.
const HOST: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "quickstart-chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    #![allow(unreachable_code)]

    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(|err| panic!("Error while connecting: {err}"))
        .on_disconnect(on_disconnected)
        // .with_credentials(todo!())
        .with_module_name(DB_NAME)
        .with_uri(HOST)
        .build()
        .expect("Failed to connect")
}

// # Subscribe to queries

/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(ctx: &DbConnection) {
    ctx.subscription_builder().on_applied(on_sub_applied).subscribe(vec![
        "SELECT * FROM User;".to_string(),
        "SELECT * FROM Message;".to_string(),
    ]);
}

// # Handle user input

/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop(ctx: &DbConnection) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            ctx.reducers.set_name(name.to_string()).unwrap();
        } else {
            ctx.reducers.send_message(line).unwrap();
        }
    }
}
