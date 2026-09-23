import { schema, table, t } from 'spacetimedb/server';

const damage_event = table({
  name: 'damage_event',
  public: true,
  event: true,
}, {
  entityId: t.u64(),
  damage: t.u32(),
  source: t.string(),
});

const spacetimedb = schema({ damage_event });
export default spacetimedb;

export const deal_damage = spacetimedb.reducer(
  { entityId: t.u64(), damage: t.u32(), source: t.string() },
  (ctx, { entityId, damage, source }) => {
    ctx.db.damage_event.insert({ entityId, damage, source });
  }
);
