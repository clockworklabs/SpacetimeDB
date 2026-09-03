import { schema, table, t } from 'spacetimedb/server';

const legacyItem = table(
  { name: 'legacy_item', public: true },
  { id: t.u64().primaryKey(), value: t.string() }
);
const spacetimedb = schema({ legacyItem });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.legacyItem.insert({ id: 1n, value: 'old' });
});
