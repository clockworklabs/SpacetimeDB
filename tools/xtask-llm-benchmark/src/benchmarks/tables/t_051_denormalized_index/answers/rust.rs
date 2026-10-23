use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = category, public)]
pub struct Category {
    #[primary_key]
    pub id: u64,
    pub slug: String,
}

#[table(
    accessor = product,
    public,
    index(accessor = by_category, btree(columns = [category_id])),
    index(accessor = by_category_slug, btree(columns = [category_slug]))
)]
pub struct Product {
    #[primary_key]
    pub id: u64,
    pub category_id: u64,
    pub category_slug: String,
    pub name: String,
}

#[reducer]
pub fn create_category(ctx: &ReducerContext, id: u64, slug: String) {
    ctx.db.category().insert(Category { id, slug });
}

#[reducer]
pub fn create_product(ctx: &ReducerContext, id: u64, category_id: u64, name: String) -> Result<(), String> {
    let category = ctx
        .db
        .category()
        .id()
        .find(category_id)
        .ok_or_else(|| "category not found".to_string())?;
    ctx.db.product().insert(Product {
        id,
        category_id,
        category_slug: category.slug,
        name,
    });
    Ok(())
}

#[reducer]
pub fn rename_category(ctx: &ReducerContext, id: u64, new_slug: String) -> Result<(), String> {
    let mut category = ctx
        .db
        .category()
        .id()
        .find(id)
        .ok_or_else(|| "category not found".to_string())?;
    category.slug = new_slug.clone();
    ctx.db.category().id().update(category);

    for mut product in ctx.db.product().by_category().filter(id) {
        product.category_slug = new_slug.clone();
        ctx.db.product().id().update(product);
    }
    Ok(())
}
