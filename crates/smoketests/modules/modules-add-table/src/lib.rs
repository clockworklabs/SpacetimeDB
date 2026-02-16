use spacetimedb::{log, ReducerContext};

#[spacetimedb::table(accessor = person)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::table(accessor = pets)]
pub struct Pet {
    species: String,
}

#[spacetimedb::reducer]
pub fn are_we_updated_yet(_ctx: &ReducerContext) {
    log::info!("MODULE UPDATED");
}
