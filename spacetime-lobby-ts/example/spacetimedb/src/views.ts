import { Range, t } from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

import {
  RATING_POOL,
  pilot,
  duel,
  duelCombatant,
  duelRoundLog,
  duelManeuver,
  queueSummaryRow,
  ratingRow,
  spacetimedb,
  type ReadCtx,
} from './schema';

const PLAYER_ROSTER_LIMIT = 1000;

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

function activeRoomIdsForSubject(ctx: ReadCtx, subject: string): bigint[] {
  const roomIds: bigint[] = [];
  const seen = new Set<string>();
  for (const seat of [
    ...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subject),
  ]) {
    const key = seat.roomId.toString();
    if (seen.has(key)) continue;
    seen.add(key);
    roomIds.push(seat.roomId);
  }
  return roomIds;
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

export const myLobbyRatings = spacetimedb.view(
  { name: 'my_lobby_ratings', public: true },
  t.array(ratingRow),
  ctx =>
    [...ctx.db.lobby.lobbySubjectRating.bySubject.filter(subjectFor(ctx))]
      .map(row => ({
        pool: row.pool,
        subject: row.subject,
        rating: row.rating,
        wins: row.wins,
        losses: row.losses,
        draws: row.draws,
        matches: row.matches,
      }))
      .sort((a, b) => a.pool.localeCompare(b.pool))
);

export const lobbyRankedLeaderboard = spacetimedb.view(
  { name: 'lobby_ranked_leaderboard', public: true },
  t.array(ratingRow),
  ctx =>
    [...ctx.db.lobby.lobbySubjectRating.byPool.filter(RATING_POOL)]
      .sort((a, b) => {
        if (a.rating !== b.rating) return b.rating - a.rating;
        return a.subject.localeCompare(b.subject);
      })
      .slice(0, 10)
      .map(row => ({
        pool: row.pool,
        subject: row.subject,
        rating: row.rating,
        wins: row.wins,
        losses: row.losses,
        draws: row.draws,
        matches: row.matches,
      }))
);

export const myDuels = spacetimedb.view(
  { name: 'my_duels', public: true },
  t.array(duel.rowType),
  ctx => {
    const roomIds = new Set<string>();
    const rows = [];
    for (const seat of [
      ...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subjectFor(ctx)),
    ]) {
      const key = seat.roomId.toString();
      if (roomIds.has(key)) continue;
      roomIds.add(key);
      const duelRow = ctx.db.duel.roomId.find(seat.roomId);
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
  ctx => {
    const roomIds = activeRoomIdsForSubject(ctx, subjectFor(ctx));
    return roomIds.flatMap(roomId => [
      ...ctx.db.duelCombatant.byRoom.filter(roomId),
    ]);
  }
);

export const myDuelRoundLogs = spacetimedb.view(
  { name: 'my_duel_round_logs', public: true },
  t.array(duelRoundLog.rowType),
  ctx => {
    const roomIds = new Set(
      activeRoomIdsForSubject(ctx, subjectFor(ctx)).map(roomId =>
        roomId.toString()
      )
    );
    const logs = [
      ...ctx.db.duelRoundLog.byCreatedAt.filter(new Range()),
    ].filter(row => roomIds.has(row.roomId.toString()));
    return logs
      .sort((a, b) => {
        const av = a.createdAt.microsSinceUnixEpoch;
        const bv = b.createdAt.microsSinceUnixEpoch;
        return av < bv ? -1 : av > bv ? 1 : 0;
      })
      .slice(-80);
  }
);

export const myDuelManeuvers = spacetimedb.view(
  { name: 'my_duel_maneuvers', public: true },
  t.array(duelManeuver.rowType),
  ctx => {
    const roomIds = new Set(
      activeRoomIdsForSubject(ctx, subjectFor(ctx)).map(roomId =>
        roomId.toString()
      )
    );
    return [...ctx.db.duelManeuver.byRoom.filter(new Range())]
      .filter(row => roomIds.has(row.roomId.toString()))
      .sort((a, b) => {
        if (a.roomId !== b.roomId) return a.roomId < b.roomId ? -1 : 1;
        if (a.round !== b.round) return a.round - b.round;
        return a.subject.localeCompare(b.subject);
      });
  }
);

export const myLobbyTickets = spacetimedb.view(
  { name: 'my_lobby_tickets', public: true },
  lobby.t.array(lobby.lobbyQueueTicket.rowType),
  ctx => [...ctx.db.lobby.lobbyQueueTicket.bySubject.filter(subjectFor(ctx))]
);

export const myLobbyRooms = spacetimedb.view(
  { name: 'my_lobby_rooms', public: true },
  lobby.t.array(lobby.lobbyRoom.rowType),
  ctx => {
    const seen = new Set<string>();
    const rooms = [];
    for (const seat of [
      ...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subjectFor(ctx)),
    ]) {
      const key = seat.roomId.toString();
      if (seen.has(key)) continue;
      seen.add(key);
      const room = ctx.db.lobby.lobbyRoom.roomId.find(seat.roomId);
      if (room) rooms.push(room);
    }
    return rooms;
  }
);

export const myLobbyRoomSeats = spacetimedb.view(
  { name: 'my_lobby_room_seats', public: true },
  lobby.t.array(lobby.lobbyRoomSeat.rowType),
  ctx => {
    const roomIds = new Set(
      [...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subjectFor(ctx))].map(
        seat => seat.roomId.toString()
      )
    );
    return [...ctx.db.lobby.lobbyRoomSeat.iter()].filter(seat =>
      roomIds.has(seat.roomId.toString())
    );
  }
);

export const lobbyQueueSummary = spacetimedb.view(
  { name: 'lobby_queue_summary', public: true },
  t.array(queueSummaryRow),
  ctx => {
    const summary = new Map<
      string,
      {
        pool: string;
        queuedTickets: number;
        readyRooms: number;
        activeRooms: number;
      }
    >();
    const ensure = (pool: string) => {
      let row = summary.get(pool);
      if (!row) {
        row = { pool, queuedTickets: 0, readyRooms: 0, activeRooms: 0 };
        summary.set(pool, row);
      }
      return row;
    };
    for (const ticket of [
      ...ctx.db.lobby.lobbyQueueTicket.byStatus.filter(
        lobby.TicketStatus.Queued
      ),
    ]) {
      ensure(ticket.pool).queuedTickets++;
    }
    for (const room of [
      ...ctx.db.lobby.lobbyRoom.byStatus.filter(lobby.RoomStatus.Ready),
    ]) {
      ensure(room.pool).readyRooms++;
    }
    for (const room of [
      ...ctx.db.lobby.lobbyRoom.byStatus.filter(lobby.RoomStatus.Active),
    ]) {
      ensure(room.pool).activeRooms++;
    }
    return [...summary.values()].sort((a, b) => a.pool.localeCompare(b.pool));
  }
);
