import { schema, table, t } from 'spacetimedb/server';

const message = table({
  name: 'message',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  owner: t.identity().index('btree'),
  text: t.string(),
});

const spacetimedb = schema({ message });
export default spacetimedb;

export const create_message = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    ctx.db.message.insert({
      id: 0n,
      owner: ctx.sender,
      text,
    });
  }
);

export const delete_message = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const msg = ctx.db.message.id.find(id);
    if (!msg) {
      throw new Error("not found");
    }
    if (!msg.owner.equals(ctx.sender)) {
      throw new Error("unauthorized");
    }
    ctx.db.message.id.delete(id);
  }
);
