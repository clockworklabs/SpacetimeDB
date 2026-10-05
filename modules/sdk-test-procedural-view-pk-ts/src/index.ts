import { schema, t, table } from 'spacetimedb/server';

const LeftSource = t.row('LeftSource', {
  id: t.u64().primaryKey(),
  sender: t.identity().index('btree'),
  filter: t.u64(),
});

const RightSource = t.row('RightSource', {
  id: t.u64().primaryKey(),
  sender: t.identity().index('btree'),
  filter: t.u64(),
});

const left_source = table({ public: true }, LeftSource);
const right_source = table({ public: true }, RightSource);

const spacetimedb = schema({ left_source, right_source });
export default spacetimedb;

export const insert_left = spacetimedb.reducer(
  { id: t.u64(), filter: t.u64() },
  (ctx, { id, filter }) => {
    ctx.db.left_source.insert({ id, sender: ctx.sender, filter });
  }
);

export const update_left = spacetimedb.reducer(
  { id: t.u64(), filter: t.u64() },
  (ctx, { id, filter }) => {
    ctx.db.left_source.id.update({ id, sender: ctx.sender, filter });
  }
);

export const insert_right = spacetimedb.reducer(
  { id: t.u64(), filter: t.u64() },
  (ctx, { id, filter }) => {
    ctx.db.right_source.insert({ id, sender: ctx.sender, filter });
  }
);

export const sender_left_view = spacetimedb.view(
  { public: true },
  t.array(left_source.rowType),
  ctx => Array.from(ctx.db.left_source.sender.filter(ctx.sender))
);

export const sender_right_view = spacetimedb.view(
  { public: true },
  t.array(right_source.rowType),
  ctx => Array.from(ctx.db.right_source.sender.filter(ctx.sender))
);
