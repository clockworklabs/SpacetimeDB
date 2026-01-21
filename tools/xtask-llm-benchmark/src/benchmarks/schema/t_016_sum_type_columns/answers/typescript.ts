import { table, schema, t } from 'spacetimedb/server';

export const Rect = t.object('Rect', {
  width: t.i32(),
  height: t.i32(),
});

export const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: Rect,
});

export const Drawing = table({
  name: 'drawing',
}, {
  id: t.i32().primaryKey(),
  a: Shape,
  b: Shape,
});

const spacetimedb = schema(Drawing);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.drawing.insert({
      id: 1,
      a: { circle: 10 },
      b: { rectangle: { width: 4, height: 6 } },
    });
  }
);
