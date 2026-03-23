use spacetimedb::{table, view, ViewContext, Identity, Query};

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
fn my_profile(ctx: &ViewContext) -> impl Query<Profile> {
    ctx.from
        .profile()
        .r#where(|p| p.identity.eq(ctx.sender()))
        .build()
}
