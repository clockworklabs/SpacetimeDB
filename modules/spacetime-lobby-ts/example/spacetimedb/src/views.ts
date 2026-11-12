import { t } from 'spacetimedb/server';

import {
  pilot,
  duel,
  duelCombatant,
  duelRoundLog,
  duelManeuver,
  spacetimedb,
  type ReadCtx,
} from './schema';

const PLAYER_ROSTER_LIMIT = 1000;
const ROUND_LOG_LIMIT = 80;

function takeRows<T>(rows: Iterable<T>, limit: number): T[] {
  const result: T[] = [];
  for (const row of rows) {
    if (result.length >= limit) break;
    result.push(row);
  }
  return result;
}

function subjectFor(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

function roomIdsForSubject(ctx: ReadCtx, subject: string): bigint[] {
  const roomIds = new Map<string, bigint>();
  for (const seat of ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subject)) {
    roomIds.set(seat.roomId.toString(), seat.roomId);
  }
  return [...roomIds.values()];
}

export const myProfile = spacetimedb.view(
  { name: 'my_profile', public: true },
  t.array(pilot.rowType),
  ctx => {
    const row = ctx.db.pilot.subject.find(subjectFor(ctx));
    return row ? [row] : [];
  }
);

export const players = spacetimedb.view(
  { name: 'players', public: true },
  t.array(pilot.rowType),
  ctx => takeRows(ctx.db.pilot.iter(), PLAYER_ROSTER_LIMIT)
);

export const myDuels = spacetimedb.view(
  { name: 'my_duels', public: true },
  t.array(duel.rowType),
  ctx => {
    const rows = [];
    for (const roomId of roomIdsForSubject(ctx, subjectFor(ctx))) {
      const duelRow = ctx.db.duel.roomId.find(roomId);
      if (duelRow) rows.push(duelRow);
    }
    return rows.sort((a, b) => {
      const av = a.updatedAt.microsSinceUnixEpoch;
      const bv = b.updatedAt.microsSinceUnixEpoch;
      return av < bv ? 1 : av > bv ? -1 : 0;
    });
  }
);

export const myDuelCombatants = spacetimedb.view(
  { name: 'my_duel_combatants', public: true },
  t.array(duelCombatant.rowType),
  ctx =>
    roomIdsForSubject(ctx, subjectFor(ctx)).flatMap(roomId => [
      ...ctx.db.duelCombatant.byRoom.filter(roomId),
    ])
);

export const myDuelRoundLogs = spacetimedb.view(
  { name: 'my_duel_round_logs', public: true },
  t.array(duelRoundLog.rowType),
  ctx =>
    roomIdsForSubject(ctx, subjectFor(ctx))
      .flatMap(roomId => [...ctx.db.duelRoundLog.byRoom.filter(roomId)])
      .sort((a, b) => (a.logId < b.logId ? -1 : a.logId > b.logId ? 1 : 0))
      .slice(-ROUND_LOG_LIMIT)
);

// The caller sees its own choices and every choice from resolved rounds. The
// opponent's choice for the round in progress stays hidden until it resolves.
export const myDuelManeuvers = spacetimedb.view(
  { name: 'my_duel_maneuvers', public: true },
  t.array(duelManeuver.rowType),
  ctx => {
    const subject = subjectFor(ctx);
    return roomIdsForSubject(ctx, subject)
      .flatMap(roomId => {
        const resolvedRound = ctx.db.duel.roomId.find(roomId)?.round ?? 0;
        return [...ctx.db.duelManeuver.byRoom.filter(roomId)].filter(
          row => row.subject === subject || row.round <= resolvedRound
        );
      })
      .sort((a, b) => {
        if (a.roomId !== b.roomId) return a.roomId < b.roomId ? -1 : 1;
        if (a.round !== b.round) return a.round - b.round;
        return a.subject.localeCompare(b.subject);
      });
  }
);
