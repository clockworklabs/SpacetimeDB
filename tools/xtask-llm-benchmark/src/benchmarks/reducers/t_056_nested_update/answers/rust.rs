use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType)]
pub struct Preferences {
    pub theme: String,
    pub email_notifications: bool,
    pub timezone: String,
}

#[table(accessor = profile, public)]
pub struct Profile {
    #[primary_key]
    pub id: u64,
    pub preferences: Preferences,
}

#[reducer]
pub fn create_profile(ctx: &ReducerContext, id: u64, theme: String, email_notifications: bool, timezone: String) {
    ctx.db.profile().insert(Profile {
        id,
        preferences: Preferences {
            theme,
            email_notifications,
            timezone,
        },
    });
}

#[reducer]
pub fn update_theme(ctx: &ReducerContext, id: u64, theme: String) -> Result<(), String> {
    let mut profile = ctx.db.profile().id().find(id).ok_or("profile not found")?;
    profile.preferences.theme = theme;
    ctx.db.profile().id().update(profile);
    Ok(())
}
