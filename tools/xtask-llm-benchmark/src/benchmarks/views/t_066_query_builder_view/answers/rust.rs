use spacetimedb::{reducer, table, view, Query, ReducerContext, Table, ViewContext};
#[table(accessor = ticket, public)]
pub struct Ticket { #[primary_key] pub id: u64, #[index(btree)] pub status: String, pub title: String }
#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.ticket().insert(Ticket { id: 1, status: "open".into(), title: "A".into() });
    ctx.db.ticket().insert(Ticket { id: 2, status: "closed".into(), title: "B".into() });
}
#[view(accessor = open_ticket, public)]
pub fn open_ticket(ctx: &ViewContext) -> impl Query<Ticket> { ctx.from.ticket().filter(|ticket| ticket.status.eq("open".to_string())) }
