mod module_bindings;
use module_bindings::*;
use std::env;

use spacetimedb_sdk::{DbConnection, Table};

fn main() {
    // The URI of the SpacetimeDB instance hosting our chat module.
    let host: String = env::var("SPACETIMEDB_HOST").unwrap_or("http://localhost:3000".to_string());

    // The module name we chose when we published our module.
    let db_name: String = env::var("SPACETIMEDB_DB_NAME").unwrap_or("my-db".to_string());

    // Connect to the database
    let conn = DbConnection::builder()
        .with_module_name(db_name)
        .with_host(host)
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
