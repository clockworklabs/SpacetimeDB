import { schema, table, t } from 'spacetimedb/server';

const legacyItem = table(
  { name: 'legacy_item', public: true },
  { id: t.u64().primaryKey(), value: t.string() }
);
const itemV2 = table(
  { name: 'item_v2', public: true },
  { id: t.u64().primaryKey(), value: t.string(), version: t.u32() }
);
const spacetimedb = schema({ legacyItem, itemV2 });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.legacyItem.insert({ id: 1n, value: 'old' });
});

export const migrate = spacetimedb.reducer(ctx => {
  for (const row of ctx.db.legacyItem.iter()) {
    if (!ctx.db.itemV2.id.find(row.id)) {
      ctx.db.itemV2.insert({ id: row.id, value: row.value, version: 2 });
    }
  }
});

export const dual_write = spacetimedb.reducer(
  { id: t.u64(), value: t.string() },
  (ctx, { id, value }) => {
    ctx.db.legacyItem.insert({ id, value });
    ctx.db.itemV2.insert({ id, value, version: 2 });
  }
);
