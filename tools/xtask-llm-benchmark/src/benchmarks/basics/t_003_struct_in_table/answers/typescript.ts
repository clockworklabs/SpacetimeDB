import { table, schema, t } from 'spacetimedb/server';

export const Position = t.object('Position', {
  x: t.i32(),
  y: t.i32(),
});

export const Entity = table({
  name: 'entity',
}, {
  id: t.i32().primaryKey(),
  pos: Position,
});

const spacetimedb = schema(Entity);
