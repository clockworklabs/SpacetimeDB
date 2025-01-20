import time
from .. import Smoketest, run_cmd, requires_docker, COMPOSE_FILE
from urllib.request import urlopen
from .add_remove_index import AddRemoveIndex


def restart_docker():

    # Behold!
    #
    # You thought stop/start restarts? How wrong. Restart restarts.
    run_cmd("docker", "compose", "-f", COMPOSE_FILE, "restart")
    # The suspense!
    #
    # Wait until compose believes the health probe succeeds.
    #
    # The container may decide to recompile, or grab a coffee at crates.io, or
    # whatever. In any case, restart doesn't mean the server is up yet.
    run_cmd("docker", "compose","-f", COMPOSE_FILE,  "up", "--no-recreate", "--detach", "--wait-timeout", "60")
    # Belts and suspenders!
    #
    # The health probe runs inside the container, but that doesn't mean we can
    # reach it from outside. Ping until we get through.
    ping()

def ping():
    tries = 0
    host = "127.0.0.1:3000"
    while tries < 10:
        tries += 1
        try:
            print(f"Ping Server at {host}")
            urlopen(f"http://{host}/database/ping")
            print("Server up")
            break
        except Exception:
            print("Server down")
            time.sleep(3)
    else:
        raise Exception(f"Server at {host} not responding")
    print(f"Server up after {tries} tries")


@requires_docker
class DockerRestartModule(Smoketest):
    # Note: creating indexes on `Person`
    # exercises more possible failure cases when replaying after restart
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person, index(name = name_idx, btree(columns = [name])))]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"""

    def test_restart_module(self):
        """This tests to see if SpacetimeDB can be queried after a restart"""

        self.call("add", "Robert")

        restart_docker()

        self.call("add", "Julie")
        self.call("add", "Samantha")
        self.call("say_hello")
        logs = self.logs(100)
        self.assertIn("Hello, Samantha!", logs)
        self.assertIn("Hello, Julie!", logs)
        self.assertIn("Hello, Robert!", logs)
        self.assertIn("Hello, World!", logs)


@requires_docker
class DockerRestartSql(Smoketest):
    # Note: creating indexes on `Person`
    # exercises more possible failure cases when replaying after restart
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person, index(name = name_idx, btree(columns = [name])))]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"""

    def test_restart_module(self):
        """This tests to see if SpacetimeDB can be queried after a restart"""

        self.call("add", "Robert")
        self.call("add", "Julie")
        self.call("add", "Samantha")
        self.call("say_hello")
        logs = self.logs(100)
        self.assertIn("Hello, Samantha!", logs)
        self.assertIn("Hello, Julie!", logs)
        self.assertIn("Hello, Robert!", logs)
        self.assertIn("Hello, World!", logs)

        restart_docker()

        sql_out = self.spacetime("sql", self.database_identity, "SELECT name FROM person WHERE id = 3")
        self.assertMultiLineEqual(sql_out, """ name       \n------------\n "Samantha" \n""")

@requires_docker
class DockerRestartAutoDisconnect(Smoketest):
    MODULE_CODE = """
use log::info;
use spacetimedb::{Address, Identity, ReducerContext, Table};

#[spacetimedb::table(name = connected_client)]
pub struct ConnectedClient {
    identity: Identity,
    address: Address,
}

#[spacetimedb::reducer(client_connected)]
fn on_connect(ctx: &ReducerContext) {
    ctx.db.connected_client().insert(ConnectedClient {
        identity: ctx.sender,
        address: ctx.address.expect("sender address unset"),
    });
}

#[spacetimedb::reducer(client_disconnected)]
fn on_disconnect(ctx: &ReducerContext) {
    let sender_identity = &ctx.sender;
    let sender_address = ctx.address.as_ref().expect("sender address unset");
    let match_client = |row: &ConnectedClient| {
        &row.identity == sender_identity && &row.address == sender_address
    };
    if let Some(client) = ctx.db.connected_client().iter().find(match_client) {
        ctx.db.connected_client().delete(client);
    }
}

#[spacetimedb::reducer]
fn print_num_connected(ctx: &ReducerContext) {
    let n = ctx.db.connected_client().count();
    info!("CONNECTED CLIENTS: {n}")
}
"""

    def test_restart_disconnects(self):
        """Tests if clients are automatically disconnected after a restart"""

        # Start two subscribers
        self.subscribe("SELECT * FROM connected_client", n=2)
        self.subscribe("SELECT * FROM connected_client", n=2)

        # Assert that we have two clients + the reducer call
        self.call("print_num_connected")
        logs = self.logs(10)
        self.assertEqual("CONNECTED CLIENTS: 3", logs.pop())

        restart_docker()

        # After restart, only the current call should be connected
        self.call("print_num_connected")
        logs = self.logs(10)
        self.assertEqual("CONNECTED CLIENTS: 1", logs.pop())

@requires_docker
class AddRemoveIndexAfterRestart(AddRemoveIndex):
    """
        `AddRemoveIndex` from `add_remove_index.py`,
        but restarts docker between each publish.

        This detects a bug we once had, hopefully fixed now,
        where the system autoinc sequences were borked after restart,
        leading newly-created database objects to re-use IDs.

        First publish the module without the indices,
        then restart docker, then add the indices and publish.
        Then restart docker, and publish again.
        There should be no errors from publishing,
        and the unindexed versions should reject subscriptions.
    """
    def between_publishes(self):
        restart_docker()
