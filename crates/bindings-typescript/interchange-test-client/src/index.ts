// A client of the module in ../spacetimedb. `pnpm build` compiles it twice: as
// written, against the generated bindings, and with tsconfig.source.json,
// against the module source. Proposal 0040 requires both to compile.
import {
  DbConnection,
  procedures,
  reducers,
  tables,
  type ErrorContext,
  type EventContext,
} from './module_bindings';
import type {
  LevelUp,
  MatchSummary,
  Player,
  TeamStanding,
} from './module_bindings/types';

export function connect(
  uri: string,
  databaseName: string
): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(uri)
      .withDatabaseName(databaseName)
      .onConnect(conn => resolve(conn))
      .onConnectError((_ctx: ErrorContext, error) => reject(error))
      .build();
  });
}

export function subscribe(conn: DbConnection): Promise<void> {
  return new Promise((resolve, reject) => {
    conn
      .subscriptionBuilder()
      .onApplied(() => resolve())
      .onError(ctx => reject(ctx.event))
      .subscribe([
        tables.player,
        tables.matchResult2.where(row => row.score1.gte(0)),
        tables.levelUp,
        tables.teamStandings,
        tables.myMatches,
      ]);
  });
}

/** Calls each reducer and procedure, and reports what the client then sees. */
export async function play(conn: DbConnection) {
  const levelUps: LevelUp[] = [];
  conn.db.levelUp.onInsert((_ctx: EventContext, row) => levelUps.push(row));
  const updated = new Promise<Player>(resolve =>
    conn.db.player.onUpdate((_ctx, _old, row) => {
      if (row.status.tag === 'Banned') resolve(row);
    })
  );

  await conn.reducers.addPlayer({
    name: 'alice',
    teamId: 1,
    position: { x: 1, y: 2 },
  });
  await conn.reducers.addPlayer({
    name: 'bob',
    teamId: 1,
    position: { x: -3, y: 4 },
  });
  const alice: Player | null = conn.db.player.name.find('alice');
  const bob: Player | null = conn.db.player.name.find('bob');
  if (!alice || !bob) throw new Error('players were not inserted');
  await conn.reducers.recordMatch({ winnerId: alice.id, score1: 3, score2: 1 });
  await conn.reducers.banPlayer({ id: bob.id, reason: 'cheating' });
  await conn.reducers.resetLevels({});
  const banned = await updated;
  const count: number = await conn.procedures.playerCount({ minLevel: 1 });

  const standings: TeamStanding[] = [...conn.db.teamStandings.iter()];
  const matches: MatchSummary[] = [...conn.db.myMatches.iter()];
  return {
    players: [...conn.db.player.iter()]
      .map(({ name, teamId, level, position, status }) => ({
        name,
        teamId,
        level,
        position,
        status,
      }))
      .sort((a, b) => a.name.localeCompare(b.name)),
    byPrimaryKey: conn.db.player.id.find(alice.id)?.name,
    byIndex: [...conn.db.player.teamId.filter(1)].length,
    byMultiColumnIndex: [...conn.db.player.byTeamAndLevel.filter([1, 1])]
      .length,
    // A prefix of a hash index's columns, which only a btree can scan.
    byHashIndexPrefix: [...conn.db.player.byLevelAndTeam.filter(1)].length,
    matches: matches.map(({ score1, score2 }) => ({ score1, score2 })),
    matchesByWinner: [...conn.db.matchResult2.winnerId.filter(alice.id)].length,
    levelUps: levelUps.map(({ level }) => level),
    banned: banned.status,
    standings,
    count,
    reducerNames: Object.keys(reducers).sort(),
    procedureNames: Object.keys(procedures).sort(),
  };
}
