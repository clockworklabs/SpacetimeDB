use log::info;
use spacetimedb::{reducer, ReducerContext, Table};

#[spacetimedb::table(name = accounts, public)]
#[derive(Debug, Clone)]
pub struct Accounts {
    #[primary_key]
    pub id: u32,
    pub balance: i64,
}

#[reducer]
pub fn seed(ctx: &ReducerContext, n: u32, initial_balance: i64) -> Result<(), String> {
    let accounts = ctx.db.accounts();

    //reset
    for row in accounts.iter() {
        accounts.delete(row);
    }

    //seed
    for id in 0..n {
        accounts.insert(Accounts {
            id,
            balance: initial_balance,
        });
    }

    info!("seeded {} accounts with balance {}", n, initial_balance);
    Ok(())
}

#[reducer]
pub fn create_account(ctx: &ReducerContext, id: u32, balance: i64) -> Result<(), String> {
    let accounts = ctx.db.accounts();
    let by_id = accounts.id();

    if let Some(mut row) = by_id.find(&id) {
        row.balance = balance;
        by_id.update(row);
    } else {
        accounts.insert(Accounts { id, balance });
    }
    Ok(())
}

#[reducer]
pub fn transfer(
    ctx: &ReducerContext,
    from: u32,
    to: u32,
    amount: i64,
    _client_txn_id: u64,
) -> Result<(), String> {
    if from == to {
        return Err("same_account".into());
    }
    if amount <= 0 {
        return Err("non_positive_amount".into());
    }

    let accounts = ctx.db.accounts();
    let by_id = accounts.id();

    let from_row = by_id.find(&from).ok_or("account_missing")?;
    let to_row = by_id.find(&to).ok_or("account_missing")?;

    if from_row.balance < amount {
        return Err("insufficient_funds".into());
    }

    by_id.update(Accounts {
        id: from,
        balance: from_row.balance - amount,
    });

    by_id.update(Accounts {
        id: to,
        balance: to_row.balance + amount,
    });

    Ok(())
}
