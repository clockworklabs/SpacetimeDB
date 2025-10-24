from .. import Smoketest
import sys
import logging


class AddTableAutoMigration(Smoketest):
    MODULE_CODE_INIT = """
use spacetimedb::{log, ReducerContext, Table, SpacetimeType};
use PersonKind::*;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
    kind: PersonKind,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String, kind: String) {
    let kind = kind_from_string(kind);
    ctx.db.person().insert(Person { name, kind });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        let kind = kind_to_string(person.kind);
        log::info!("{prefix}: {} - {kind}", person.name);
    }
}

#[spacetimedb::table(name = point_mass)]
pub struct PointMass {
    mass: f64,
    /// This used to cause an error when check_compatible did not resolve types in a `ModuleDef`.
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::client_visibility_filter]
const PERSON_VISIBLE: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM person");
"""

    MODULE_CODE = MODULE_CODE_INIT + """
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Student,
}

fn kind_from_string(_: String) -> PersonKind {
    Student
}

fn kind_to_string(Student: PersonKind) -> &'static str {
    "Student"
}
"""

    MODULE_CODE_UPDATED = (
        MODULE_CODE_INIT
        + """
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Student,
    Professor,
}

fn kind_from_string(kind: String) -> PersonKind {
    match &*kind {
        "Student" => Student,
        "Professor" => Professor,
        _ => panic!(),
    }
}

fn kind_to_string(kind: PersonKind) -> &'static str {
    match kind {
        Student => "Student",
        Professor => "Professor",
    }
}

#[spacetimedb::table(name = book, public)]
pub struct Book {
    isbn: String,
}

#[spacetimedb::reducer]
pub fn add_book(ctx: &ReducerContext, isbn: String) {
    ctx.db.book().insert(Book { isbn });
}

#[spacetimedb::reducer]
pub fn print_books(ctx: &ReducerContext, prefix: String) {
    for book in ctx.db.book().iter() {
        log::info!("{}: {}", prefix, book.isbn);
    }
}

#[spacetimedb::client_visibility_filter]
const BOOK_VISIBLE: spacetimedb::Filter = spacetimedb::Filter::Sql("SELECT * FROM book");
"""
    )

    def assertSql(self, sql, expected):
        self.maxDiff = None
        sql_out = self.spacetime("sql", self.database_identity, sql)
        sql_out = "\n".join([line.rstrip() for line in sql_out.splitlines()])
        expected = "\n".join([line.rstrip() for line in expected.splitlines()])
        self.assertMultiLineEqual(sql_out, expected)

    def test_add_table_auto_migration(self):
        """This tests uploading a module with a schema change that should not require clearing the database."""

        # Check the row-level SQL filter is created correctly
        self.assertSql(
            "SELECT sql FROM st_row_level_security",
            """\
 sql
------------------------
 "SELECT * FROM person"
""",
        )

        logging.info("Initial publish complete")

        # Start a subscription before publishing the module, to test that the subscription remains intact after re-publishing.
        sub = self.subscribe("select * from person", n=4)

        # initial module code is already published by test framework
        self.call("add_person", "Robert", "Student")
        self.call("add_person", "Julie", "Student")
        self.call("add_person", "Samantha", "Student")
        self.call("print_persons", "BEFORE")
        logs = self.logs(100)
        self.assertIn("BEFORE: Samantha - Student", logs)
        self.assertIn("BEFORE: Julie - Student", logs)
        self.assertIn("BEFORE: Robert - Student", logs)

        logging.info(
            "Initial operations complete, updating module without clear",
        )

        self.write_module_code(self.MODULE_CODE_UPDATED)
        self.publish_module(self.database_identity, clear=False)

        logging.info("Updated")
        self.call("add_person", "Husserl", "Student")

        # If subscription, we should get 4 rows corresponding to 4 reducer calls (including before and after update)
        sub = sub();
        self.assertEqual(len(sub), 4)

        # Check the row-level SQL filter is added correctly
        self.assertSql(
            "SELECT sql FROM st_row_level_security",
            """\
 sql
------------------------
 "SELECT * FROM person"
 "SELECT * FROM book"
""",
        )

        self.logs(100)

        self.call("add_person", "Husserl", "Professor")
        self.call("add_book", "1234567890")
        self.call("print_persons", "AFTER_PERSON")
        self.call("print_books", "AFTER_BOOK")

        logs = self.logs(100)
        self.assertIn("AFTER_PERSON: Samantha - Student", logs)
        self.assertIn("AFTER_PERSON: Julie - Student", logs)
        self.assertIn("AFTER_PERSON: Robert - Student", logs)
        self.assertIn("AFTER_PERSON: Husserl - Professor", logs)
        self.assertIn("AFTER_BOOK: 1234567890", logs)


class RejectTableChanges(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {}", prefix, person.name);
    }
}
"""

    MODULE_CODE_UPDATED = """
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
    age: u128,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {}", prefix, person.name);
    }
}
"""

    def test_reject_schema_changes(self):
        """This tests that a module with invalid schema changes cannot be published without -c or a migration."""

        logging.info("Initial publish complete, trying to do an invalid update.")

        with self.assertRaises(Exception):
            self.write_module_code(self.MODULE_CODE_UPDATED)
            self.publish_module(self.database_identity, clear=False)

        logging.info("Rejected as expected.")

class AddTableColumns(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {}", prefix, person.name);
    }
}
"""

    MODULE_UPDATED = """
use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(name = person)]
pub struct Person {
    // Add indexes to verify they are handled correctly during migration,
    // issue #3441
    #[index(btree)]
    name: String,
    #[default(0)]
    age: u16,
    #[default(19)]
    mass: u16,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70, mass: 180 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    log::info!("FIRST_UPDATE: client disconnected");
}
"""

    MODULE_UPDATED_AGAIN = """
use spacetimedb::{log, ReducerContext, Table};

#[derive(Debug)]
#[spacetimedb::table(name = person)]
pub struct Person {
    name: String,
    age: u16,
    #[default(19)]
    mass: u16,
    #[default(160)]
    height: u32,
}

#[spacetimedb::reducer]
pub fn add_person(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name, age: 70, mass: 180, height: 72 });
}

#[spacetimedb::reducer]
pub fn print_persons(ctx: &ReducerContext, prefix: String) {
    for person in ctx.db.person().iter() {
        log::info!("{}: {:?}", prefix, person);
    }
}
"""

    def test_add_table_columns(self):
        """Verify schema upgrades that add columns with defaults (twice)."""

        # Subscribe to person table changes multiple times to simulate active clients
        NUM_SUBSCRIBERS = 20
        subs = [None] * NUM_SUBSCRIBERS
        for i in range(NUM_SUBSCRIBERS):
            subs[i]= self.subscribe("select * from person", n=5)

        # Insert under initial schema
        self.call("add_person", "Robert")

        # First upgrade: add age & mass columns
        self.write_module_code(self.MODULE_UPDATED)
        self.publish_module(self.database_identity, clear=False, break_clients=True)
        self.call("print_persons", "FIRST_UPDATE")

        logs1 = self.logs(100)

        # Validate disconnect + schema migration logs
        self.assertIn("Disconnecting all users", logs1)
        self.assertIn(
            'FIRST_UPDATE: Person { name: "Robert", age: 0, mass: 19 }',
            logs1,
        )
        disconnect_count = logs1.count("FIRST_UPDATE: client disconnected")

        # Insert new data under upgraded schema
        self.call("add_person", "Robert2")

        self.assertEqual(
            disconnect_count,
        # +1 is due to reducer call above
            NUM_SUBSCRIBERS + 1, 
            msg=f"Unexpected disconnect counts: {disconnect_count}",
        )

        # Validate all subscribers received only single update before disconnect
        for i in range(NUM_SUBSCRIBERS):
            sub = subs[i]()
            self.assertEqual(len(sub), 1, msg=f"Subscriber {i} received unexpected rows: {sub}")


        # Second upgrade
        self.write_module_code(self.MODULE_UPDATED_AGAIN)
        self.publish_module(self.database_identity, clear=False, break_clients=True)
        self.call("print_persons", "UPDATE_2")

        logs2 = self.logs(100)

        # Validate new schema with height
        self.assertIn(
            'UPDATE_2: Person { name: "Robert2", age: 70, mass: 180, height: 160 }',
            logs2,
        )
