import { table, schema, t } from 'spacetimedb/server';

const Rect = t.object('Rect', {
  width: t.i32(),
  height: t.i32(),
});

const Shape = t.enum('Shape', {
  circle: t.i32(),
  rectangle: Rect,
});

const result = table(
  {
    name: 'result',
  },
  {
    id: t.i32().primaryKey(),
    value: Shape,
  }
);

const spacetimedb = schema({ result });
export default spacetimedb;

export const setCircle = spacetimedb.reducer(
  { id: t.i32(), radius: t.i32() },
  (ctx, { id, radius }) => {
    ctx.db.result.insert({ id, value: { circle: radius } });
  }
);
