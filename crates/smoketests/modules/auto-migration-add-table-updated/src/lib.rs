use spacetimedb::{log, ReducerContext, SpacetimeType, Table};
use PersonKind::*;

#[spacetimedb::table(accessor = person, public)]
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

#[spacetimedb::table(accessor = point_mass)]
pub struct PointMass {
    mass: f64,
    position: Vector2,
}

#[derive(SpacetimeType, Clone, Copy)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

#[spacetimedb::table(accessor = person_info)]
pub struct PersonInfo {
    #[primary_key]
    #[auto_inc]
    id: u64,
}

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

#[spacetimedb::table(accessor = book, public)]
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
