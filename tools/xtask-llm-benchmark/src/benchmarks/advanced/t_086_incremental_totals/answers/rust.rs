use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = sale, public)]
pub struct Sale {
    #[primary_key]
    pub id: u64,
    pub category: String,
    pub amount: i64,
}

#[table(accessor = category_total, public)]
pub struct CategoryTotal {
    #[primary_key]
    pub category: String,
    pub total_amount: i64,
    pub sale_count: u64,
}

fn adjust(ctx: &ReducerContext, category: String, amount: i64, adding: bool) {
    let old = ctx.db.category_total().category().find(&category);
    let mut total = old.unwrap_or(CategoryTotal {
        category: category.clone(),
        total_amount: 0,
        sale_count: 0,
    });
    if adding {
        total.total_amount += amount;
        total.sale_count += 1;
    } else {
        total.total_amount -= amount;
        total.sale_count -= 1;
    }
    ctx.db.category_total().category().delete(&category);
    if total.sale_count > 0 {
        ctx.db.category_total().insert(total);
    }
}
#[reducer]
pub fn set_sale(ctx: &ReducerContext, id: u64, category: String, amount: i64) -> Result<(), String> {
    if category.is_empty() {
        return Err("invalid category".into());
    }
    if let Some(old) = ctx.db.sale().id().find(id) {
        adjust(ctx, old.category, old.amount, false);
        ctx.db.sale().id().delete(id);
    }
    ctx.db.sale().insert(Sale {
        id,
        category: category.clone(),
        amount,
    });
    adjust(ctx, category, amount, true);
    Ok(())
}

#[reducer]
pub fn remove_sale(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if let Some(old) = ctx.db.sale().id().find(id) {
        ctx.db.sale().id().delete(id);
        adjust(ctx, old.category, old.amount, false);
    }
    Ok(())
}
