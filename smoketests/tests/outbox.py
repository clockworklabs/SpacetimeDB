import time

from .. import Smoketest, random_string


class OutboxPingPong(Smoketest):
    AUTOPUBLISH = False

    RECEIVER_MODULE = """
use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType)]
pub struct Ping {
    payload: String,
}

#[spacetimedb::table(accessor = received_pings, public)]
pub struct ReceivedPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: Identity,
    payload: String,
}

#[spacetimedb::reducer]
pub fn receive_ping(ctx: &ReducerContext, ping: Ping) {
    ctx.db.received_pings().insert(ReceivedPing {
        id: 0,
        sender: ctx.sender(),
        payload: ping.payload,
    });
}
"""

    # Receiver that always returns an error from the reducer.
    FAILING_RECEIVER_MODULE = """
use spacetimedb::{ReducerContext, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Ping {
    payload: String,
}

#[spacetimedb::reducer]
pub fn receive_ping(ctx: &ReducerContext, ping: Ping) -> Result<(), String> {
    Err(format!("deliberate failure for payload: {}", ping.payload))
}
"""

    SENDER_MODULE = """
use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType)]
pub struct Ping {
    payload: String,
}

#[spacetimedb::table(accessor = outbound_pings, public, outbox(receive_ping, on_result = local_callback))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    target: Identity,
    ping: Ping,
}

#[spacetimedb::table(accessor = callback_results, public)]
pub struct CallbackResult {
    #[primary_key]
    #[auto_inc]
    id: u64,
    payload: String,
    status: String,
}

#[spacetimedb::reducer]
pub fn send_ping(ctx: &ReducerContext, target_hex: String, payload: String) {
    let target = Identity::from_hex(&target_hex).expect("target identity should be valid hex");
    ctx.db.outbound_pings().insert(OutboundPing {
        id: 0,
        target,
        ping: Ping { payload },
    });
}

#[spacetimedb::reducer]
pub fn local_callback(ctx: &ReducerContext, request: OutboundPing, result: Result<(), String>) {
    let status = match result {
        Ok(()) => "ok".to_string(),
        Err(err) => format!("err:{err}"),
    };
    ctx.db.callback_results().insert(CallbackResult {
        id: 0,
        payload: request.ping.payload,
        status,
    });
}
"""

    # --- helpers ---

    def sql_on(self, database_identity, query):
        return self.spacetime("sql", "--anonymous", "--", database_identity, query)

    def publish_module_with_cleanup(self, module_code, name_prefix):
        """Publish a module and register a cleanup that deletes it. Returns its identity."""
        self.write_module_code(module_code)
        self.publish_module(f"{name_prefix}-{random_string()}", clear=False)
        identity = self.database_identity
        self.addCleanup(lambda db=identity: self.spacetime("delete", "--yes", db))
        return identity

    def poll_until(self, condition, timeout, poll_interval=0.1, timeout_msg="timed out"):
        """
        Poll `condition()` every `poll_interval` seconds until it returns a truthy
        value or `timeout` seconds elapse. Returns the truthy value on success,
        or calls self.fail() on timeout.
        """
        deadline = time.time() + timeout
        while True:
            result = condition()
            if result:
                return result
            if time.time() >= deadline:
                msg = timeout_msg() if callable(timeout_msg) else timeout_msg
                self.fail(msg)
            time.sleep(poll_interval)

    def assert_callback_result(self, sender_identity, payload, expected_status_fragment):
        """Assert that exactly one callback row exists for `payload` with the expected status."""
        results = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
        self.assertEqual(results.count(f'"{payload}"'), 1, results)
        self.assertIn(expected_status_fragment, results, results)

    def assert_no_callback_yet(self, sender_identity, payload):
        """Assert that no callback row has been recorded for `payload` yet."""
        results = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
        self.assertNotIn(
            f'"{payload}"',
            results,
            f"callback fired unexpectedly for payload '{payload}':\n{results}",
        )

    def assert_not_redelivered(self, sender_identity, payload, expected_status_fragment, wait=1.0):
        """After a short wait, confirm the callback row count has not grown."""
        time.sleep(wait)
        self.assert_callback_result(sender_identity, payload, expected_status_fragment)

    def poll_for_callback(self, sender_identity, payload, status_fragment, timeout):
        """Poll until a callback row with the given payload and status fragment appears."""
        self.poll_until(
            lambda: (
                f'"{payload}"' in (r := self.sql_on(sender_identity, "SELECT payload, status FROM callback_results"))
                and status_fragment in r
            ),
            timeout=timeout,
            timeout_msg=lambda: (
                f"timed out waiting for callback with status '{status_fragment}', last query output:\n"
                + self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
            ),
        )

    # --- tests ---

    def test_outbox_ping_from_sender_module_reaches_receiver_module(self):
        receiver_identity = self.publish_module_with_cleanup(self.RECEIVER_MODULE, "outbox-receiver")
        sender_identity = self.publish_module_with_cleanup(self.SENDER_MODULE, "outbox-sender")

        payload = "ping"
        self.call("send_ping", receiver_identity, payload)

        self.poll_until(
            lambda: f'"{payload}"' in self.sql_on(receiver_identity, "SELECT * FROM received_pings"),
            timeout=8,
            timeout_msg=lambda: (
                "timed out waiting for ping delivery, last query output:\n"
                + self.sql_on(receiver_identity, "SELECT * FROM received_pings")
            ),
        )

        self.poll_for_callback(sender_identity, payload, '"ok"', timeout=8)
        self.assert_callback_result(sender_identity, payload, '"ok"')
        self.assert_not_redelivered(sender_identity, payload, '"ok"')

    def test_outbox_callback_receives_error_when_remote_reducer_fails(self):
        """
        When the remote reducer returns Err(...), the sender's on_result callback
        should be invoked with the error string, not "ok".
        """
        receiver_identity = self.publish_module_with_cleanup(self.FAILING_RECEIVER_MODULE, "outbox-failing-receiver")
        sender_identity = self.publish_module_with_cleanup(self.SENDER_MODULE, "outbox-sender")

        payload = "ping-that-will-fail"
        self.call("send_ping", receiver_identity, payload)

        self.poll_for_callback(sender_identity, payload, '"err:', timeout=8)

        results = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
        self.assertEqual(results.count(f'"{payload}"'), 1, results)
        self.assertIn('"err:', results)
        self.assertIn("deliberate failure for payload", results)
        self.assertNotIn('"ok"', results)

        # Reducer errors are terminal — must not be retried.
        self.assert_not_redelivered(sender_identity, payload, '"err:')

    def test_outbox_retries_delivery_until_remote_module_is_available(self):
        """
        When a message is sent to a non-existent target database, the IDC actor
        should receive a transport error (HTTP 404/503) and retry with backoff.
        Once the target database is published, the message should eventually be
        delivered and the callback fired with "ok".
        """
        sender_identity = self.publish_module_with_cleanup(self.SENDER_MODULE, "outbox-sender")

        # Publish then immediately delete a receiver to obtain a real identity
        # that is currently absent from the system.
        self.write_module_code(self.RECEIVER_MODULE)
        receiver_name = f"outbox-receiver-{random_string()}"
        self.publish_module(receiver_name, clear=False)
        receiver_identity = self.database_identity
        self.spacetime("delete", "--yes", receiver_identity)

        payload = "retry-ping"
        self.database_identity = sender_identity
        self.call("send_ping", receiver_identity, payload)

        # Give the IDC actor a moment to attempt delivery and hit a transport error.
        time.sleep(1.0)
        self.assert_no_callback_yet(sender_identity, payload)

        # Bring the receiver back online under the same name (same identity).
        self.write_module_code(self.RECEIVER_MODULE)
        self.publish_module(receiver_name, clear=False)
        receiver_identity_new = self.database_identity
        self.addCleanup(lambda db=receiver_identity_new: self.spacetime("delete", "--yes", db))

        self.assertEqual(
            receiver_identity,
            receiver_identity_new,
            "re-published receiver identity differs; retry test requires a stable identity",
        )

        # Wait for the IDC actor to retry and succeed (allow for exponential backoff).
        self.poll_for_callback(sender_identity, payload, '"ok"', timeout=30)
        self.assert_callback_result(sender_identity, payload, '"ok"')
        self.assert_not_redelivered(sender_identity, payload, '"ok"')
