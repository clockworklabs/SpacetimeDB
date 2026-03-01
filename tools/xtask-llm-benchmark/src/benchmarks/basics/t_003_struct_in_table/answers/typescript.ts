import { table, schema, t } from 'spacetimedb/server';

const Position = t.object('Position', {
  x: t.i32(),
  y: t.i32(),
});

const entity = table({
  name: 'entity',
}, {
  id: t.i32().primaryKey(),
  pos: Position,
});

const spacetimedb = schema({ entity });
export default spacetimedb;
