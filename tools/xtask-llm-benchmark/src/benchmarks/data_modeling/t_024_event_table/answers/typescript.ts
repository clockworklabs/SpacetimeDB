import { schema, table, t } from 'spacetimedb/server';

const damageEvent = table({
  name: 'damage_event',
  public: true,
  event: true,
}, {
  entityId: t.u64(),
  damage: t.u32(),
  source: t.string(),
});

const spacetimedb = schema({ damageEvent });
export default spacetimedb;

export const deal_damage = spacetimedb.reducer(
  { entityId: t.u64(), damage: t.u32(), source: t.string() },
  (ctx, { entityId, damage, source }) => {
    ctx.db.damageEvent.insert({ entityId, damage, source });
  }
);
