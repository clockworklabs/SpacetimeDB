import { table, schema, t } from 'spacetimedb/server';

export const Result = table({
  name: 'result',
}, {
  id: t.i32().primaryKey(),
  sum: t.i32(),
});

const spacetimedb = schema({ Result });
export default spacetimedb;

function add(a: number, b: number): number {
  return a + b;
}

export const computeSum = spacetimedb.reducer({ id: t.i32(), a: t.i32(), b: t.i32() },
  (ctx, { id, a, b }) => {
    ctx.db.result.insert({ id, sum: add(a, b) });
  }
);
