import { schema, table, t } from 'spacetimedb/server';

const widget = table(
  { name: 'widget', public: true },
  { id: t.u64().primaryKey(), name: t.string(), enabled: t.bool().default(true) }
);

const spacetimedb = schema({ widget });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.widget.insert({ id: 1n, name: 'legacy', enabled: true });
});

export const touch = spacetimedb.reducer(_ctx => {});
