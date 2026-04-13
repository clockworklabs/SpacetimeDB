use spacetimedb::{log, Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = person)]
pub struct Person {
    #[unique]
    id: i32,

    name: String,

    #[unique]
    nick: String,
}

#[spacetimedb::reducer]
pub fn insert_person(ctx: &ReducerContext, id: i32, name: String, nick: String) {
    ctx.db.person().insert(Person { id, name, nick });
}

#[spacetimedb::reducer]
pub fn insert_person_twice(ctx: &ReducerContext, id: i32, name: String, nick: String) {
    // We'd like to avoid an error due to a set-semantic error.
    let name2 = format!("{name}2");
    ctx.db.person().insert(Person {
        id,
        name,
        nick: nick.clone(),
    });
    match ctx.db.person().try_insert(Person {
        id,
        name: name2,
        nick: nick.clone(),
    }) {
        Ok(_) => {}
        Err(_) => {
            log::info!("UNIQUE CONSTRAINT VIOLATION ERROR: id = {}, nick = {}", id, nick)
        }
    }
}

#[spacetimedb::reducer]
pub fn delete_person(ctx: &ReducerContext, id: i32) {
    ctx.db.person().id().delete(&id);
}

#[spacetimedb::reducer]
pub fn find_person(ctx: &ReducerContext, id: i32) {
    match ctx.db.person().id().find(&id) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", id, person.name),
        None => log::info!("UNIQUE NOT FOUND: id {}", id),
    }
}

#[spacetimedb::reducer]
pub fn find_person_read_only(ctx: &ReducerContext, id: i32) {
    let ctx = ctx.as_read_only();
    match ctx.db.person().id().find(&id) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", id, person.name),
        None => log::info!("UNIQUE NOT FOUND: id {}", id),
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_name(ctx: &ReducerContext, name: String) {
    for person in ctx.db.person().iter().filter(|p| p.name == name) {
        log::info!("UNIQUE FOUND: id {}: {} aka {}", person.id, person.name, person.nick);
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_nick(ctx: &ReducerContext, nick: String) {
    match ctx.db.person().nick().find(&nick) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", person.id, person.nick),
        None => log::info!("UNIQUE NOT FOUND: nick {}", nick),
    }
}

#[spacetimedb::reducer]
pub fn find_person_by_nick_read_only(ctx: &ReducerContext, nick: String) {
    let ctx = ctx.as_read_only();
    match ctx.db.person().nick().find(&nick) {
        Some(person) => log::info!("UNIQUE FOUND: id {}: {}", person.id, person.nick),
        None => log::info!("UNIQUE NOT FOUND: nick {}", nick),
    }
}

#[spacetimedb::table(accessor = nonunique_person)]
pub struct NonuniquePerson {
    #[index(btree)]
    id: i32,
    name: String,
    is_human: bool,
}

#[spacetimedb::reducer]
pub fn insert_nonunique_person(ctx: &ReducerContext, id: i32, name: String, is_human: bool) {
    ctx.db.nonunique_person().insert(NonuniquePerson { id, name, is_human });
}

#[spacetimedb::reducer]
pub fn find_nonunique_person(ctx: &ReducerContext, id: i32) {
    for person in ctx.db.nonunique_person().id().filter(&id) {
        log::info!("NONUNIQUE FOUND: id {}: {}", id, person.name)
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_person_read_only(ctx: &ReducerContext, id: i32) {
    let ctx = ctx.as_read_only();
    for person in ctx.db.nonunique_person().id().filter(&id) {
        log::info!("NONUNIQUE FOUND: id {}: {}", id, person.name)
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_humans(ctx: &ReducerContext) {
    for person in ctx.db.nonunique_person().iter().filter(|p| p.is_human) {
        log::info!("HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

#[spacetimedb::reducer]
pub fn find_nonunique_non_humans(ctx: &ReducerContext) {
    for person in ctx.db.nonunique_person().iter().filter(|p| !p.is_human) {
        log::info!("NON-HUMAN FOUND: id {}: {}", person.id, person.name);
    }
}

// Ensure that [Identity] is filterable and a legal unique column.
#[spacetimedb::table(accessor = identified_person)]
struct IdentifiedPerson {
    #[unique]
    identity: Identity,
    name: String,
}

fn identify(id_number: u64) -> Identity {
    let mut bytes = [0u8; 32];
    bytes[..8].clone_from_slice(&id_number.to_le_bytes());
    Identity::from_byte_array(bytes)
}

#[spacetimedb::reducer]
fn insert_identified_person(ctx: &ReducerContext, id_number: u64, name: String) {
    let identity = identify(id_number);
    ctx.db.identified_person().insert(IdentifiedPerson { identity, name });
}

#[spacetimedb::reducer]
fn find_identified_person(ctx: &ReducerContext, id_number: u64) {
    let identity = identify(id_number);
    match ctx.db.identified_person().identity().find(&identity) {
        Some(person) => log::info!("IDENTIFIED FOUND: {}", person.name),
        None => log::info!("IDENTIFIED NOT FOUND"),
    }
}

// Ensure that indices on non-unique columns behave as we expect.
#[spacetimedb::table(accessor = indexed_person)]
struct IndexedPerson {
    #[unique]
    id: i32,
    given_name: String,
    #[index(btree)]
    surname: String,
}

#[spacetimedb::reducer]
fn insert_indexed_person(ctx: &ReducerContext, id: i32, given_name: String, surname: String) {
    ctx.db.indexed_person().insert(IndexedPerson {
        id,
        given_name,
        surname,
    });
}

#[spacetimedb::reducer]
fn delete_indexed_person(ctx: &ReducerContext, id: i32) {
    ctx.db.indexed_person().id().delete(&id);
}

#[spacetimedb::reducer]
fn find_indexed_people(ctx: &ReducerContext, surname: String) {
    for person in ctx.db.indexed_person().surname().filter(&surname) {
        log::info!(
            "INDEXED FOUND: id {}: {}, {}",
            person.id,
            person.surname,
            person.given_name
        );
    }
}

#[spacetimedb::reducer]
fn find_indexed_people_read_only(ctx: &ReducerContext, surname: String) {
    let ctx = ctx.as_read_only();
    for person in ctx.db.indexed_person().surname().filter(&surname) {
        log::info!(
            "INDEXED FOUND: id {}: {}, {}",
            person.id,
            person.surname,
            person.given_name
        );
    }
}
