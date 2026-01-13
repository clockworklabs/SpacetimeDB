#![allow(clippy::disallowed_macros)]

mod module_bindings;
use module_bindings::*;
use std::env;

use spacetimedb_sdk::{credentials, DbContext, Error, Identity, Table};

fn main() {
    println!("SpacetimeDB SDK Test Procedure Client");

    // Connect to the database
    let ctx = connect_to_db();

    // Register callbacks to run in response to database events.
    register_callbacks(&ctx);

    // Subscribe to tables (if any)
    subscribe_to_tables(&ctx);

    // Spawn a thread where the connection will process messages and invoke callbacks
    ctx.run_threaded();

    println!("Connected! Running procedure tests...");

    // Run procedure tests
    run_tests(&ctx);

    println!("Tests complete. Press Ctrl+C to exit.");

    // Keep the connection alive for 2 seconds to receive responses
    std::thread::sleep(std::time::Duration::from_secs(2));

    println!("Timeout reached, exiting.");
    std::process::exit(0);
}

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    let host: String = env::var("SPACETIMEDB_HOST").unwrap_or("http://localhost:3000".to_string());

    let db_name: String = env::var("SPACETIMEDB_DB_NAME").unwrap_or("sdk-test-procedure-cpp".to_string());

    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_token(creds_store().load().expect("Error loading credentials"))
        .with_module_name(db_name)
        .with_uri(host)
        .build()
        .expect("Failed to connect")
}

/// Save/load credentials
fn creds_store() -> credentials::File {
    credentials::File::new("sdk-test-procedure-cpp")
}

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(_ctx: &DbConnection, identity: Identity, token: &str) {
    println!("Connected with identity: {}", identity.to_hex());
    if let Err(e) = creds_store().save(token) {
        eprintln!("Failed to save credentials: {e:?}");
    }
}

/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Connection error: {err}");
    std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        eprintln!("Disconnected: {err}");
        std::process::exit(1);
    } else {
        println!("Disconnected.");
        std::process::exit(0);
    }
}

/// Register callbacks for procedures and tables
fn register_callbacks(_ctx: &DbConnection) {
    // Procedure callbacks are registered inline with _then() methods
}

/// Subscribe to tables
fn subscribe_to_tables(ctx: &DbConnection) {
    ctx.subscription_builder()
        .on_applied(on_sub_applied)
        .on_error(on_sub_error)
        .subscribe("SELECT * FROM my_table");
}

fn on_sub_applied(_ctx: &SubscriptionEventContext) {
    println!("Subscriptions applied.");
}

fn on_sub_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Subscription failed: {err}");
    std::process::exit(1);
}

/// Run the procedure tests
fn run_tests(ctx: &DbConnection) {
    println!("\n=== Running Procedure Tests ===\n");

    // Test return_primitive
    println!("Testing return_primitive(10, 32)...");
    ctx.procedures.return_primitive_then(10, 32, |_, res| match res {
        Ok(sum) => {
            if sum == 42 {
                println!("  ✓ return_primitive(10, 32) = {}", sum);
            } else {
                eprintln!("  ✗ Expected 42 but got {}", sum);
            }
        }
        Err(err) => {
            eprintln!("  ✗ return_primitive failed: {}", err);
        }
    });

    // Test return_struct
    println!("Testing return_struct(42, \"hello\")...");
    ctx.procedures
        .return_struct_then(42, "hello".to_string(), |_, res| match res {
            Ok(strukt) => {
                if strukt.a == 42 && &*strukt.b == "hello" {
                    println!(
                        "  ✓ return_struct(42, \"hello\") = ReturnStruct {{ a: {}, b: \"{}\" }}",
                        strukt.a, strukt.b
                    );
                } else {
                    eprintln!("  ✗ Unexpected struct values: a={}, b=\"{}\"", strukt.a, strukt.b);
                }
            }
            Err(err) => {
                eprintln!("  ✗ return_struct failed: {}", err);
            }
        });

    // Test return_enum_a
    println!("Testing return_enum_a(42)...");
    ctx.procedures.return_enum_a_then(42, |_, res| match res {
        Ok(enum_val) => {
            if matches!(enum_val, ReturnEnum::A(_)) {
                if let ReturnEnum::A(val) = enum_val {
                    if val == 42 {
                        println!("  ✓ return_enum_a(42) = ReturnEnum::A(42)");
                    } else {
                        eprintln!("  ✗ Expected A(42) but got A({})", val);
                    }
                }
            } else {
                eprintln!("  ✗ Expected A variant but got B");
            }
        }
        Err(err) => {
            eprintln!("  ✗ return_enum_a failed: {}", err);
        }
    });

    // Test return_enum_b
    println!("Testing return_enum_b(\"world\")...");
    ctx.procedures
        .return_enum_b_then("world".to_string(), |_, res| match res {
            Ok(enum_val) => {
                if matches!(enum_val, ReturnEnum::B(_)) {
                    if let ReturnEnum::B(val) = enum_val {
                        if val == "world" {
                            println!("  ✓ return_enum_b(\"world\") = ReturnEnum::B(\"world\")");
                        } else {
                            eprintln!("  ✗ Expected B(\"world\") but got B(\"{}\")", val);
                        }
                    }
                } else {
                    eprintln!("  ✗ Expected B variant but got A");
                }
            }
            Err(err) => {
                eprintln!("  ✗ return_enum_b failed: {}", err);
            }
        });

    // Test will_panic (should fail)
    println!("Testing will_panic - should fail...");
    ctx.procedures.will_panic_then(|_, res| match res {
        Ok(_) => {
            eprintln!("  ✗ Expected failure but got Ok");
        }
        Err(err) => {
            println!("  ✓ will_panic failed as expected: {}", err);
        }
    });

    // Test insert_with_tx_rollback (run first on empty table)
    println!("Testing insert_with_tx_rollback...");
    ctx.procedures.insert_with_tx_rollback_then(|ctx, res| match res {
        Ok(_) => {
            // Check that the table still has 0 rows (rollback worked)
            let count = ctx.db.my_table().count();
            if count == 0 {
                println!("  ✓ insert_with_tx_rollback rolled back successfully (0 rows in table)");
            } else {
                eprintln!("  ✗ Expected 0 rows but found {}", count);
            }
        }
        Err(err) => {
            eprintln!("  ✗ insert_with_tx_rollback failed: {}", err);
        }
    });

    // Test insert_with_tx_commit
    println!("Testing insert_with_tx_commit...");
    ctx.procedures.insert_with_tx_commit_then(|ctx, res| match res {
        Ok(_) => {
            // Check that the table has 1 row
            let count = ctx.db.my_table().count();
            if count == 1 {
                println!("  ✓ insert_with_tx_commit committed successfully (1 row in table)");
            } else {
                eprintln!("  ✗ Expected 1 row but found {}", count);
            }
        }
        Err(err) => {
            eprintln!("  ✗ insert_with_tx_commit failed: {}", err);
        }
    });

    // Test read_my_schema (HTTP)
    println!("Testing read_my_schema (HTTP)...");
    ctx.procedures.read_my_schema_then(|_, res| match res {
        Ok(schema) => {
            if !schema.is_empty() {
                println!("  ✓ read_my_schema returned schema data ({} bytes)", schema.len());
                if schema.len() > 100 {
                    println!("    Preview: {}...", &schema[..100]);
                } else {
                    println!("    Data: {}", schema);
                }
            } else {
                eprintln!("  ✗ read_my_schema returned empty string");
            }
        }
        Err(err) => {
            eprintln!("  ✗ read_my_schema failed: {}", err);
        }
    });

    // Test invalid_request (HTTP error handling)
    println!("Testing invalid_request (HTTP)...");
    ctx.procedures.invalid_request_then(|_, res| match res {
        Ok(error_msg) => {
            if !error_msg.is_empty() {
                println!("  ✓ invalid_request returned error message: {}", error_msg);
            } else {
                eprintln!("  ✗ invalid_request returned empty error");
            }
        }
        Err(err) => {
            eprintln!("  ✗ invalid_request failed: {}", err);
        }
    });

    // Test simple HTTP GET request
    println!("Testing test_simple_http (HTTP)...");
    ctx.procedures.test_simple_http_then(|_, res| match res {
        Ok(result) => {
            println!("  ✓ test_simple_http: {}", result);
        }
        Err(err) => {
            eprintln!("  ✗ test_simple_http failed: {}", err);
        }
    });
}
