use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = account, public)]
pub struct Account {
    #[primary_key]
    pub id: u64,
    pub balance: i64,
}

#[table(accessor = transfer_request, public)]
pub struct TransferRequest {
    #[primary_key]
    pub request_id: String,
    pub from_id: u64,
    pub to_id: u64,
    pub amount: i64,
}

#[reducer]
pub fn create_account(ctx: &ReducerContext, id: u64, balance: i64) {
    ctx.db.account().insert(Account { id, balance });
}

#[reducer]
pub fn transfer(ctx: &ReducerContext, request_id: String, from_id: u64, to_id: u64, amount: i64) -> Result<(), String> {
    if ctx.db.transfer_request().request_id().find(&request_id).is_some() {
        return Ok(());
    }
    if amount <= 0 || from_id == to_id {
        return Err("invalid transfer".into());
    }
    let mut from = ctx.db.account().id().find(from_id).ok_or("source account not found")?;
    let mut to = ctx.db.account().id().find(to_id).ok_or("destination account not found")?;
    if from.balance < amount {
        return Err("insufficient balance".into());
    }
    from.balance -= amount;
    to.balance += amount;
    ctx.db.account().id().update(from);
    ctx.db.account().id().update(to);
    ctx.db.transfer_request().insert(TransferRequest {
        request_id,
        from_id,
        to_id,
        amount,
    });
    Ok(())
}
