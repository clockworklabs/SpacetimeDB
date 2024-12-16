from .. import Smoketest, random_string
from subprocess import CalledProcessError
import time
import itertools

class UpdateModule(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
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
    MODULE_CODE_B = """
#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    age: u8,
}
"""

    MODULE_CODE_C = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::table(name = pets)]
pub struct Pet {
    species: String,
}

#[spacetimedb::reducer]
pub fn are_we_updated_yet(ctx: &ReducerContext) {
    log::info!("MODULE UPDATED");
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

        # Adding a table is ok
        self.write_module_code(self.MODULE_CODE_C)
        self.publish_module(name, clear=False)
        self.call("are_we_updated_yet")
        self.assertIn("MODULE UPDATED", self.logs(2))


class UploadModule1(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
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
use spacetimedb::{log, duration, ReducerContext, Table, Timestamp};


#[spacetimedb::table(name = scheduled_message, public, scheduled(my_repeating_reducer))]
pub struct ScheduledMessage {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    #[scheduled_at]
    scheduled_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    ctx.db.scheduled_message().insert(ScheduledMessage { prev: Timestamp::now(), scheduled_id: 0, scheduled_at: duration!(100ms).into(), });
}

#[spacetimedb::reducer]
pub fn my_repeating_reducer(_ctx: &ReducerContext, arg: ScheduledMessage) {
    log::info!("Invoked: ts={:?}, delta={:?}", Timestamp::now(), arg.prev.elapsed());
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
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}
"""

    MODULE_CODE_B = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::table(name = pet)]
pub struct Pet {
    #[primary_key]
    species: String,
}

#[spacetimedb::reducer]
pub fn add_pet(ctx: &ReducerContext, species: String) {
    ctx.db.pet().insert(Pet { species });
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
            {'person': {'deletes': [], 'inserts': [{'id': 1, 'name': 'Horst'}]}},
            {'person': {'deletes': [], 'inserts': [{'id': 2, 'name': 'Cindy'}]}}
        ])
