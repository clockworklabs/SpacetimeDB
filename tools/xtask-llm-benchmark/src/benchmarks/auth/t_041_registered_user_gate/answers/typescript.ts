import { schema, table, t } from 'spacetimedb/server';

const user = table({
  name: 'user',
}, {
  identity: t.identity().primaryKey(),
  name: t.string(),
});

const message = table({
  name: 'message',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  sender: t.identity().index('btree'),
  text: t.string(),
});

const spacetimedb = schema({ user, message });
export default spacetimedb;

export const register = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.user.identity.find(ctx.sender)) {
      throw new Error('already registered');
    }
    ctx.db.user.insert({ identity: ctx.sender, name });
  }
);

export const post_message = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    if (!ctx.db.user.identity.find(ctx.sender)) {
      throw new Error('not registered');
    }
    ctx.db.message.insert({ id: 0n, sender: ctx.sender, text });
  }
);
