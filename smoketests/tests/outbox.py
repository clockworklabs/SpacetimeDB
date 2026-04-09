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

    def sql_on(self, database_identity, query):
        return self.spacetime("sql", "--anonymous", "--", database_identity, query)

    def test_outbox_ping_from_sender_module_reaches_receiver_module(self):
        self.write_module_code(self.RECEIVER_MODULE)
        receiver_name = f"outbox-receiver-{random_string()}"
        self.publish_module(receiver_name, clear=False)
        receiver_identity = self.database_identity
        self.addCleanup(lambda db=receiver_identity: self.spacetime("delete", "--yes", db))

        self.write_module_code(self.SENDER_MODULE)
        sender_name = f"outbox-sender-{random_string()}"
        self.publish_module(sender_name, clear=False)
        sender_identity = self.database_identity
        self.addCleanup(lambda db=sender_identity: self.spacetime("delete", "--yes", db))

        payload = "ping"
        self.call("send_ping", receiver_identity, payload)

        deadline = time.time() + 8
        while True:
            output = self.sql_on(receiver_identity, "SELECT * FROM received_pings")
            if f'"{payload}"' in output:
                break
            if time.time() >= deadline:
                self.fail(f"timed out waiting for ping delivery, last query output:\n{output}")
            time.sleep(0.1)

        received = self.sql_on(receiver_identity, "SELECT * FROM received_pings")
        self.assertIn(f'"{payload}"', received)

        callback_deadline = time.time() + 8
        while True:
            callback_results = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
            if f'"{payload}"' in callback_results and '"ok"' in callback_results:
                break
            if time.time() >= callback_deadline:
                self.fail(
                    "timed out waiting for callback result, last query output:\n"
                    f"{callback_results}"
                )
            time.sleep(0.1)

        callback_results = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
        self.assertEqual(callback_results.count(f'"{payload}"'), 1, callback_results)
        self.assertEqual(callback_results.count('"ok"'), 1, callback_results)

        time.sleep(1.0)

        callback_results_after_wait = self.sql_on(sender_identity, "SELECT payload, status FROM callback_results")
        self.assertEqual(callback_results_after_wait.count(f'"{payload}"'), 1, callback_results_after_wait)
        self.assertEqual(callback_results_after_wait.count('"ok"'), 1, callback_results_after_wait)
