use spacetimedb::{reducer, table, Identity, ReducerContext, Table};
#[table(accessor = seat_event, public)]
pub struct SeatEvent {
    #[primary_key]
    pub id: u64,
    pub capacity: u32,
}
#[table(accessor = booking, public, index(accessor = by_owner_event, btree(columns = [owner, event_id])))]
pub struct Booking {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    #[index(btree)]
    pub event_id: u64,
    pub status: String,
}
#[reducer]
pub fn create_event(ctx: &ReducerContext, id: u64, capacity: u32) -> Result<(), String> {
    if capacity == 0 {
        return Err("invalid capacity".into());
    }
    if ctx.db.seat_event().id().find(id).is_some() {
        return Err("event exists".into());
    }
    ctx.db.seat_event().insert(SeatEvent { id, capacity });
    Ok(())
}
#[reducer]
pub fn join_event(ctx: &ReducerContext, event_id: u64) -> Result<(), String> {
    let event = ctx.db.seat_event().id().find(event_id).ok_or("missing event")?;
    if ctx
        .db
        .booking()
        .by_owner_event()
        .filter((ctx.sender(), event_id))
        .any(|b| b.status != "cancelled")
    {
        return Ok(());
    }
    let seated = ctx
        .db
        .booking()
        .event_id()
        .filter(event_id)
        .filter(|b| b.status == "seated")
        .count();
    ctx.db.booking().insert(Booking {
        id: 0,
        owner: ctx.sender(),
        event_id,
        status: if seated < event.capacity as usize {
            "seated"
        } else {
            "waiting"
        }
        .into(),
    });
    Ok(())
}
#[reducer]
pub fn cancel_booking(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let Some(mut row) = ctx.db.booking().id().find(id) else {
        return Ok(());
    };
    if row.owner != ctx.sender() {
        return Err("owner only".into());
    }
    if row.status == "cancelled" {
        return Ok(());
    }
    let promote = row.status == "seated";
    let event_id = row.event_id;
    row.status = "cancelled".into();
    ctx.db.booking().id().update(row);
    if promote {
        if let Some(mut next) = ctx
            .db
            .booking()
            .event_id()
            .filter(event_id)
            .filter(|b| b.status == "waiting")
            .min_by_key(|b| b.id)
        {
            next.status = "seated".into();
            ctx.db.booking().id().update(next);
        }
    }
    Ok(())
}
