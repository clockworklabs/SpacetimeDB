from .. import Smoketest, run_cmd, requires_docker
from urllib.request import urlopen, URLError

def restart_docker():
    # Behold!
    #
    # You thought stop/start restarts? How wrong. Restart restarts.
    run_cmd("docker", "compose", "restart")
    # The suspense!
    #
    # Wait until compose believes the health probe succeeds.
    #
    # The container may decide to recompile, or grab a coffee at crates.io, or
    # whatever. In any case, restart doesn't mean the server is up yet.
    run_cmd("docker", "compose", "up", "--no-recreate", "--detach", "--wait-timeout", "60")
    # Belts and suspenders!
    #
    # The health probe runs inside the container, but that doesn't mean we can
    # reach it from outside. Ping until we get through.
    ping()

def ping():
    tries = 0
    host = "127.0.0.1:3000"
    while tries < 5:
        tries += 1
        try:
            urlopen(f"http://{host}/database/ping")
            break
        except URLError:
            print("Server down")
    else:
        raise Exception(f"Server at {host} not responding")
    print(f"Server up after {tries} try")


@requires_docker
class DockerRestartModule(Smoketest):
    # Note: creating indexes on `Person`
    # exercises more possible failure cases when replaying after restart
    MODULE_CODE = """
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "name_idx", name))]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u32,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
Person::insert(Person { id: 0, name }).unwrap();
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
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
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "name_idx", name))]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u32,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
Person::insert(Person { id: 0, name }).unwrap();
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
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

        sql_out = self.spacetime("sql", self.address, "SELECT name FROM Person WHERE id = 3")
        self.assertMultiLineEqual(sql_out, """ name       \n------------\n "Samantha" \n""")
