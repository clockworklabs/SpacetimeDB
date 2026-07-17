use spacetimedb::{reducer, table, view, AnonymousViewContext, ReducerContext, Table};
#[table(accessor = content, public)]
pub struct Content { #[primary_key] pub id: u64, #[index(btree)] pub category: String, pub active: bool, pub score: i32 }
#[reducer]
pub fn seed(ctx: &ReducerContext) {
    for row in [
        Content { id: 1, category: "news".into(), active: true, score: 20 },
        Content { id: 2, category: "news".into(), active: false, score: 20 },
        Content { id: 3, category: "news".into(), active: true, score: 5 },
        Content { id: 4, category: "sports".into(), active: true, score: 20 },
    ] { ctx.db.content().insert(row); }
}
#[view(accessor = featured_content, public)]
pub fn featured_content(ctx: &AnonymousViewContext) -> Vec<Content> {
    ctx.db.content().category().filter(&"news".to_string()).filter(|row| row.active && row.score >= 10).collect()
}
