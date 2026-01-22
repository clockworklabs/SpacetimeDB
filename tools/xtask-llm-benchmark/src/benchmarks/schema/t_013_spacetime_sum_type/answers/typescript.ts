import { table, schema, t } from 'spacetimedb/server';

export const Rect = t.object('Rect', {
  width: t.i32(),
  height: t.i32(),
});

export const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: Rect,
});

export const Result = table({
  name: 'result',
}, {
  id: t.i32().primaryKey(),
  value: Shape,
});

const spacetimedb = schema(Result);

spacetimedb.reducer('setCircle', { id: t.i32(), radius: t.i32() },
  (ctx, { id, radius }) => {
    ctx.db.result.insert({ id, value: { circle: radius } });
  }
);
