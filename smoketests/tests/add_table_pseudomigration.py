from .. import Smoketest
import sys
import logging


class AddTablePseudomigration(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, ReducerContext, Table};

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
        println!("{}: {}", prefix, person.name);
    }
}
"""

    MODULE_CODE_UPDATED = (
        MODULE_CODE
        + """
#[spacetimedb::table(name = book)]
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
        println!("{}: {}", prefix, book.isbn);
    }
}
"""
    )

    def test_add_table_pseudomigration(self):
        """This tests uploading a module with a schema change that should not require clearing the database."""

        logging.info("Initial publish complete")
        # initial module code is already published by test framework

        self.call("add_person", "Robert")
        self.call("add_person", "Julie")
        self.call("add_person", "Samantha")
        self.call("print_persons", "BEFORE")
        logs = self.logs(100)
        self.assertIn("BEFORE: Samantha", logs)
        self.assertIn("BEFORE: Julie", logs)
        self.assertIn("BEFORE: Robert", logs)

        logging.info(
            "Initial operations complete, updating module without clear",
        )

        self.write_module_code(self.MODULE_CODE_UPDATED)
        self.publish_module(self.address, clear=False)

        logging.info("Updated")
        self.logs(100)

        self.call("add_person", "Husserl")
        self.call("add_book", "1234567890")
        self.call("print_persons", "AFTER_PERSON")
        self.call("print_books", "AFTER_BOOK")

        logs = self.logs(100)
        self.assertIn("AFTER_PERSON: Samantha", logs)
        self.assertIn("AFTER_PERSON: Julie", logs)
        self.assertIn("AFTER_PERSON: Robert", logs)
        self.assertIn("AFTER_PERSON: Husserl", logs)
        self.assertIn("AFTER_BOOK: 1234567890", logs)


class RejectTableChanges(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, ReducerContext, Table};

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
        println!("{}: {}", prefix, person.name);
    }
}
"""

    MODULE_CODE_UPDATED = """
use spacetimedb::{println, ReducerContext, Table};

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
        println!("{}: {}", prefix, person.name);
    }
}
"""

    def test_reject_schema_changes(self):
        """This tests that a module with invalid schema changes cannot be published without -c or a migration."""

        logging.info("Initial publish complete, trying to do an invalid update.")

        with self.assertRaises(Exception):
            self.write_module_code(self.MODULE_CODE_UPDATED)
            self.publish_module(self.address, clear=False)

        logging.info("Rejected as expected.")
