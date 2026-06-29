import { table, schema, t } from 'spacetimedb/server';

const Rect = t.object('Rect', {
  width: t.i32(),
  height: t.i32(),
});

const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: Rect,
});

const drawing = table({
  name: 'drawing',
}, {
  id: t.u64().primaryKey().autoInc(),
  a: Shape,
  b: Shape,
});

const spacetimedb = schema({ drawing });
export default spacetimedb;

export const seed = spacetimedb.reducer(
  ctx => {
    ctx.db.drawing.insert({
      id: 0n,
      a: { tag: 'circle', value: 10 },
      b: { tag: 'rectangle', value: { width: 4, height: 6 } },
    });
  }
);
