import {
  SenderError,
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { installLobby } from './install';

export const ticketStatus = t.enum('LobbyTicketStatus', [
  'Queued',
  'Matched',
  'Cancelled',
  'Expired',
]);
export const TicketStatus = {
  Queued: { tag: 'Queued' as const },
  Matched: { tag: 'Matched' as const },
  Cancelled: { tag: 'Cancelled' as const },
  Expired: { tag: 'Expired' as const },
};

export const roomStatus = t.enum('LobbyRoomStatus', [
  'Ready',
  'Active',
  'Closed',
  'Abandoned',
]);
export const RoomStatus = {
  Ready: { tag: 'Ready' as const },
  Active: { tag: 'Active' as const },
  Closed: { tag: 'Closed' as const },
  Abandoned: { tag: 'Abandoned' as const },
};

export const seatStatus = t.enum('LobbySeatStatus', [
  'Reserved',
  'Joined',
  'Left',
  'Disconnected',
]);
export const SeatStatus = {
  Reserved: { tag: 'Reserved' as const },
  Joined: { tag: 'Joined' as const },
  Left: { tag: 'Left' as const },
  Disconnected: { tag: 'Disconnected' as const },
};

export const lobbyConfig = table(
  { name: 'lobby_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    defaultTicketTtlSeconds: t.u32(),
    maxMatchSize: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const lobbyAdminIdentity = table(
  { name: 'lobby_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

export const lobbyQueueTicket = table(
  {
    name: 'lobby_queue_ticket',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] },
      { accessor: 'bySubject', algorithm: 'btree', columns: ['subject'] },
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
      {
        accessor: 'byPoolStatusCreatedAt',
        algorithm: 'btree',
        columns: ['pool', 'status', 'createdAt'],
      },
      {
        accessor: 'byStatusExpiresAt',
        algorithm: 'btree',
        columns: ['status', 'expiresAtMicros'],
      },
      {
        accessor: 'bySubjectStatus',
        algorithm: 'btree',
        columns: ['subject', 'status'],
      },
    ],
  },
  {
    ticketId: t.string().primaryKey(),
    pool: t.string(),
    subject: t.string(),
    status: ticketStatus,
    matchSize: t.u32(),
    ranked: t.bool(),
    rating: t.option(t.i32()),
    ratingPool: t.option(t.string()),
    partyId: t.option(t.string()),
    attributesJson: t.option(t.string()),
    roomId: t.option(t.u64()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
    expiresAtMicros: t.i64(),
  }
);

export const lobbySubjectRating = table(
  {
    name: 'lobby_subject_rating',
    public: false,
    indexes: [
      { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] },
      { accessor: 'bySubject', algorithm: 'btree', columns: ['subject'] },
      { accessor: 'byRating', algorithm: 'btree', columns: ['rating'] },
      {
        accessor: 'byLeaderboardOrder',
        algorithm: 'btree',
        columns: ['pool', 'ratingOrder', 'subject'],
      },
      { accessor: 'byUpdatedAt', algorithm: 'btree', columns: ['updatedAt'] },
    ],
  },
  {
    ratingId: t.string().primaryKey(),
    pool: t.string(),
    subject: t.string(),
    rating: t.i32(),
    ratingOrder: t.i64(),
    wins: t.u32(),
    losses: t.u32(),
    draws: t.u32(),
    matches: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const lobbyRoom = table(
  {
    name: 'lobby_room',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] },
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
    ],
  },
  {
    roomId: t.u64().primaryKey().autoInc(),
    pool: t.string(),
    status: roomStatus,
    capacity: t.u32(),
    metadataJson: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
    closedAt: t.option(t.timestamp()),
  }
);

export const lobbyMatchResult = table(
  {
    name: 'lobby_match_result',
    public: false,
    indexes: [
      { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] },
      { accessor: 'byReportedAt', algorithm: 'btree', columns: ['reportedAt'] },
    ],
  },
  {
    resultId: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    pool: t.string(),
    winnerSubject: t.option(t.string()),
    loserSubject: t.option(t.string()),
    subjectA: t.string(),
    subjectB: t.string(),
    ratingABefore: t.i32(),
    ratingAAfter: t.i32(),
    ratingBBefore: t.i32(),
    ratingBAfter: t.i32(),
    reportedAt: t.timestamp(),
  }
);

export const lobbyRoomSeat = table(
  {
    name: 'lobby_room_seat',
    public: false,
    indexes: [
      { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'bySubject', algorithm: 'btree', columns: ['subject'] },
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      {
        accessor: 'byRoomSubject',
        algorithm: 'btree',
        columns: ['roomId', 'subject'],
      },
    ],
  },
  {
    seatId: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    subject: t.string(),
    ticketId: t.option(t.string()),
    seatIndex: t.u32(),
    status: seatStatus,
    ready: t.bool(),
    joinedAt: t.option(t.timestamp()),
    leftAt: t.option(t.timestamp()),
    updatedAt: t.timestamp(),
  }
);

export const queueSummaryRow = t.object('LobbyQueueSummaryRow', {
  pool: t.string(),
  queuedTickets: t.u32(),
  readyRooms: t.u32(),
  activeRooms: t.u32(),
});

export const lobbyStatusRow = t.object('LobbyStatusRow', {
  defaultTicketTtlSeconds: t.u32(),
  maxMatchSize: t.u32(),
  queuedTickets: t.u32(),
  readyRooms: t.u32(),
  activeRooms: t.u32(),
});

export const rankedRatingRow = t.object('LobbyRankedRatingRow', {
  pool: t.string(),
  subject: t.string(),
  rating: t.i32(),
  wins: t.u32(),
  losses: t.u32(),
  draws: t.u32(),
  matches: t.u32(),
});

export const spacetimedb = schema({
  lobbyConfig,
  lobbyAdminIdentity,
  lobbyQueueTicket,
  lobbySubjectRating,
  lobbyRoom,
  lobbyMatchResult,
  lobbyRoomSeat,
});

export const init = spacetimedb.init(ctx => {
  installLobby(ctx);
});

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { SenderError, t };
