import { table, schema, t } from 'spacetimedb/server';

export const entity = table(
  {
    name: 'entity',
  },
  {
    id: t.i32().primaryKey(),
  }
);

export const position = table(
  {
    name: 'position',
  },
  {
    entityId: t.i32().primaryKey(),
    x: t.i32(),
    y: t.i32(),
  }
);

export const velocity = table(
  {
    name: 'velocity',
  },
  {
    entityId: t.i32().primaryKey(),
    vx: t.i32(),
    vy: t.i32(),
  }
);

export const nextPosition = table(
  {
    name: 'nextPosition',
  },
  {
    entityId: t.i32().primaryKey(),
    x: t.i32(),
    y: t.i32(),
  }
);

const spacetimedb = schema({ entity, position, velocity, nextPosition });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.entity.insert({ id: 1 });
  ctx.db.entity.insert({ id: 2 });

  ctx.db.position.insert({ entityId: 1, x: 1, y: 0 });
  ctx.db.position.insert({ entityId: 2, x: 10, y: 0 });

  ctx.db.velocity.insert({ entityId: 1, vx: 1, vy: 0 });
  ctx.db.velocity.insert({ entityId: 2, vx: -2, vy: 3 });
});

export const step = spacetimedb.reducer(ctx => {
  for (const p of ctx.db.position.iter()) {
    const v = ctx.db.velocity.entityId.find(p.entityId);
    if (v) {
      const np = {
        entityId: p.entityId,
        x: p.x + v.vx,
        y: p.y + v.vy,
      };

      if (ctx.db.nextPosition.entityId.find(p.entityId)) {
        ctx.db.nextPosition.entityId.update(np);
      } else {
        ctx.db.nextPosition.insert(np);
      }
    }
  }
});
