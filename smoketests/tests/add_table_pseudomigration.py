from .. import Smoketest
import sys


class AddTablePseudomigration(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, spacetimedb};

#[spacetimedb(table)]
pub struct Person {
    name: String,
}

#[spacetimedb(reducer)]
pub fn add_person(name: String) {
    Person::insert(Person { name });
}

#[spacetimedb(reducer)]
pub fn print_persons(prefix: String) {
    for person in Person::iter() {
        println!("{}: {}", prefix, person.name);
    }
}
"""

    MODULE_CODE_UPDATED = (
        MODULE_CODE
        + """
#[spacetimedb(table)]
pub struct Book {
    isbn: String,
}
 
pub fn add_book(isbn: String) {
    Book::insert(Book { isbn });
}

pub fn print_books(prefix: String) {
    for book in Book::iter() {
        println!("{}: {}", prefix, book.isbn);
    }
}
"""
    )

    def test_upload_module_1(self):
        """This tests uploading a basic module and calling some functions and checking logs afterwards."""

        print("Initial publish complete", file=sys.stderr)
        # initial module code is already published by test framework

        self.call("add_person", "Robert")
        self.call("add_person", "Julie")
        self.call("add_person", "Samantha")
        self.call("print_persons", "BEFORE")
        logs = self.logs(100)
        self.assertIn("BEFORE: Samantha", logs)
        self.assertIn("BEFORE: Julie", logs)
        self.assertIn("BEFORE: Robert", logs)

        print(
            "Initial operations complete, updating module without clear",
            file=sys.stderr,
        )

        self.write_module_code(self.MODULE_CODE_UPDATED)
        self.publish_module(clear=False)

        print("Updated", file=sys.stderr)

        self.call("add_person", "Husserl")
        self.call("add_book", "1234567890")
        self.call("print_persons", "AFTER_PERSON")
        self.call("print_books", "AFTER_BOOK")

        logs = self.logs(100)
        self.assertIn("AFTER_PERSON: Samantha!", logs)
        self.assertIn("AFTER_PERSON: Julie!", logs)
        self.assertIn("AFTER_PERSON: Robert!", logs)
        self.assertIn("AFTER_PERSON: Husserl!", logs)
        self.assertIn("AFTER_BOOK: 1234567890", logs)
