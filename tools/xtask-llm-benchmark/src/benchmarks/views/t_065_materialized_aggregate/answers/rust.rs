use spacetimedb::{reducer, table, view, Query, ReducerContext, Table, ViewContext};

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

fn add_to_total(ctx: &ReducerContext, category: &String, amount: i64) {
    if let Some(mut total) = ctx.db.category_total().category().find(category) {
        total.total_amount += amount;
        total.sale_count += 1;
        ctx.db.category_total().category().update(total);
    } else {
        ctx.db.category_total().insert(CategoryTotal {
            category: category.clone(),
            total_amount: amount,
            sale_count: 1,
        });
    }
}

fn remove_from_total(ctx: &ReducerContext, category: &String, amount: i64) {
    let mut total = ctx
        .db
        .category_total()
        .category()
        .find(category)
        .expect("missing category total");
    if total.sale_count == 1 {
        ctx.db.category_total().category().delete(category);
    } else {
        total.total_amount -= amount;
        total.sale_count -= 1;
        ctx.db.category_total().category().update(total);
    }
}

fn upsert_sale(ctx: &ReducerContext, sale: Sale) {
    let id = sale.id;
    let category = sale.category.clone();
    let amount = sale.amount;
    if let Some(old) = ctx.db.sale().id().find(sale.id) {
        remove_from_total(ctx, &old.category, old.amount);
        ctx.db.sale().id().update(sale);
    } else {
        ctx.db.sale().insert(sale);
    }
    debug_assert!(ctx.db.sale().id().find(id).is_some());
    add_to_total(ctx, &category, amount);
}

fn delete_sale(ctx: &ReducerContext, id: u64) {
    if let Some(old) = ctx.db.sale().id().find(id) {
        ctx.db.sale().id().delete(id);
        remove_from_total(ctx, &old.category, old.amount);
    }
}

#[reducer]
pub fn exercise(ctx: &ReducerContext) {
    upsert_sale(
        ctx,
        Sale {
            id: 1,
            category: "books".into(),
            amount: 10,
        },
    );
    upsert_sale(
        ctx,
        Sale {
            id: 2,
            category: "books".into(),
            amount: 20,
        },
    );
    upsert_sale(
        ctx,
        Sale {
            id: 2,
            category: "books".into(),
            amount: 25,
        },
    );
    upsert_sale(
        ctx,
        Sale {
            id: 3,
            category: "games".into(),
            amount: 40,
        },
    );
    delete_sale(ctx, 3);
    delete_sale(ctx, 1);
}

#[view(accessor = category_summary, public)]
pub fn category_summary(ctx: &ViewContext) -> impl Query<CategoryTotal> {
    ctx.from.category_total()
}
