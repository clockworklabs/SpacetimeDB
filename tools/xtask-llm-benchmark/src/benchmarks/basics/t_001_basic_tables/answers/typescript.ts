import { table, schema, t } from 'spacetimedb/server';

const user = table({
  name: 'user',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
  age: t.i32(),
  active: t.bool(),
});

const product = table({
  name: 'product',
}, {
  id: t.i32().primaryKey(),
  title: t.string(),
  price: t.f32(),
  inStock: t.bool(),
});

const note = table({
  name: 'note',
}, {
  id: t.i32().primaryKey(),
  body: t.string(),
  rating: t.i64(),
  pinned: t.bool(),
});

const spacetimedb = schema({ user, product, note });
export default spacetimedb;
