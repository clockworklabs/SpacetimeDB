import { table, schema, t } from 'spacetimedb/server';

export const Score = t.object('Score', {
  left: t.i32(),
  right: t.i32(),
});

export const Result = table({
  name: 'result',
}, {
  id: t.i32().primaryKey(),
  value: Score,
});

const spacetimedb = schema(Result);

spacetimedb.reducer('setScore', { id: t.i32(), left: t.i32(), right: t.i32() },
  (ctx, { id, left, right }) => {
    ctx.db.result.insert({ id, value: { left, right } });
  }
);
