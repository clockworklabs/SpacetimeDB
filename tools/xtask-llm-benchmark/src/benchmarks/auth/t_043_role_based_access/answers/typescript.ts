import { schema, table, t } from 'spacetimedb/server';

const user = table({
  name: 'user',
}, {
  identity: t.identity().primaryKey(),
  role: t.string(),
});

const spacetimedb = schema({ user });
export default spacetimedb;

export const register = spacetimedb.reducer(
  {},
  (ctx) => {
    if (ctx.db.user.identity.find(ctx.sender)) {
      throw new Error('already registered');
    }
    ctx.db.user.insert({ identity: ctx.sender, role: 'member' });
  }
);

export const promote = spacetimedb.reducer(
  { target: t.identity() },
  (ctx, { target }) => {
    const caller = ctx.db.user.identity.find(ctx.sender);
    if (!caller) throw new Error('not registered');
    if (caller.role !== 'admin') throw new Error('not admin');
    const targetUser = ctx.db.user.identity.find(target);
    if (!targetUser) throw new Error('target not registered');
    ctx.db.user.identity.update({ ...targetUser, role: 'admin' });
  }
);

export const member_action = spacetimedb.reducer(
  {},
  (ctx) => {
    if (!ctx.db.user.identity.find(ctx.sender)) throw new Error('not registered');
  }
);

export const admin_action = spacetimedb.reducer(
  {},
  (ctx) => {
    const user = ctx.db.user.identity.find(ctx.sender);
    if (!user) throw new Error('not registered');
    if (user.role !== 'admin') throw new Error('not admin');
  }
);
