import {
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
  type SumBuilder,
} from 'spacetimedb/server';

type LobbyTicketStatusBuilder = SumBuilder<
  Record<
    'Queued' | 'Matched' | 'Cancelled' | 'Expired',
    ReturnType<typeof t.unit>
  >
>;
export const ticketStatus: LobbyTicketStatusBuilder = t.enum(
  'LobbyTicketStatus',
  {
    Queued: t.unit(),
    Matched: t.unit(),
    Cancelled: t.unit(),
    Expired: t.unit(),
  }
);
export const TicketStatus = {
  Queued: { tag: 'Queued' as const },
  Matched: { tag: 'Matched' as const },
  Cancelled: { tag: 'Cancelled' as const },
  Expired: { tag: 'Expired' as const },
};

type LobbyRoomStatusBuilder = SumBuilder<
  Record<'Ready' | 'Active' | 'Closed' | 'Abandoned', ReturnType<typeof t.unit>>
>;
export const roomStatus: LobbyRoomStatusBuilder = t.enum('LobbyRoomStatus', {
  Ready: t.unit(),
  Active: t.unit(),
  Closed: t.unit(),
  Abandoned: t.unit(),
});
export const RoomStatus = {
  Ready: { tag: 'Ready' as const },
  Active: { tag: 'Active' as const },
  Closed: { tag: 'Closed' as const },
  Abandoned: { tag: 'Abandoned' as const },
};

type LobbySeatStatusBuilder = SumBuilder<
  Record<'Reserved' | 'Joined' | 'Left', ReturnType<typeof t.unit>>
>;
export const seatStatus: LobbySeatStatusBuilder = t.enum('LobbySeatStatus', {
  Reserved: t.unit(),
  Joined: t.unit(),
  Left: t.unit(),
});
export const SeatStatus = {
  Reserved: { tag: 'Reserved' as const },
  Joined: { tag: 'Joined' as const },
  Left: { tag: 'Left' as const },
};

const lobbyConfigOptions = { name: 'lobby_config', public: false };
const lobbyConfigColumns = {
  singleton: t.bool().primaryKey(),
  defaultTicketTtlSeconds: t.u32(),
  maxMatchSize: t.u32(),
  /** Ready rooms whose seats are not all joined by then are abandoned. */
  readyTimeoutSeconds: t.u32(),
  /** Finished tickets, rooms, seats, and results are deleted after this. */
  retentionSeconds: t.u32(),
  updatedAt: t.timestamp(),
};
export const lobbyConfig: ReturnType<
  typeof table<typeof lobbyConfigColumns, typeof lobbyConfigOptions>
> = table(lobbyConfigOptions, lobbyConfigColumns);

const lobbySweepTickOptions = { name: 'lobby_sweep_tick' };
const lobbySweepTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const lobbySweepTick: ReturnType<
  typeof table<typeof lobbySweepTickColumns, typeof lobbySweepTickOptions>
> = table(lobbySweepTickOptions, lobbySweepTickColumns);

const lobbyAdminIdentityOptions = {
  name: 'lobby_admin_identity',
  public: false,
};
const lobbyAdminIdentityColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
export const lobbyAdminIdentity: ReturnType<
  typeof table<
    typeof lobbyAdminIdentityColumns,
    typeof lobbyAdminIdentityOptions
  >
> = table(lobbyAdminIdentityOptions, lobbyAdminIdentityColumns);

const lobbyQueueTicketOptions = {
  name: 'lobby_queue_ticket',
  public: false,
  indexes: [
    { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] } as const,
    {
      accessor: 'bySubject',
      algorithm: 'btree',
      columns: ['subject'],
    } as const,
    {
      accessor: 'byCreatedAt',
      algorithm: 'btree',
      columns: ['createdAt'],
    } as const,
    {
      accessor: 'byStatusUpdatedAt',
      algorithm: 'btree',
      columns: ['status', 'updatedAt'],
    } as const,
    {
      accessor: 'byPoolStatusCreatedAt',
      algorithm: 'btree',
      columns: ['pool', 'status', 'createdAt'],
    } as const,
    {
      accessor: 'byStatusExpiresAt',
      algorithm: 'btree',
      columns: ['status', 'expiresAt'],
    } as const,
    {
      accessor: 'bySubjectStatus',
      algorithm: 'btree',
      columns: ['subject', 'status'],
    } as const,
  ],
};
const lobbyQueueTicketColumns = {
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
  expiresAt: t.timestamp(),
};
export const lobbyQueueTicket: ReturnType<
  typeof table<typeof lobbyQueueTicketColumns, typeof lobbyQueueTicketOptions>
> = table(lobbyQueueTicketOptions, lobbyQueueTicketColumns);

const lobbySubjectRatingOptions = {
  name: 'lobby_subject_rating',
  public: false,
  indexes: [
    { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] } as const,
    {
      accessor: 'bySubject',
      algorithm: 'btree',
      columns: ['subject'],
    } as const,
    { accessor: 'byRating', algorithm: 'btree', columns: ['rating'] } as const,
    {
      accessor: 'byLeaderboardOrder',
      algorithm: 'btree',
      columns: ['pool', 'ratingOrder', 'subject'],
    } as const,
    {
      accessor: 'byUpdatedAt',
      algorithm: 'btree',
      columns: ['updatedAt'],
    } as const,
  ],
};
const lobbySubjectRatingColumns = {
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
};
export const lobbySubjectRating: ReturnType<
  typeof table<
    typeof lobbySubjectRatingColumns,
    typeof lobbySubjectRatingOptions
  >
> = table(lobbySubjectRatingOptions, lobbySubjectRatingColumns);

const lobbyRoomOptions = {
  name: 'lobby_room',
  public: false,
  indexes: [
    { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] } as const,
    { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] } as const,
    {
      accessor: 'byCreatedAt',
      algorithm: 'btree',
      columns: ['createdAt'],
    } as const,
    {
      accessor: 'byStatusUpdatedAt',
      algorithm: 'btree',
      columns: ['status', 'updatedAt'],
    } as const,
  ],
};
const lobbyRoomColumns = {
  roomId: t.u64().primaryKey().autoInc(),
  pool: t.string(),
  status: roomStatus,
  capacity: t.u32(),
  metadataJson: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
  closedAt: t.option(t.timestamp()),
};
export const lobbyRoom: ReturnType<
  typeof table<typeof lobbyRoomColumns, typeof lobbyRoomOptions>
> = table(lobbyRoomOptions, lobbyRoomColumns);

const lobbyMatchResultOptions = {
  name: 'lobby_match_result',
  public: false,
  indexes: [
    { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] } as const,
    { accessor: 'byPool', algorithm: 'btree', columns: ['pool'] } as const,
    {
      accessor: 'byReportedAt',
      algorithm: 'btree',
      columns: ['reportedAt'],
    } as const,
  ],
};
const lobbyMatchResultColumns = {
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
};
export const lobbyMatchResult: ReturnType<
  typeof table<typeof lobbyMatchResultColumns, typeof lobbyMatchResultOptions>
> = table(lobbyMatchResultOptions, lobbyMatchResultColumns);

const lobbyRoomSeatOptions = {
  name: 'lobby_room_seat',
  public: false,
  indexes: [
    { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] } as const,
    {
      accessor: 'bySubject',
      algorithm: 'btree',
      columns: ['subject'],
    } as const,
    { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] } as const,
    {
      accessor: 'byRoomSubject',
      algorithm: 'btree',
      columns: ['roomId', 'subject'],
    } as const,
  ],
};
const lobbyRoomSeatColumns = {
  seatId: t.u64().primaryKey().autoInc(),
  roomId: t.u64(),
  subject: t.string(),
  ticketId: t.option(t.string()),
  seatIndex: t.u32(),
  status: seatStatus,
  joinedAt: t.option(t.timestamp()),
  leftAt: t.option(t.timestamp()),
  updatedAt: t.timestamp(),
};
export const lobbyRoomSeat: ReturnType<
  typeof table<typeof lobbyRoomSeatColumns, typeof lobbyRoomSeatOptions>
> = table(lobbyRoomSeatOptions, lobbyRoomSeatColumns);

export const queueSummaryRow = t.object('LobbyQueueSummaryRow', {
  pool: t.string(),
  queuedTickets: t.u32(),
  readyRooms: t.u32(),
  activeRooms: t.u32(),
});

export const lobbyStatus = t.object('LobbyStatus', {
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

const tables: {
  lobbyConfig: typeof lobbyConfig;
  lobbyAdminIdentity: typeof lobbyAdminIdentity;
  lobbyQueueTicket: typeof lobbyQueueTicket;
  lobbySubjectRating: typeof lobbySubjectRating;
  lobbyRoom: typeof lobbyRoom;
  lobbyMatchResult: typeof lobbyMatchResult;
  lobbyRoomSeat: typeof lobbyRoomSeat;
  lobbySweepTick: typeof lobbySweepTick;
} = {
  lobbyConfig,
  lobbyAdminIdentity,
  lobbyQueueTicket,
  lobbySubjectRating,
  lobbyRoom,
  lobbyMatchResult,
  lobbyRoomSeat,
  lobbySweepTick,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { t };
