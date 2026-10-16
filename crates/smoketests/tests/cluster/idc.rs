use std::time::{Duration, Instant};

use spacetimedb_smoketests::{
    copy_module_fixture, parse_identity_from_publish_output, patch_module_cargo_to_local_bindings, random_string,
    Smoketest,
};

struct IdcTest {
    test: Smoketest,
    receiver: String,
    sender: String,
}

impl IdcTest {
    fn new() -> Self {
        let mut test = Smoketest::builder().autopublish(false).build();
        let receiver_db = format!("idc-receiver-{}", random_string());
        let sender_db = format!("idc-sender-{}", random_string());

        for name in ["receiver", "sender"] {
            let dir = test.project_dir.path().join(name);
            copy_module_fixture(&format!("idc-{name}"), &dir).expect("failed to copy module fixture");
            patch_module_cargo_to_local_bindings(&dir).expect("failed to patch module Cargo.toml");
        }
        std::fs::write(
            test.project_dir.path().join("spacetime.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "server": test.server_url,
                "database": receiver_db,
                "module-path": "./receiver",
                "children": [
                    {
                        "database": sender_db,
                        "module-path": "./sender",
                        "dependencies": [{ "name": "game_world", "database": receiver_db }]
                    }
                ]
            }))
            .unwrap(),
        )
        .expect("failed to write spacetime.json");
        test.spacetime(&["generate", &sender_db])
            .expect("failed to generate receiver bindings");
        test.spacetime(&["publish", "--server", &test.server_url, "--yes=all"])
            .expect("failed to publish IDC modules");

        let resolve_identity = |database: &str| {
            let response = test
                .api_call("GET", &format!("/v1/database/{database}/identity"))
                .expect("failed to resolve database identity");
            assert!(
                response.is_success(),
                "identity lookup failed for {database}: HTTP {}",
                response.status_code
            );
            response.text().expect("invalid database identity response")
        };
        let receiver = resolve_identity(&receiver_db);
        let sender = resolve_identity(&sender_db);
        test.database_identity = Some(sender.clone());
        Self { test, receiver, sender }
    }

    fn publish(&self, module: &str, database: &str) -> String {
        let output = self
            .test
            .spacetime(&[
                "publish",
                "--no-config",
                "--server",
                &self.test.server_url,
                "--module-path",
                self.test.project_dir.path().join(module).to_str().unwrap(),
                "--yes=all",
                database,
            ])
            .expect("failed to publish module update");
        parse_identity_from_publish_output(&output).expect("publish output should contain database identity")
    }

    fn rows(&self, database: &str, sql: &str) -> Vec<serde_json::Value> {
        let output = self
            .test
            .spacetime(&[
                "sql",
                "--no-config",
                "--server",
                &self.test.server_url,
                "--format",
                "json",
                database,
                sql,
            ])
            .expect("failed to query database");
        let results: serde_json::Value = serde_json::from_str(&output).expect("invalid SQL JSON output");
        results[0]["rows"]
            .as_array()
            .expect("SQL result should contain rows")
            .clone()
    }

    fn enqueue(&self, bodies: &[String]) {
        self.test
            .call(
                "enqueue_pings",
                &[
                    &serde_json::to_string(&self.receiver).unwrap(),
                    &serde_json::to_string(bodies).unwrap(),
                ],
            )
            .expect("failed to enqueue pings");
    }

    fn wait_for_messages(&self, expected: &[(String, u32)]) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            // The auto-increment columns record execution order, independently of message IDs.
            let mut delivered = self.rows(&self.receiver, "SELECT id, body, module_version FROM ping_log");
            let mut callbacks = self.rows(
                &self.sender,
                "SELECT callback_order, body, module_version, succeeded, error FROM ping_result",
            );
            delivered.sort_by_key(|row| row[0].as_u64().unwrap());
            callbacks.sort_by_key(|row| row[0].as_u64().unwrap());
            let pending = self.rows(&self.sender, "SELECT id FROM outbound_ping");
            assert!(delivered.len() <= expected.len(), "duplicate deliveries: {delivered:?}");
            assert!(callbacks.len() <= expected.len(), "duplicate callbacks: {callbacks:?}");
            if delivered.len() == expected.len() && callbacks.len() == expected.len() && pending.is_empty() {
                for (index, (body, version)) in expected.iter().enumerate() {
                    assert_eq!(delivered[index][1], *body, "receiver delivery order");
                    assert_eq!(delivered[index][2], *version, "receiver module version");
                    assert_eq!(callbacks[index][1], *body, "sender callback order");
                    assert_eq!(callbacks[index][2], *version, "sender module version");
                    assert_eq!(callbacks[index][3], true, "callback failed: {:?}", callbacks[index]);
                }
                return;
            }
            assert!(
                Instant::now() < deadline,
                "IDC did not finish: delivered={delivered:?}, callbacks={callbacks:?}, pending={pending:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn send_ping_to(&self, receiver: &str, body: &str) {
        self.test
            .call(
                "enqueue_ping",
                &[
                    &serde_json::to_string(receiver).unwrap(),
                    &serde_json::to_string(body).unwrap(),
                ],
            )
            .expect("failed to enqueue ping");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let callbacks = self.rows(
                &self.sender,
                &format!("SELECT succeeded FROM ping_result WHERE body = '{body}'"),
            );
            let pending = self.rows(&self.sender, "SELECT id FROM outbound_ping");
            if !callbacks.is_empty() && pending.is_empty() {
                assert_eq!(callbacks, vec![serde_json::json!([true])]);
                assert_eq!(
                    self.rows(receiver, &format!("SELECT body FROM ping_log WHERE body = '{body}'")),
                    vec![serde_json::json!([body])],
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "ping {body} did not finish: {callbacks:?}, {pending:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn send_detailed_ping(&self, body: &str) {
        self.test
            .call(
                "enqueue_detailed_ping",
                &[
                    &serde_json::to_string(&self.receiver).unwrap(),
                    &serde_json::to_string(body).unwrap(),
                ],
            )
            .expect("failed to enqueue detailed ping");
        let deadline = Instant::now() + Duration::from_secs(20);
        let expected_details = serde_json::json!([
            ["nested", "idc"],
            [[1, [0, []]], [2, [1, [0, "waiting"]]], [3, [1, [1, []]]]],
            [0, "optional-note"]
        ]);
        loop {
            let delivered = self.rows(
                &self.receiver,
                &format!("SELECT details FROM detailed_ping_log WHERE body = '{body}'"),
            );
            let callbacks = self.rows(
                &self.sender,
                &format!("SELECT details FROM detailed_ping_result WHERE body = '{body}'"),
            );
            let pending = self.rows(&self.sender, "SELECT id FROM outbound_detailed_ping");
            if !callbacks.is_empty() && pending.is_empty() {
                assert_eq!(delivered, vec![serde_json::json!([expected_details])]);
                assert_eq!(callbacks, vec![serde_json::json!([expected_details])]);
                return;
            }
            assert!(
                Instant::now() < deadline,
                "detailed IDC ping did not finish: delivered={delivered:?}, callbacks={callbacks:?}, pending={pending:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn edit_module(&self, module: &str, from: &str, to: &str) {
        let path = self.test.project_dir.path().join(module).join("src/lib.rs");
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(source.contains(from), "missing source in {module}: {from}");
        std::fs::write(&path, source.replace(from, to)).unwrap();
    }

    fn update_modules(&self, version: u32) {
        for (module, database) in [("receiver", &self.receiver), ("sender", &self.sender)] {
            self.edit_module(
                module,
                &format!("const MODULE_VERSION: u32 = {};", version - 1),
                &format!("const MODULE_VERSION: u32 = {version};"),
            );
            assert_eq!(self.publish(module, database), *database);
        }
    }
}

#[test]
fn idc_outbox_delivers_between_two_rust_modules() {
    let suite = IdcTest::new();
    suite
        .test
        .call(
            "enqueue_ping",
            &[&serde_json::to_string(&suite.receiver).unwrap(), "\"hello-idc\""],
        )
        .expect("failed to enqueue ping");
    suite.wait_for_messages(&[("hello-idc".into(), 1)]);
}

#[test]
fn idc_outbox_delivers_twenty_messages_in_order() {
    let suite = IdcTest::new();
    let bodies: Vec<_> = (0..20).map(|index| format!("message-{index}")).collect();
    suite.enqueue(&bodies);
    suite.wait_for_messages(&bodies.into_iter().map(|body| (body, 1)).collect::<Vec<_>>());
}

#[test]
fn idc_outbox_delivers_to_two_databases_with_the_same_receiver_module() {
    let suite = IdcTest::new();
    let second_receiver = suite.publish("receiver", &format!("idc-receiver-{}", random_string()));
    assert_ne!(suite.receiver, second_receiver);

    // The same bindings work for both databases before either receiver changes.
    suite.send_ping_to(&suite.receiver, "first-receiver");
    suite.send_ping_to(&second_receiver, "second-receiver");

    // Update only the first receiver. Existing rows get revision 0; new calls supply it.
    suite.edit_module(
        "receiver",
        "    module_version: u32,\n    body: String,",
        "    module_version: u32,\n    body: String,\n    #[default(0)]\n    revision: u32,",
    );
    suite.edit_module(
        "receiver",
        "pub fn receive_ping(ctx: &spacetimedb::ReducerContext, body: String)",
        "pub fn receive_ping(ctx: &spacetimedb::ReducerContext, body: String, revision: u32)",
    );
    suite.edit_module("receiver", "        body,\n", "        body,\n        revision,\n");
    assert_eq!(suite.publish("receiver", &suite.receiver), suite.receiver);
    assert_eq!(
        suite.rows(&suite.receiver, "SELECT body, revision FROM ping_log"),
        vec![serde_json::json!(["first-receiver", 0])],
    );

    // Regenerate the first receiver's bindings and add the new argument to the sender's outbox.
    suite
        .test
        .spacetime(&["generate"])
        .expect("failed to regenerate receiver bindings");
    suite.edit_module(
        "sender",
        "    bodypo: String,",
        "    bodypo: String,\n    #[default(0)]\n    revision: u32,",
    );
    suite.edit_module(
        "sender",
        "        bodypo: body,",
        "        bodypo: body,\n        revision: 7,",
    );
    assert_eq!(suite.publish("sender", &suite.sender), suite.sender);
    suite.send_ping_to(&suite.receiver, "after-update");
    assert_eq!(
        suite.rows(
            &suite.receiver,
            "SELECT revision FROM ping_log WHERE body = 'after-update'"
        ),
        vec![serde_json::json!([7])],
    );
    assert_eq!(
        suite.rows(&second_receiver, "SELECT body FROM ping_log"),
        vec![serde_json::json!(["second-receiver"])],
    );
}

#[test]
fn idc_outboxes_deliver_in_both_directions_concurrently() {
    let suite = IdcTest::new();
    let peer = suite.publish("sender", &format!("idc-peer-{}", random_string()));
    let from_sender: Vec<_> = (0..20).map(|index| format!("sender-{index}")).collect();
    let from_peer: Vec<_> = (0..20).map(|index| format!("peer-{index}")).collect();

    // Release both calls together, without waiting for either direction's deliveries.
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        for (source, target, bodies) in [(&suite.sender, &peer, &from_sender), (&peer, &suite.sender, &from_peer)] {
            let (test, start) = (&suite.test, &start);
            scope.spawn(move || {
                start.wait();
                test.spacetime(&[
                    "call",
                    "--no-config",
                    "--server",
                    &test.server_url,
                    "--",
                    source,
                    "enqueue_pings",
                    &serde_json::to_string(target).unwrap(),
                    &serde_json::to_string(bodies).unwrap(),
                ])
                .expect("failed to enqueue concurrent pings");
            });
        }
    });

    for (database, incoming, outgoing) in [
        (&suite.sender, &from_peer, &from_sender),
        (&peer, &from_sender, &from_peer),
    ] {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let mut delivered = suite.rows(database, "SELECT id, body FROM ping_log");
            let mut callbacks = suite.rows(database, "SELECT callback_order, body, succeeded FROM ping_result");
            let pending = suite.rows(database, "SELECT id FROM outbound_ping");
            assert!(delivered.len() <= incoming.len(), "duplicate deliveries: {delivered:?}");
            assert!(callbacks.len() <= outgoing.len(), "duplicate callbacks: {callbacks:?}");
            if delivered.len() == incoming.len() && callbacks.len() == outgoing.len() && pending.is_empty() {
                delivered.sort_by_key(|row| row[0].as_u64().unwrap());
                callbacks.sort_by_key(|row| row[0].as_u64().unwrap());
                for (row, body) in delivered.iter().zip(incoming) {
                    assert_eq!(row[1], *body, "delivery order for {database}");
                }
                for (row, body) in callbacks.iter().zip(outgoing) {
                    assert_eq!(row[1], *body, "callback order for {database}");
                    assert_eq!(row[2], true, "callback failed: {row:?}");
                }
                break;
            }
            assert!(
                Instant::now() < deadline,
                "bidirectional IDC did not finish for {database}: delivered={delivered:?}, callbacks={callbacks:?}, pending={pending:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn idc_outbox_preserves_order_across_module_updates() {
    let suite = IdcTest::new();
    let mut expected = Vec::new();
    for version in 1..=4 {
        if version == 2 {
            // Rename the Rust accessor while preserving the canonical name used by the old bindings.
            suite.edit_module(
                "receiver",
                "#[spacetimedb::reducer]\npub fn receive_ping",
                "#[spacetimedb::reducer(name = \"receive_ping\")]\npub fn accept_ping",
            );
        }
        if version > 1 {
            suite.update_modules(version);
        }
        let bodies: Vec<_> = (expected.len()..expected.len() + 5)
            .map(|index| format!("message-{index}"))
            .collect();
        suite.enqueue(&bodies);
        expected.extend(bodies.into_iter().map(|body| (body, version)));
        suite.wait_for_messages(&expected);
    }
}

#[test]
fn idc_outbox_uses_regenerated_bindings_after_argument_reordering() {
    let suite = IdcTest::new();
    suite.send_detailed_ping("before-update");

    // Changing parameter order changes the signature hash and the outbox's argument-column mapping.
    // The stored row types stay the same, so both databases can be updated without clearing data.
    suite.edit_module(
        "receiver",
        "body: String, details: PingDetails",
        "details: PingDetails, body: String",
    );
    suite
        .test
        .spacetime(&["generate"])
        .expect("failed to regenerate receiver bindings");
    assert_eq!(suite.publish("receiver", &suite.receiver), suite.receiver);
    // The regenerated bindings map the unchanged sender columns into the new argument order.
    assert_eq!(suite.publish("sender", &suite.sender), suite.sender);

    suite.send_detailed_ping("after-update");
}
