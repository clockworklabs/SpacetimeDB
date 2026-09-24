// The TypeScript version of `modules/scoped-views-bench`:
// the same team chat as a per-user view and as a scoped view.
// See `crates/testing/tests/scoped_views_bench.rs`.

import { schema, t, table } from 'spacetimedb/server';

const player = table(
  { name: 'player' },
  {
    identity: t.identity().primaryKey(),
    teamId: t.u64(),
  }
);

const chatMessage = table(
  { name: 'chat_message' },
  {
    id: t.u64().primaryKey().autoInc(),
    teamId: t.u64().index(),
    text: t.string(),
  }
);

const spacetimedb = schema({ player, chatMessage });
export default spacetimedb;

// The team chat as a per-user view: computed once per subscriber.
export const team_chat_per_user = spacetimedb.view(
  { name: 'team_chat_per_user', public: true },
  t.array(chatMessage.rowType),
  ctx => {
    const me = ctx.db.player.identity.find(ctx.sender);
    return me ? Array.from(ctx.db.chatMessage.teamId.filter(me.teamId)) : [];
  }
);

// The team chat as a scoped view: computed once per team.
export const team_chat_scoped = spacetimedb.scopedView(
  { name: 'team_chat_scoped', public: true, scope: t.u64() },
  t.array(chatMessage.rowType),
  ctx => ctx.db.player.identity.find(ctx.sender)?.teamId,
  (ctx, teamId) => Array.from(ctx.db.chatMessage.teamId.filter(teamId))
);

export const join = spacetimedb.reducer(
  { teamId: t.u64() },
  (ctx, { teamId }) => {
    ctx.db.player.insert({ identity: ctx.sender, teamId });
  }
);

export const set_team = spacetimedb.reducer(
  { teamId: t.u64() },
  (ctx, { teamId }) => {
    const me = ctx.db.player.identity.find(ctx.sender);
    if (me) {
      ctx.db.player.identity.update({ ...me, teamId });
    }
  }
);

export const send = spacetimedb.reducer(
  { teamId: t.u64(), text: t.string() },
  (ctx, { teamId, text }) => {
    ctx.db.chatMessage.insert({ id: 0n, teamId, text });
  }
);
