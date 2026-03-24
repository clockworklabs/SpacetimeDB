use spacetimedb::{table, view, AnonymousViewContext};

#[table(accessor = announcement, public)]
pub struct Announcement {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub message: String,
    #[index(btree)]
    pub active: bool,
}

#[view(accessor = active_announcements, public)]
fn active_announcements(ctx: &AnonymousViewContext) -> Vec<Announcement> {
    ctx.db.announcement().active().filter(true).collect()
}
