import { Range } from 'spacetimedb/server';
import {
  RoomStatus,
  TicketStatus,
  lobbyMatchResult,
  lobbyQueueTicket,
  lobbyRoom,
  lobbyRoomSeat,
  queueSummaryRow,
  rankedRatingRow,
  spacetimedb,
  t,
  type ViewModuleCtx,
} from './schema';

const MAX_MATCH_CANDIDATES = 5000;
const MAX_VIEW_ROWS = 500;

function subjectForSender(ctx: ViewModuleCtx): string {
  return ctx.sender.toHexString();
}

function isAdmin(ctx: ViewModuleCtx): boolean {
  return ctx.db.lobbyAdminIdentity.identity.find(ctx.sender) != null;
}

function newestFirst<T extends { createdAt: { microsSinceUnixEpoch: bigint } }>(
  rows: T[]
): T[] {
  return rows.sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch;
    const bv = b.createdAt.microsSinceUnixEpoch;
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
}

function selectTopRows<T>(
  rows: Iterable<T>,
  limit: number,
  compare: (a: T, b: T) => number
): T[] {
  const selected: T[] = [];
  for (const row of rows) {
    let low = 0;
    let high = selected.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      if (compare(row, selected[mid]!) < 0) high = mid;
      else low = mid + 1;
    }
    if (low >= limit) continue;
    selected.splice(low, 0, row);
    if (selected.length > limit) selected.pop();
  }
  return selected;
}

function newestRowsBy<T>(
  rows: Iterable<T>,
  limit: number,
  timestamp: (row: T) => bigint
): T[] {
  return selectTopRows(rows, limit, (a, b) => {
    const av = timestamp(a);
    const bv = timestamp(b);
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
}

function newestRows<T extends { createdAt: { microsSinceUnixEpoch: bigint } }>(
  rows: Iterable<T>,
  limit: number
): T[] {
  return newestRowsBy(rows, limit, row => row.createdAt.microsSinceUnixEpoch);
}

function take<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

export const myLobbyTickets = spacetimedb.view(
  { name: 'my_lobby_tickets', public: true },
  t.array(lobbyQueueTicket.rowType),
  ctx =>
    newestRows(
      ctx.db.lobbyQueueTicket.bySubject.filter(subjectForSender(ctx)),
      MAX_VIEW_ROWS
    )
);

export const myLobbyRatings = spacetimedb.view(
  { name: 'my_lobby_ratings', public: true },
  t.array(rankedRatingRow),
  ctx =>
    selectTopRows(
      ctx.db.lobbySubjectRating.bySubject.filter(subjectForSender(ctx)),
      MAX_VIEW_ROWS,
      (a, b) => a.pool.localeCompare(b.pool)
    )
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

export const myLobbyRoomSeats = spacetimedb.view(
  { name: 'my_lobby_room_seats', public: true },
  t.array(lobbyRoomSeat.rowType),
  ctx =>
    newestRowsBy(
      ctx.db.lobbyRoomSeat.bySubject.filter(subjectForSender(ctx)),
      MAX_VIEW_ROWS,
      row => row.updatedAt.microsSinceUnixEpoch
    )
);

export const myLobbyRooms = spacetimedb.view(
  { name: 'my_lobby_rooms', public: true },
  t.array(lobbyRoom.rowType),
  ctx => {
    const subject = subjectForSender(ctx);
    const seen = new Set<string>();
    const rooms = [];
    for (const seat of newestRowsBy(
      ctx.db.lobbyRoomSeat.bySubject.filter(subject),
      MAX_VIEW_ROWS,
      row => row.updatedAt.microsSinceUnixEpoch
    )) {
      const key = seat.roomId.toString();
      if (seen.has(key)) continue;
      seen.add(key);
      const room = ctx.db.lobbyRoom.roomId.find(seat.roomId);
      if (room) rooms.push(room);
    }
    return newestFirst(rooms);
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
    for (const ticket of take(
      ctx.db.lobbyQueueTicket.byStatus.filter(TicketStatus.Queued),
      MAX_MATCH_CANDIDATES
    )) {
      ensure(ticket.pool).queuedTickets++;
    }
    for (const room of take(
      ctx.db.lobbyRoom.byStatus.filter(RoomStatus.Ready),
      MAX_MATCH_CANDIDATES
    )) {
      ensure(room.pool).readyRooms++;
    }
    for (const room of take(
      ctx.db.lobbyRoom.byStatus.filter(RoomStatus.Active),
      MAX_MATCH_CANDIDATES
    )) {
      ensure(room.pool).activeRooms++;
    }
    return [...summary.values()].sort((a, b) => a.pool.localeCompare(b.pool));
  }
);

export const lobbyRankedLeaderboard = spacetimedb.view(
  { name: 'lobby_ranked_leaderboard', public: true },
  t.array(rankedRatingRow),
  ctx =>
    take(
      ctx.db.lobbySubjectRating.byLeaderboardOrder.filter(new Range()),
      500
    ).map(row => ({
      pool: row.pool,
      subject: row.subject,
      rating: row.rating,
      wins: row.wins,
      losses: row.losses,
      draws: row.draws,
      matches: row.matches,
    }))
);

export const lobbyAdminTickets = spacetimedb.view(
  { name: 'lobby_admin_tickets', public: true },
  t.array(lobbyQueueTicket.rowType),
  ctx =>
    isAdmin(ctx)
      ? newestRows(
          ctx.db.lobbyQueueTicket.byCreatedAt.filter(new Range()),
          MAX_VIEW_ROWS
        )
      : []
);

export const lobbyAdminRooms = spacetimedb.view(
  { name: 'lobby_admin_rooms', public: true },
  t.array(lobbyRoom.rowType),
  ctx =>
    isAdmin(ctx)
      ? newestRows(
          ctx.db.lobbyRoom.byCreatedAt.filter(new Range()),
          MAX_VIEW_ROWS
        )
      : []
);

export const lobbyAdminRoomSeats = spacetimedb.view(
  { name: 'lobby_admin_room_seats', public: true },
  t.array(lobbyRoomSeat.rowType),
  ctx => (isAdmin(ctx) ? take(ctx.db.lobbyRoomSeat.iter(), 1000) : [])
);

export const lobbyAdminMatchResults = spacetimedb.view(
  { name: 'lobby_admin_match_results', public: true },
  t.array(lobbyMatchResult.rowType),
  ctx =>
    isAdmin(ctx)
      ? selectTopRows(
          ctx.db.lobbyMatchResult.byReportedAt.filter(new Range()),
          500,
          (a, b) => {
            const av = a.reportedAt.microsSinceUnixEpoch;
            const bv = b.reportedAt.microsSinceUnixEpoch;
            return av < bv ? 1 : av > bv ? -1 : 0;
          }
        )
      : []
);
