use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = quota, public)]
pub struct Quota {
    #[primary_key]
    pub owner: spacetimedb::Identity,
    pub used: u64,
}

#[table(accessor = accepted_request, public)]
pub struct AcceptedRequest {
    #[primary_key]
    pub request_id: String,
    pub owner: spacetimedb::Identity,
    pub units: u64,
}

#[reducer]
pub fn submit(ctx: &ReducerContext, request_id: String, units: i64) -> Result<(), String> {
    let owner = ctx.sender();
    if let Some(old) = ctx.db.accepted_request().request_id().find(&request_id) {
        if old.owner == owner && units > 0 && old.units == units as u64 {
            return Ok(());
        }
        return Err("request conflict".into());
    }
    if request_id.is_empty() || units <= 0 {
        return Err("invalid request".into());
    }
    let used = ctx.db.quota().owner().find(owner).map_or(0, |r| r.used);
    if units as u64 > 3 - used {
        return Err("quota exceeded".into());
    }
    let next = Quota {
        owner,
        used: used + units as u64,
    };
    if ctx.db.quota().owner().find(owner).is_some() {
        ctx.db.quota().owner().update(next);
    } else {
        ctx.db.quota().insert(next);
    }
    ctx.db.accepted_request().insert(AcceptedRequest {
        request_id,
        owner,
        units: units as u64,
    });
    Ok(())
}
