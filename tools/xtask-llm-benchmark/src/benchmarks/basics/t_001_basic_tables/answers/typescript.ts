import { table, schema, t } from 'spacetimedb/server';

export const User = table({
  name: 'user',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
  age: t.i32(),
  active: t.bool(),
});

export const Product = table({
  name: 'product',
}, {
  id: t.i32().primaryKey(),
  title: t.string(),
  price: t.f32(),
  inStock: t.bool(),
});

export const Note = table({
  name: 'note',
}, {
  id: t.i32().primaryKey(),
  body: t.string(),
  rating: t.i64(),
  pinned: t.bool(),
});

const spacetimedb = schema(User, Product, Note);
