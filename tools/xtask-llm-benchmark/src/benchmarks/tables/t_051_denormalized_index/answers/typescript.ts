import { schema, table, t } from 'spacetimedb/server';

const category = table(
  { name: 'category', public: true },
  { id: t.u64().primaryKey(), slug: t.string() }
);

const product = table(
  {
    name: 'product',
    public: true,
    indexes: [
      { accessor: 'byCategory', algorithm: 'btree', columns: ['categoryId'] },
      { accessor: 'byCategorySlug', algorithm: 'btree', columns: ['categorySlug'] },
    ],
  },
  {
    id: t.u64().primaryKey(),
    categoryId: t.u64(),
    categorySlug: t.string(),
    name: t.string(),
  }
);

const spacetimedb = schema({ category, product });
export default spacetimedb;

export const create_category = spacetimedb.reducer(
  { id: t.u64(), slug: t.string() },
  (ctx, { id, slug }) => ctx.db.category.insert({ id, slug })
);

export const create_product = spacetimedb.reducer(
  { id: t.u64(), categoryId: t.u64(), name: t.string() },
  (ctx, { id, categoryId, name }) => {
    const found = ctx.db.category.id.find(categoryId);
    if (!found) throw new Error('category not found');
    ctx.db.product.insert({ id, categoryId, categorySlug: found.slug, name });
  }
);

export const rename_category = spacetimedb.reducer(
  { id: t.u64(), newSlug: t.string() },
  (ctx, { id, newSlug }) => {
    const found = ctx.db.category.id.find(id);
    if (!found) throw new Error('category not found');
    ctx.db.category.id.update({ ...found, slug: newSlug });
    for (const existing of ctx.db.product.byCategory.filter(id)) {
      ctx.db.product.id.update({ ...existing, categorySlug: newSlug });
    }
  }
);
