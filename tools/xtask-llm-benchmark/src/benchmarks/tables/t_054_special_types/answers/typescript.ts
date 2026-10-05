import { schema, table, t } from 'spacetimedb/server';

const specialValue = table(
  { name: 'special_value', public: true },
  {
    id: t.u64().primaryKey(),
    uuid: t.uuid(),
    connectionId: t.connectionId(),
    duration: t.timeDuration(),
    unsigned128: t.u128(),
    signed128: t.i128(),
    unsigned256: t.u256(),
    signed256: t.i256(),
  }
);

export default schema({ specialValue });
