mod module_bindings;
use module_bindings::*;
use std::env;
use std::io::{self, Read};

use spacetimedb_sdk::{DbContext, Table};

fn main() {
    // The URI of the SpacetimeDB instance hosting our chat module.
    let host: String = env::var("SPACETIMEDB_HOST").unwrap_or("http://localhost:3000".to_string());

    // The module name we chose when we published our module.
    let db_name: String = env::var("SPACETIMEDB_DB_NAME").unwrap_or("my-db".to_string());

    // Connect to the database
    let conn = DbConnection::builder()
        .with_module_name(db_name)
        .with_uri(host)
        .on_connect(|_, _, _| {
            println!("Connected to SpacetimeDB");
        })
        .on_connect_error(|_ctx, e| {
            eprintln!("Connection error: {:?}", e);
            std::process::exit(1);
        })
        .build()
        .expect("Failed to connect");

    conn.run_threaded();

    // Subscribe to the person table
    conn.subscription_builder()
        .on_applied(|_ctx| println!("Subscripted to the person table"))
        .on_error(|_ctx, e| eprintln!("There was an error when subscring to the person table: {e}"))
        .subscribe(["SELECT * FROM person"]);

    // Register a callback for when rows are inserted into the person table
    conn.db().person().on_insert(|_ctx, person| {
        println!("New person: {}", person.name);
    });

    println!("Press any key to exit...");

    let _ = io::stdin().read(&mut [0u8]).unwrap();
}
