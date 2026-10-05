// This file tests proposal 0032's `#[name("...")]` on a field: the module def records an explicit name for the field,
// keyed by its type, once, and only for a field that has one.

use spacetimedb::sats::AlgebraicTypeRef;
use spacetimedb::spacetimedb_lib::db::raw_def::v10::{ExplicitNameEntry, RawModuleDefV10Builder};
use spacetimedb::SpacetimeType;

#[derive(SpacetimeType)]
pub struct Info {
    #[name("ageValue")]
    age_value: u8,
    other: u8,
}

#[spacetimedb::table(accessor = person)]
#[spacetimedb::table(accessor = logged_out_person)]
pub struct Person {
    #[primary_key]
    id: u64,
    #[name("playerRef")]
    #[index(btree)]
    player_ref: u32,
    info: Info,
}

#[derive(SpacetimeType)]
pub struct Plain {
    x: u32,
}

fn field_names(module: RawModuleDefV10Builder) -> Vec<(AlgebraicTypeRef, String, String)> {
    let module = module.finish();
    let entries = module.explicit_names().cloned().unwrap_or_default().into_entries();
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            ExplicitNameEntry::Field(field) => Some((
                field.ty,
                field.source_name.to_string(),
                field.canonical_name.to_string(),
            )),
            _ => None,
        })
        .collect()
}

fn main() {
    let mut module = RawModuleDefV10Builder::new();
    let person = Person::make_type(&mut module);
    let person_again = Person::make_type(&mut module);
    assert_eq!(person, person_again);
    let info = Info::make_type(&mut module);
    let (Some(person), Some(info)) = (person.as_ref(), info.as_ref()) else {
        panic!("expected refs");
    };
    assert_eq!(
        field_names(module),
        [
            (*info, "age_value".into(), "ageValue".into()),
            (*person, "player_ref".into(), "playerRef".into()),
        ]
    );

    // A type without `#[name]` adds no explicit names.
    let mut module = RawModuleDefV10Builder::new();
    Plain::make_type(&mut module);
    assert!(module.finish().explicit_names().is_none());
}
