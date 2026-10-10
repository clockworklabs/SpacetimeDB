use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = stock, public)]
pub struct Stock {
    #[primary_key]
    pub id: u64,
    pub available: i64,
}

#[table(accessor = reservation, public)]
pub struct Reservation {
    #[primary_key]
    pub request_id: String,
    pub first_id: u64,
    pub first_qty: i64,
    pub second_id: u64,
    pub second_qty: i64,
}

#[reducer]
pub fn add_stock(ctx: &ReducerContext, id: u64, available: i64) -> Result<(), String> {
    if available < 0 {
        return Err("invalid stock".into());
    }
    ctx.db.stock().insert(Stock { id, available });
    Ok(())
}

#[reducer]
pub fn reserve(
    ctx: &ReducerContext,
    request_id: String,
    first_id: u64,
    first_qty: i64,
    second_id: u64,
    second_qty: i64,
) -> Result<(), String> {
    if let Some(old) = ctx.db.reservation().request_id().find(&request_id) {
        if (old.first_id, old.first_qty, old.second_id, old.second_qty) != (first_id, first_qty, second_id, second_qty)
        {
            return Err("request conflict".into());
        }
        return Ok(());
    }
    if request_id.is_empty() || first_id == second_id || first_qty <= 0 || second_qty <= 0 {
        return Err("invalid reservation".into());
    }
    let mut first = ctx.db.stock().id().find(first_id).ok_or("missing product")?;
    let mut second = ctx.db.stock().id().find(second_id).ok_or("missing product")?;
    if first.available < first_qty || second.available < second_qty {
        return Err("insufficient stock".into());
    }
    first.available -= first_qty;
    second.available -= second_qty;
    ctx.db.stock().id().update(first);
    ctx.db.stock().id().update(second);
    ctx.db.reservation().insert(Reservation {
        request_id,
        first_id,
        first_qty,
        second_id,
        second_qty,
    });
    Ok(())
}
