import { table, schema, t } from 'spacetimedb/server';

export const Address = t.object('Address', {
  street: t.string(),
  zip: t.i32(),
});

export const Position = t.object('Position', {
  x: t.i32(),
  y: t.i32(),
});

export const Profile = table({
  name: 'profile',
}, {
  id: t.i32().primaryKey(),
  home: Address,
  work: Address,
  pos: Position,
});

const spacetimedb = schema(Profile);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.profile.insert({
      id: 1,
      home: { street: "1 Main", zip: 11111 },
      work: { street: "2 Broad", zip: 22222 },
      pos: { x: 7, y: 9 },
    });
  }
);
