mod module_bindings;
use module_bindings::*;

use spacetimedb_sdk::{DbConnection, Table};

const HOST: &str = "http://localhost:3000";
const DB_NAME: &str = "my-db";

fn main() {
    // Connect to the database
    let conn = DbConnection::builder()
        .with_module_name(DB_NAME)
        .with_host(HOST)
        .on_connect(|_, _, _| {
            println!("Connected to SpacetimeDB");
        })
        .on_connect_error(|e| {
            eprintln!("Connection error: {:?}", e);
            std::process::exit(1);
        })
        .build()
        .expect("Failed to connect");

    // Subscribe to the person table
    conn.subscribe(&[
        "SELECT * FROM person"
    ]);

    // Register a callback for when rows are inserted into the person table
    Person::on_insert(|_ctx, person| {
        println!("New person: {}", person.name);
    });

    // Run the connection on the current thread
    // This will block and handle all database events
    conn.run();
}
