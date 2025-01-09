from .. import Smoketest
import sys
import logging
import re

# 7-bit C1 ANSI sequences
ansi_escape = re.compile(
    r"""
    \x1B  # ESC
    (?:   # 7-bit C1 Fe (except CSI)
        [@-Z\\-_]
    |     # or [ for CSI, followed by a control sequence
        \[
        [0-?]*  # Parameter bytes
        [ -/]*  # Intermediate bytes
        [@-~]   # Final byte
    )
""",
    re.VERBOSE,
)


def strip_ansi_escape_codes(text: str) -> str:
    return ansi_escape.sub("", text)


class AddTableAutoMigration(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext, Table, SpacetimeType};

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

#[spacetimedb::table(name = point_mass)]
#[index(name = point_masses_by_mass, btree(columns = position))]
pub struct PointMass {
    #[primary_key]
    #[auto_inc]
    id: u64,
    mass: f64,
    /// This used to cause an error when check_compatible did not resolve types in a `ModuleDef`.
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(name = scheduled_table, scheduled(send_scheduled_message), public)]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    text: String,
}

#[spacetimedb::reducer]
fn send_scheduled_message(_ctx: &ReducerContext, arg: ScheduledTable) {
    let _ = arg.text;
    let _ = arg.scheduled_at;
    let _ = arg.scheduled_id;
}

spacetimedb::filter!("SELECT * FROM person");
"""

    MODULE_CODE_UPDATED = """
use spacetimedb::{log, ReducerContext, Table, SpacetimeType};

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

#[spacetimedb::table(name = point_mass, public)] // private -> public
// remove index
pub struct PointMass {
    // remove primary_key and auto_inc
    id: u64,
    mass: f64,
    /// This used to cause an error when check_compatible did not resolve types in a `ModuleDef`.
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

// TODO: once removing schedules is implemented, remove the schedule here.
#[spacetimedb::table(name = scheduled_table, scheduled(send_scheduled_message), public)]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    text: String,
}

#[spacetimedb::reducer]
fn send_scheduled_message(_ctx: &ReducerContext, arg: ScheduledTable) {
    let _ = arg.text;
    let _ = arg.scheduled_at;
    let _ = arg.scheduled_id;
}

spacetimedb::filter!("SELECT * FROM person");

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
        log::info!("{}: {}", prefix, book.isbn);
    }
}

spacetimedb::filter!("SELECT * FROM book");

#[spacetimedb::table(name = parabolas)]
#[index(name = parabolas_by_b_c, btree(columns = [b, c]))]
pub struct Parabola {
    #[primary_key]
    #[auto_inc]
    id: u64,
    a: f64,
    b: f64,
    c: f64,
}
"""

    EXPECTED_MIGRATION_REPORT = """--------------
Performed automatic migration
--------------
- Removed index `point_mass_id_idx_btree` on columns [`id`] of table `point_mass`
- Removed unique constraint `point_mass_id_key` on columns [`id`] of table `point_mass`
- Removed auto-increment constraint `point_mass_id_seq` on column `id` of table `point_mass`
- Created table: `book` (private)
    - Columns:
        - `isbn`: String
- Created table: `parabolas` (private)
    - Columns:
        - `id`: U64
        - `a`: F64
        - `b`: F64
        - `c`: F64
    - Unique constraints:
        - `parabolas_id_key` on [`id`]
    - Indexes:
        - `parabolas_id_idx_btree` on [`id`]
    - Auto-increment constraints:
        - `parabolas_id_seq` on `id`
- Created row level security policy:
    `SELECT * FROM book`
- Changed access for table `point_mass` (private -> public)"""

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
        output = self.publish_module(self.database_identity, clear=False)
        output = strip_ansi_escape_codes(output)

        print("got output\n", output)

        # Remark: if this test ever fails mysteriously,
        # try double-checking the pretty printing code for trailing spaces before newlines.
        # Also make sure the pretty-printing is deterministic.
        self.assertIn(self.EXPECTED_MIGRATION_REPORT, output)

        logging.info("Updated")

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
