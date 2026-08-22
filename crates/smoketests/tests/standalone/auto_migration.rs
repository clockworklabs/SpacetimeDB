use spacetimedb_smoketests::{require_local_server, Smoketest};

/// Regression test: dropping an event table must not brick commitlog replay.
///
/// Dropping an event table deletes its `st_table`, `st_column` and `st_event_table` rows
/// in a single transaction. Replay applies deletes in ascending table id order,
/// so the `st_table` row is already gone when the `st_column` deletes are replayed,
/// while the `st_event_table` row is still present.
/// Replay therefore treated the dropped table as a live event table
/// and tried to refresh its layout, failing with
/// `Table with ID ... not found in st_table`
/// and permanently preventing the database from starting.
#[test]
fn automigrate_drop_event_table_replays_after_restart() {
    require_local_server!();
    let mut test = Smoketest::builder()
        .precompiled_module("auto-migration-drop-event-table-before")
        .build();

    let identity = test
        .database_identity
        .clone()
        .expect("database should be published after build");

    // Write some history, including an event row.
    test.call("add_person", &["Robert"]).unwrap();
    test.call("emit_event", &[]).unwrap();

    // Drop the event table.
    test.use_precompiled_module("auto-migration-drop-event-table-after");
    test.publish()
        .name(&identity)
        .break_clients(true)
        .run()
        .expect("Dropping the event table should succeed");

    // Wait until data written after the drop is durable,
    // which implies the drop itself is durable too.
    test.call("add_person", &["Julie"]).unwrap();
    let output = test.sql_confirmed("SELECT * FROM person WHERE name = 'Julie'").unwrap();
    assert!(output.contains("Julie"), "Data not confirmed before restart: {output}");

    // Restarting forces a commitlog replay, which must replay the event table drop.
    test.restart_server();

    let output = test.sql("SELECT name FROM person").unwrap();
    assert!(output.contains("Robert"), "Expected 'Robert' after restart: {output}");
    assert!(output.contains("Julie"), "Expected 'Julie' after restart: {output}");

    // The database should still accept writes after replay.
    test.call("add_person", &["Samantha"]).unwrap();
    let output = test.sql("SELECT name FROM person WHERE name = 'Samantha'").unwrap();
    assert!(
        output.contains("Samantha"),
        "Expected 'Samantha' after restart: {output}"
    );
}
