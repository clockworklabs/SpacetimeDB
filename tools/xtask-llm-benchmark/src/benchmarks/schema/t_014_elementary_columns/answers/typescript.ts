import { table, schema, t } from 'spacetimedb/server';

export const Primitive = table({
  name: 'primitive',
}, {
  id: t.i32().primaryKey(),
  count: t.i32(),
  total: t.i64(),
  price: t.f32(),
  ratio: t.f64(),
  active: t.bool(),
  name: t.string(),
});

const spacetimedb = schema(Primitive);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.primitive.insert({
      id: 1,
      count: 2,
      total: 3000000000n,
      price: 1.5,
      ratio: 2.25,
      active: true,
      name: "Alice",
    });
  }
);
