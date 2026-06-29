import { schema, table, t } from 'spacetimedb/server';

const admin = table({
  name: 'admin',
}, {
  identity: t.identity().primaryKey(),
});

const banned = table({
  name: 'banned',
}, {
  identity: t.identity().primaryKey(),
});

const player = table({
  name: 'player',
  public: true,
}, {
  identity: t.identity().primaryKey(),
  name: t.string(),
});

const spacetimedb = schema({ admin, banned, player });
export default spacetimedb;

export const add_admin = spacetimedb.reducer(
  { target: t.identity() },
  (ctx, { target }) => {
    if (!ctx.db.admin.identity.find(ctx.sender)) throw new Error('not admin');
    try { ctx.db.admin.insert({ identity: target }); } catch {}
  }
);

export const ban_player = spacetimedb.reducer(
  { target: t.identity() },
  (ctx, { target }) => {
    if (!ctx.db.admin.identity.find(ctx.sender)) throw new Error('not admin');
    ctx.db.banned.insert({ identity: target });
    if (ctx.db.player.identity.find(target)) {
      ctx.db.player.identity.delete(target);
    }
  }
);

export const join_game = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    if (ctx.db.banned.identity.find(ctx.sender)) throw new Error('banned');
    if (ctx.db.player.identity.find(ctx.sender)) throw new Error('already in game');
    ctx.db.player.insert({ identity: ctx.sender, name });
  }
);
