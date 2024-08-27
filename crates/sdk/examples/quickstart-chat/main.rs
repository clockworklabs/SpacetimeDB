mod module_bindings;

use module_bindings::*;

use spacetimedb_sdk::{
    disconnect,
    identity::{load_credentials, once_on_connect, save_credentials, Credentials, Identity},
    on_disconnect, on_subscription_applied,
    reducer::Status,
    subscribe,
    table::{TableType, TableWithPrimaryKey},
    Address,
};

// # Our main function

fn main() {
    register_callbacks();
    connect_to_db();
    subscribe_to_tables();
    user_input_loop();
    disconnect();
}

// # Register callbacks

/// Register all the callbacks our app will use to respond to database events.
fn register_callbacks() {
    // When we receive our `Credentials`, save them to a file.
    once_on_connect(on_connected);

    // When a new user joins, print a notification.
    User::on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    User::on_update(on_user_updated);

    // When a new message is received, print it.
    Message::on_insert(on_message_inserted);

    // When we receive the message backlog, print it in timestamp order.
    on_subscription_applied(on_sub_applied);

    // When we fail to set our name, print a warning.
    on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    on_send_message(on_message_sent);

    // When our connection closes, inform the user and exit.
    on_disconnect(on_disconnected);
}

// ## Save credentials to a file

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(creds: &Credentials, _address: Address) {
    if let Err(e) = save_credentials(CREDS_DIR, creds) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}

const CREDS_DIR: &str = ".spacetime_chat";

// ## Notify about new users

/// Our `User::on_insert` callback: if the user is online, print a notification.
fn on_user_inserted(user: &User, _: Option<&ReducerEvent>) {
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
fn on_user_updated(old: &User, new: &User, _: Option<&ReducerEvent>) {
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
fn on_message_inserted(message: &Message, reducer_event: Option<&ReducerEvent>) {
    if reducer_event.is_some() {
        print_message(message);
    }
}

fn print_message(message: &Message) {
    let sender = User::find_by_identity(message.sender)
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    println!("{}: {}", sender, message.text);
}

// ## Print message backlog

/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
fn on_sub_applied() {
    let mut messages = Message::iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(&message);
    }
}

// ## Warn if set_name failed

/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(_sender_id: &Identity, _sender_addr: Option<Address>, status: &Status, name: &String) {
    if let Status::Failed(err) = status {
        eprintln!("Failed to change name to {:?}: {}", name, err);
    }
}

// ## Warn if a message was rejected

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(_sender: &Identity, _sender_addr: Option<Address>, status: &Status, text: &String) {
    if let Status::Failed(err) = status {
        eprintln!("Failed to send message {:?}: {}", text, err);
    }
}

// ## Exit when disconnected

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected() {
    eprintln!("Disconnected!");
    std::process::exit(0)
}

// # Connect to the database

/// The URL of the SpacetimeDB instance hosting our chat module.
const HOST: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "quickstart-chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() {
    connect(
        HOST,
        DB_NAME,
        load_credentials(CREDS_DIR).expect("Error reading stored credentials"),
        None,
    )
    .expect("Failed to connect");
}

// # Subscribe to queries

/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables() {
    subscribe(&["SELECT * FROM User;", "SELECT * FROM Message;"]).unwrap();
}

// # Handle user input

/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop() {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            set_name(name.to_string());
        } else {
            send_message(line);
        }
    }
}
