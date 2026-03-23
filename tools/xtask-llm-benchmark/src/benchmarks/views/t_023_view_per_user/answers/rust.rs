use spacetimedb::{table, view, ViewContext};
use spacetimedb_lib::Identity;

#[table(accessor = profile, public)]
pub struct Profile {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub identity: Identity,
    pub name: String,
    pub bio: String,
}

#[view(accessor = my_profile, public)]
fn my_profile(ctx: &ViewContext) -> Option<Profile> {
    ctx.db.profile().identity().find(ctx.sender())
}
