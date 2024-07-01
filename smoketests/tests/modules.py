from .. import Smoketest, random_string
from subprocess import CalledProcessError
import time
import itertools

class UpdateModule(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u64,
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
    MODULE_CODE_B = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
    age: u8,
}
"""

    MODULE_CODE_C = """
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

#[spacetimedb(table)]
pub struct Pet {
    species: String,
}

#[spacetimedb(update)]
pub fn on_module_update() {
    println!("MODULE UPDATED");
}
"""


    def test_module_update(self):
        """Test publishing a module without the --clear-database option"""

        name = random_string()

        self.publish_module(name, clear=False)

        self.call("add", "Robert")
        self.call("add", "Julie")
        self.call("add", "Samantha")
        self.call("say_hello")
        logs = self.logs(100)
        self.assertIn("Hello, Samantha!", logs)
        self.assertIn("Hello, Julie!", logs)
        self.assertIn("Hello, Robert!", logs)
        self.assertIn("Hello, World!", logs)

        # Unchanged module is ok
        self.publish_module(name, clear=False)

        # Changing an existing table isn't
        self.write_module_code(self.MODULE_CODE_B)
        with self.assertRaises(CalledProcessError) as cm:
            self.publish_module(name, clear=False)
        self.assertIn("Error: Database update rejected", cm.exception.stderr)

        # Check that the old module is still running by calling say_hello
        self.call("say_hello")

        # Adding a table is ok, and invokes update
        self.write_module_code(self.MODULE_CODE_C)
        self.publish_module(name, clear=False)
        self.assertIn("MODULE UPDATED", self.logs(2))


class UploadModule1(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn say_hello() {
    for person in Person::iter() {
        println!("Hello, {}!", person.name);
    }
    println!("Hello, World!");
}
"""

    def test_upload_module_1(self):
        """This tests uploading a basic module and calling some functions and checking logs afterwards."""

        self.call("add", "Robert")
        self.call("add", "Julie")
        self.call("add", "Samantha")
        self.call("say_hello")
        logs = self.logs(100)
        self.assertIn("Hello, Samantha!", logs)
        self.assertIn("Hello, Julie!", logs)
        self.assertIn("Hello, Robert!", logs)
        self.assertIn("Hello, World!", logs)


class UploadModule2(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, spacetimedb, Timestamp};

#[spacetimedb(init)]
fn init() {
    spacetimedb::schedule!("100ms", my_repeating_reducer(Timestamp::now()));
}

#[spacetimedb(reducer)]
pub fn my_repeating_reducer(prev: Timestamp) {
  println!("Invoked: ts={:?}, delta={:?}", Timestamp::now(), prev.elapsed());
    spacetimedb::schedule!("100ms", my_repeating_reducer(Timestamp::now()));
}
"""
    def test_upload_module_2(self):
        """This test deploys a module with a repeating reducer and checks the logs to make sure its running."""

        time.sleep(2)
        lines = sum(1 for line in self.logs(100) if "Invoked" in line)
        time.sleep(4)
        new_lines = sum(1 for line in self.logs(100) if "Invoked" in line)
        self.assertLess(lines, new_lines)


class HotswapModule(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add_person(name: String) {
    Person::insert(Person { id: 0, name }).ok();
}
"""

    MODULE_CODE_B = """
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
pub struct Person {
    #[primarykey]
    #[autoinc]
    id: u64,
    name: String,
}

#[spacetimedb(reducer)]
pub fn add_person(name: String) {
    Person::insert(Person { id: 0, name }).ok();
}

#[spacetimedb(table)]
pub struct Pet {
    #[primarykey]
    species: String,
}

#[spacetimedb(reducer)]
pub fn add_pet(species: String) {
    Pet::insert(Pet { species }).ok();
}
"""

    def test_hotswap_module(self):
        """Tests hotswapping of modules."""

        # Publish MODULE_CODE and subscribe to all
        name = random_string()
        self.publish_module(name, clear=False)
        sub = self.subscribe("SELECT * FROM *", n=2)

        # Trigger event on the subscription
        self.call("add_person", "Horst")

        # Update the module
        self.write_module_code(self.MODULE_CODE_B)
        self.publish_module(name, clear=False)

        # Assert that the module was updated
        self.call("add_pet", "Turtle")
        # And trigger another event on the subscription
        self.call("add_person", "Cindy")

        # Note that 'SELECT * FROM *' does NOT get refreshed to include the
        # new table (this is a known limitation).
        self.assertEqual(sub(), [
            {'Person': {'deletes': [], 'inserts': [{'id': 1, 'name': 'Horst'}]}},
            {'Person': {'deletes': [], 'inserts': [{'id': 2, 'name': 'Cindy'}]}}
        ])
