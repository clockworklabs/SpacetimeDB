import { schema, t, table } from "spacetimedb/server";

const item = table(
  { name: "item" },
  {
    id: t.u32().primaryKey(),
    value: t.u32(),
  }
);

const itemCount = t.object("ItemCountRow", {
  count: t.u64(),
});

const spacetimedb = schema({ item });
export default spacetimedb;

export const sender_table_count = spacetimedb.view(
  { public: true },
  t.option(itemCount),
  ctx => ({ count: ctx.db.item.count() })
);

export const anon_table_count = spacetimedb.anonymousView(
  { public: true },
  t.option(itemCount),
  ctx => ({ count: ctx.db.item.count() })
);

export const insert_item = spacetimedb.reducer(
  { id: t.u32(), value: t.u32() },
  (ctx, { id, value }) => {
    ctx.db.item.insert({ id, value });
  }
);

export const replace_item = spacetimedb.reducer(
  { id: t.u32(), value: t.u32() },
  (ctx, { id, value }) => {
    ctx.db.item.id.delete(id);
    ctx.db.item.insert({ id, value });
  }
);

export const delete_item = spacetimedb.reducer(
  { id: t.u32() },
  (ctx, { id }) => {
    ctx.db.item.id.delete(id);
  }
);
