// A module for testing that a client can import this source in place of its
// generated bindings. It follows TypeScript's naming conventions, as proposal
// 0040 requires of interchangeable modules, and leaves most canonical names to
// the case conversion policy.
import {
  Router,
  schema,
  SenderError,
  SyncResponse,
  t,
  table,
  type ReducerCtx,
} from 'spacetimedb/server';

const Point = t.object('Point', { x: t.i32(), y: t.i32() });

const PlayerStatus = t.enum('PlayerStatus', {
  Active: t.unit(),
  Banned: t.string(),
});

const player = table(
  {
    public: true,
    indexes: [
      {
        accessor: 'byTeamAndLevel',
        algorithm: 'btree',
        columns: ['teamId', 'level'],
      },
      // A client keeps a hash index as a btree, as its bindings declare it.
      {
        accessor: 'byLevelAndTeam',
        algorithm: 'hash',
        columns: ['level', 'teamId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().unique(),
    teamId: t.u32().index('btree'),
    level: t.u32().default(1),
    position: Point,
    status: PlayerStatus,
  }
);

const matchResult2 = table(
  { public: true },
  {
    matchId: t.u64().primaryKey().autoInc(),
    winnerId: t.u64().index('btree'),
    score1: t.u32(),
    score2: t.u32(),
    playedAt: t.timestamp(),
  }
);

const levelUp = table(
  { public: true, event: true },
  { playerId: t.u64(), level: t.u32() }
);

const auditEntry = table(
  { name: 'audit_log' },
  { id: t.u64().primaryKey().autoInc(), message: t.string() }
);

const spacetimedb = schema({ player, matchResult2, levelUp, auditEntry });
export default spacetimedb;

type Ctx = ReducerCtx<typeof spacetimedb.schemaType>;

// A helper that takes a context, as module code often does.
function requirePlayer(ctx: Ctx, id: bigint) {
  const found = ctx.db.player.id.find(id);
  if (!found) throw new SenderError(`No player ${id}`);
  return found;
}

export const init = spacetimedb.init(ctx => {
  ctx.db.auditEntry.insert({ id: 0n, message: 'init' });
});

export const onConnect = spacetimedb.clientConnected(ctx => {
  ctx.db.auditEntry.insert({ id: 0n, message: `connect ${ctx.sender}` });
});

export const addPlayer = spacetimedb.reducer(
  { name: t.string(), teamId: t.u32(), position: Point },
  (ctx, { name, teamId, position }) => {
    ctx.db.player.insert({
      id: 0n,
      name,
      teamId,
      level: 1,
      position,
      status: { tag: 'Active' },
    });
  }
);

export const recordMatch = spacetimedb.reducer(
  { name: 'record_match' },
  { winnerId: t.u64(), score1: t.u32(), score2: t.u32() },
  (ctx, { winnerId, score1, score2 }) => {
    const winner = requirePlayer(ctx, winnerId);
    ctx.db.matchResult2.insert({
      matchId: 0n,
      winnerId,
      score1,
      score2,
      playedAt: ctx.timestamp,
    });
    const level = winner.level + 1;
    ctx.db.player.id.update({ ...winner, level });
    ctx.db.levelUp.insert({ playerId: winnerId, level });
  }
);

export const banPlayer = spacetimedb.reducer(
  { id: t.u64(), reason: t.string() },
  (ctx, { id, reason }) => {
    const banned = requirePlayer(ctx, id);
    ctx.db.player.id.update({
      ...banned,
      status: { tag: 'Banned', value: reason },
    });
  }
);

export const resetLevels = spacetimedb.reducer(ctx => {
  for (const row of ctx.db.player.iter()) {
    ctx.db.player.id.update({ ...row, level: 1 });
  }
});

export const playerCount = spacetimedb.procedure(
  { minLevel: t.u32() },
  t.u32(),
  (ctx, { minLevel }) =>
    ctx.withTx(
      tx => [...tx.db.player.iter()].filter(p => p.level >= minLevel).length
    )
);

// The host converts a procedure's explicit name under the policy, to
// `count_teams`.
export const countTeams = spacetimedb.procedure(
  { name: 'CountTeams' },
  {},
  t.u32(),
  ctx =>
    ctx.withTx(tx => new Set([...tx.db.player.iter()].map(p => p.teamId)).size)
);

export const teamStandings = spacetimedb.anonymousView(
  { public: true },
  t.array(
    t.row('TeamStanding', {
      teamId: t.u32().primaryKey(),
      players: t.u32(),
      topLevel: t.u32(),
    })
  ),
  ctx => {
    const standings = new Map<
      number,
      { teamId: number; players: number; topLevel: number }
    >();
    for (const row of ctx.db.player.iter()) {
      const standing = standings.get(row.teamId) ?? {
        teamId: row.teamId,
        players: 0,
        topLevel: 0,
      };
      standing.players += 1;
      standing.topLevel = Math.max(standing.topLevel, row.level);
      standings.set(row.teamId, standing);
    }
    return [...standings.values()];
  }
);

export const myMatches = spacetimedb.view(
  { name: 'my_matches', public: true },
  t.array(
    t.row('MatchSummary', {
      matchId: t.u64().primaryKey(),
      score1: t.u32(),
      score2: t.u32(),
    })
  ),
  ctx =>
    [...ctx.db.matchResult2.iter()].map(({ matchId, score1, score2 }) => ({
      matchId,
      score1,
      score2,
    }))
);

export const health = spacetimedb.httpHandler(
  (_ctx, _req) => new SyncResponse('ok')
);

export const router = spacetimedb.httpRouter(
  new Router().get('/health', health)
);
