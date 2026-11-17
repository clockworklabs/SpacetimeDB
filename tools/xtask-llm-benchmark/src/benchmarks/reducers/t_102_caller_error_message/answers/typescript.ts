import { schema, SenderError, table, t } from 'spacetimedb/server';

const item = table(
  { name: 'item', public: true },
  { id: t.u64().primaryKey(), name: t.string() }
);
const spacetimedb = schema({ item });
export default spacetimedb;

export const add_item = spacetimedb.reducer(
  { id: t.u64(), name: t.string() },
  (ctx, { id, name }) => {
    if (name === '') {
      throw new SenderError('name required');
    }
    ctx.db.item.insert({ id, name });
  }
);
