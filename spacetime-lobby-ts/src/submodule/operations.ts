import { Range, type Infer } from 'spacetimedb/server';
import {
  RoomStatus,
  SeatStatus,
  SenderError,
  TicketStatus,
  lobbyQueueTicket,
  lobbyRoom,
  lobbySubjectRating,
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema';
import {
  DEFAULT_RATING,
  MAX_RATING,
  MIN_RATING,
  expectedScore,
  rankedSelection,
  updatedRating,
} from '../matchmaking';
import { lobbyCompositeKey } from '../keys';

const MAX_POOL_LENGTH = 96;
const MAX_SUBJECT_LENGTH = 160;
const MAX_JSON_LENGTH = 4096;
const DEFAULT_EXPIRE_LIMIT = 100;
const MAX_EXPIRE_LIMIT = 1000;
const MAX_MATCH_CANDIDATES = 5000;

type QueueTicketRow = Infer<typeof lobbyQueueTicket.rowType>;
type RoomRow = Infer<typeof lobbyRoom.rowType>;
type SubjectRatingRow = Infer<typeof lobbySubjectRating.rowType>;

export type JoinQueueArgs = {
  pool: string;
  subject: string;
  matchSize: number;
  attributesJson?: string | undefined;
  ttlSeconds?: number | undefined;
};

export type JoinRankedQueueArgs = JoinQueueArgs & {
  ratingPool?: string | undefined;
};

export type TicketSubjectArgs = {
  ticketId: string;
  subject: string;
};

export type RoomSubjectArgs = {
  roomId: bigint;
  subject: string;
};

export type ReportMatchResultArgs = {
  roomId: bigint;
  subject: string;
  winnerSubject?: string | undefined;
};

export type SetRatingArgs = {
  pool: string;
  subject: string;
  rating: number;
};

export type JoinQueueResult = {
  ticketId: string;
  roomId?: bigint | undefined;
};

function fail(message: string): never {
  throw new SenderError(`lobby.${message}`);
}

function subjectForSender(
  ctx: WriteCtx | ProcedureModuleCtx | ViewModuleCtx
): string {
  return ctx.sender.toHexString();
}

function normalizeName(value: string, field: string, max: number): string {
  const out = value.trim();
  if (!out) fail(`invalid_${field}`);
  if (out.length > max) fail(`${field}_too_long`);
  return out;
}

function validateJson(
  value: string | undefined,
  field: string
): string | undefined {
  if (value === undefined) return undefined;
  const out = value.trim();
  if (!out) return undefined;
  if (out.length > MAX_JSON_LENGTH) fail(`${field}_too_long`);
  try {
    const parsed = JSON.parse(out) as unknown;
    if (
      parsed === null ||
      typeof parsed !== 'object' ||
      Array.isArray(parsed)
    ) {
      fail(`invalid_${field}_json`);
    }
  } catch {
    fail(`invalid_${field}_json`);
  }
  return out;
}

function nowMicros(ctx: WriteCtx): bigint {
  return ctx.timestamp.microsSinceUnixEpoch;
}

function ratingId(pool: string, subject: string): string {
  return lobbyCompositeKey(pool, subject);
}

function getRating(
  ctx: WriteCtx | ViewModuleCtx,
  pool: string,
  subject: string
) {
  return ctx.db.lobbySubjectRating.ratingId.find(ratingId(pool, subject));
}

function getOrCreateRating(ctx: WriteCtx, pool: string, subject: string) {
  const existing = getRating(ctx, pool, subject);
  if (existing) return existing;
  const row = {
    ratingId: ratingId(pool, subject),
    pool,
    subject,
    rating: DEFAULT_RATING,
    ratingOrder: BigInt(-DEFAULT_RATING),
    wins: 0,
    losses: 0,
    draws: 0,
    matches: 0,
    updatedAt: ctx.timestamp,
  };
  ctx.db.lobbySubjectRating.insert(row);
  return row;
}

function validateRating(value: number): number {
  const rating = Math.trunc(value);
  if (!Number.isInteger(rating) || rating < MIN_RATING || rating > MAX_RATING) {
    fail('invalid_rating');
  }
  return rating;
}

function getConfig(ctx: WriteCtx) {
  const existing = ctx.db.lobbyConfig.singleton.find(true);
  if (existing) return existing;
  const row = {
    singleton: true,
    defaultTicketTtlSeconds: 60,
    maxMatchSize: 16,
    updatedAt: ctx.timestamp,
  };
  ctx.db.lobbyConfig.insert(row);
  return row;
}

function isAdmin(ctx: WriteCtx | ViewModuleCtx, sender = ctx.sender): boolean {
  return ctx.db.lobbyAdminIdentity.identity.find(sender) != null;
}

function requireAdmin(ctx: WriteCtx): void {
  if (!isAdmin(ctx)) fail('not_authorized');
}

function isQueued(ticket: QueueTicketRow): boolean {
  return ticket.status.tag === TicketStatus.Queued.tag;
}

function isTerminalRoom(room: RoomRow): boolean {
  return (
    room.status.tag === RoomStatus.Closed.tag ||
    room.status.tag === RoomStatus.Abandoned.tag
  );
}

function take<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

function countUpTo<T>(rows: Iterable<T>, limit: number): number {
  let count = 0;
  for (const _row of rows) {
    if (count >= limit) break;
    count++;
  }
  return count;
}

function expireQueuedTickets(ctx: WriteCtx, limit: number): number {
  const now = nowMicros(ctx);
  let expired = 0;
  for (const ticket of ctx.db.lobbyQueueTicket.byStatusExpiresAt.filter([
    TicketStatus.Queued,
    new Range(undefined, { tag: 'included', value: now }),
  ])) {
    if (expired >= limit) break;
    ctx.db.lobbyQueueTicket.ticketId.update({
      ...ticket,
      status: TicketStatus.Expired,
      updatedAt: ctx.timestamp,
    });
    expired++;
  }
  return expired;
}

function activeTicketsForSubjectPool(
  ctx: WriteCtx,
  subject: string,
  pool: string
) {
  return take(
    ctx.db.lobbyQueueTicket.bySubjectStatus.filter([
      subject,
      TicketStatus.Queued,
    ]),
    1000
  ).filter(ticket => ticket.pool === pool && isQueued(ticket));
}

function seatsForRoom(ctx: WriteCtx | ViewModuleCtx, roomId: bigint) {
  return take(ctx.db.lobbyRoomSeat.byRoom.filter(roomId), 128);
}

function findSeat(ctx: WriteCtx, roomId: bigint, subject: string) {
  for (const seat of ctx.db.lobbyRoomSeat.byRoomSubject.filter([
    roomId,
    subject,
  ]))
    return seat;
  return undefined;
}

function refreshRoomAfterJoin(ctx: WriteCtx, roomId: bigint): void {
  const room = ctx.db.lobbyRoom.roomId.find(roomId);
  if (!room || isTerminalRoom(room)) return;
  const seats = seatsForRoom(ctx, roomId);
  if (seats.length === 0) return;
  const allJoined = seats.every(
    seat => seat.status.tag === SeatStatus.Joined.tag
  );
  if (allJoined && room.status.tag === RoomStatus.Ready.tag) {
    ctx.db.lobbyRoom.roomId.update({
      ...room,
      status: RoomStatus.Active,
      updatedAt: ctx.timestamp,
    });
  }
}

function markRoomAbandoned(ctx: WriteCtx, roomId: bigint): void {
  const room = ctx.db.lobbyRoom.roomId.find(roomId);
  if (!room || isTerminalRoom(room)) return;
  ctx.db.lobbyRoom.roomId.update({
    ...room,
    status: RoomStatus.Abandoned,
    updatedAt: ctx.timestamp,
    closedAt: ctx.timestamp,
  });
}

function attemptMatch(
  ctx: WriteCtx,
  pool: string,
  matchSize: number,
  ranked: boolean
): bigint | undefined {
  const now = nowMicros(ctx);
  const queued = take(
    ctx.db.lobbyQueueTicket.byPoolStatusCreatedAt.filter([
      pool,
      TicketStatus.Queued,
      new Range(),
    ]),
    MAX_MATCH_CANDIDATES
  ).filter(
    ticket =>
      ticket.pool === pool &&
      ticket.ranked === ranked &&
      ticket.matchSize === matchSize &&
      ticket.expiresAtMicros > now
  );
  if (queued.length < matchSize) return undefined;

  const selected = ranked
    ? rankedSelection(queued, matchSize, now)
    : queued.slice(0, matchSize);
  if (!selected || selected.length < matchSize) return undefined;

  const room = ctx.db.lobbyRoom.insert({
    roomId: 0n,
    pool,
    status: RoomStatus.Ready,
    capacity: matchSize,
    metadataJson: ranked
      ? JSON.stringify({
          ranked: true,
          ratingPool: selected[0].ratingPool ?? pool,
        })
      : undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    closedAt: undefined,
  });

  selected.forEach((ticket, index) => {
    ctx.db.lobbyRoomSeat.insert({
      seatId: 0n,
      roomId: room.roomId,
      subject: ticket.subject,
      ticketId: ticket.ticketId,
      seatIndex: index,
      status: SeatStatus.Reserved,
      ready: false,
      joinedAt: undefined,
      leftAt: undefined,
      updatedAt: ctx.timestamp,
    });
    ctx.db.lobbyQueueTicket.ticketId.update({
      ...ticket,
      status: TicketStatus.Matched,
      roomId: room.roomId,
      updatedAt: ctx.timestamp,
    });
  });

  return room.roomId;
}

export function joinQueue(ctx: WriteCtx, args: JoinQueueArgs): JoinQueueResult {
  const config = getConfig(ctx);
  const pool = normalizeName(args.pool, 'pool', MAX_POOL_LENGTH);
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const matchSize = Math.trunc(args.matchSize);
  if (
    !Number.isInteger(matchSize) ||
    matchSize < 1 ||
    matchSize > config.maxMatchSize
  ) {
    fail('invalid_match_size');
  }
  const ttlSeconds =
    args.ttlSeconds === undefined
      ? config.defaultTicketTtlSeconds
      : Math.trunc(args.ttlSeconds);
  if (
    !Number.isInteger(ttlSeconds) ||
    ttlSeconds < 1 ||
    ttlSeconds > 24 * 60 * 60
  ) {
    fail('invalid_ttl_seconds');
  }
  const attributesJson = validateJson(args.attributesJson, 'attributes');

  expireQueuedTickets(ctx, DEFAULT_EXPIRE_LIMIT);
  for (const ticket of activeTicketsForSubjectPool(ctx, subject, pool)) {
    ctx.db.lobbyQueueTicket.ticketId.update({
      ...ticket,
      status: TicketStatus.Cancelled,
      updatedAt: ctx.timestamp,
    });
  }

  const ticketId = `ticket:${ctx.newUuidV7().toString()}`;
  ctx.db.lobbyQueueTicket.insert({
    ticketId,
    pool,
    subject,
    status: TicketStatus.Queued,
    matchSize,
    ranked: false,
    rating: undefined,
    ratingPool: undefined,
    partyId: undefined,
    attributesJson,
    roomId: undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    expiresAtMicros: nowMicros(ctx) + BigInt(ttlSeconds) * 1_000_000n,
  });

  const roomId = attemptMatch(ctx, pool, matchSize, false);
  return { ticketId, roomId };
}

export function joinRankedQueue(
  ctx: WriteCtx,
  args: JoinRankedQueueArgs
): JoinQueueResult {
  const config = getConfig(ctx);
  const pool = normalizeName(args.pool, 'pool', MAX_POOL_LENGTH);
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const matchSize = Math.trunc(args.matchSize);
  if (
    !Number.isInteger(matchSize) ||
    matchSize < 1 ||
    matchSize > config.maxMatchSize
  ) {
    fail('invalid_match_size');
  }
  const ttlSeconds =
    args.ttlSeconds === undefined
      ? config.defaultTicketTtlSeconds
      : Math.trunc(args.ttlSeconds);
  if (
    !Number.isInteger(ttlSeconds) ||
    ttlSeconds < 1 ||
    ttlSeconds > 24 * 60 * 60
  ) {
    fail('invalid_ttl_seconds');
  }
  const attributesJson = validateJson(args.attributesJson, 'attributes');
  const ratingPool =
    args.ratingPool === undefined
      ? pool
      : normalizeName(args.ratingPool, 'rating_pool', MAX_POOL_LENGTH);
  const rating = getOrCreateRating(ctx, ratingPool, subject).rating;

  expireQueuedTickets(ctx, DEFAULT_EXPIRE_LIMIT);
  for (const ticket of activeTicketsForSubjectPool(ctx, subject, pool)) {
    ctx.db.lobbyQueueTicket.ticketId.update({
      ...ticket,
      status: TicketStatus.Cancelled,
      updatedAt: ctx.timestamp,
    });
  }

  const ticketId = `ticket:${ctx.newUuidV7().toString()}`;
  ctx.db.lobbyQueueTicket.insert({
    ticketId,
    pool,
    subject,
    status: TicketStatus.Queued,
    matchSize,
    ranked: true,
    rating,
    ratingPool,
    partyId: undefined,
    attributesJson,
    roomId: undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    expiresAtMicros: nowMicros(ctx) + BigInt(ttlSeconds) * 1_000_000n,
  });

  const roomId = attemptMatch(ctx, pool, matchSize, true);
  return { ticketId, roomId };
}

function ratingPoolForRoom(room: RoomRow): string {
  if (!room.metadataJson) return room.pool;
  try {
    const metadata = JSON.parse(room.metadataJson) as { ratingPool?: unknown };
    return typeof metadata.ratingPool === 'string' && metadata.ratingPool.trim()
      ? metadata.ratingPool.trim()
      : room.pool;
  } catch {
    return room.pool;
  }
}

function applyResultRow(
  ctx: WriteCtx,
  row: SubjectRatingRow,
  score: number,
  opponentRating: number
) {
  const nextRating = updatedRating(
    row.rating,
    expectedScore(row.rating, opponentRating),
    score
  );
  const next = {
    ...row,
    rating: nextRating,
    ratingOrder: BigInt(-nextRating),
    wins: row.wins + (score === 1 ? 1 : 0),
    losses: row.losses + (score === 0 ? 1 : 0),
    draws: row.draws + (score === 0.5 ? 1 : 0),
    matches: row.matches + 1,
    updatedAt: ctx.timestamp,
  };
  ctx.db.lobbySubjectRating.ratingId.update(next);
  return next;
}

export function reportMatchResult(
  ctx: WriteCtx,
  args: ReportMatchResultArgs
): void {
  const reporter = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail('room_not_found');
  if (room.status.tag !== RoomStatus.Active.tag) fail('room_not_active');
  const seats = seatsForRoom(ctx, args.roomId).filter(
    seat => seat.status.tag !== SeatStatus.Left.tag
  );
  if (seats.length !== 2) fail('elo_requires_two_seats');
  if (!seats.some(seat => seat.subject === reporter) && !isAdmin(ctx))
    fail('not_room_participant');
  for (const _existing of ctx.db.lobbyMatchResult.byRoom.filter(args.roomId))
    return;

  const [seatA, seatB] = seats.sort((a, b) => a.seatIndex - b.seatIndex);
  const winnerSubject =
    args.winnerSubject === undefined
      ? undefined
      : normalizeName(args.winnerSubject, 'winner_subject', MAX_SUBJECT_LENGTH);
  if (
    winnerSubject !== undefined &&
    winnerSubject !== seatA.subject &&
    winnerSubject !== seatB.subject
  ) {
    fail('winner_not_in_room');
  }

  const scoreA =
    winnerSubject === undefined ? 0.5 : winnerSubject === seatA.subject ? 1 : 0;
  const scoreB = 1 - scoreA;
  const ratingPool = ratingPoolForRoom(room);
  const ratingA = getOrCreateRating(ctx, ratingPool, seatA.subject);
  const ratingB = getOrCreateRating(ctx, ratingPool, seatB.subject);
  const nextA = applyResultRow(ctx, ratingA, scoreA, ratingB.rating);
  const nextB = applyResultRow(ctx, ratingB, scoreB, ratingA.rating);
  ctx.db.lobbyMatchResult.insert({
    resultId: 0n,
    roomId: args.roomId,
    pool: ratingPool,
    winnerSubject,
    loserSubject:
      winnerSubject === undefined
        ? undefined
        : winnerSubject === seatA.subject
          ? seatB.subject
          : seatA.subject,
    subjectA: seatA.subject,
    subjectB: seatB.subject,
    ratingABefore: ratingA.rating,
    ratingAAfter: nextA.rating,
    ratingBBefore: ratingB.rating,
    ratingBAfter: nextB.rating,
    reportedAt: ctx.timestamp,
  });
}

export function cancelTicket(ctx: WriteCtx, args: TicketSubjectArgs): void {
  const ticketId = normalizeName(args.ticketId, 'ticket_id', 200);
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const ticket = ctx.db.lobbyQueueTicket.ticketId.find(ticketId);
  if (!ticket) fail('ticket_not_found');
  if (ticket.subject !== subject) fail('not_ticket_owner');
  if (!isQueued(ticket)) fail('ticket_not_queued');
  ctx.db.lobbyQueueTicket.ticketId.update({
    ...ticket,
    status: TicketStatus.Cancelled,
    updatedAt: ctx.timestamp,
  });
}

export function joinRoom(ctx: WriteCtx, args: RoomSubjectArgs): void {
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail('room_not_found');
  if (isTerminalRoom(room)) fail('room_closed');
  const seat = findSeat(ctx, args.roomId, subject);
  if (!seat) fail('seat_not_found');
  if (seat.status.tag === SeatStatus.Left.tag) fail('seat_left');
  ctx.db.lobbyRoomSeat.seatId.update({
    ...seat,
    status: SeatStatus.Joined,
    joinedAt: seat.joinedAt ?? ctx.timestamp,
    updatedAt: ctx.timestamp,
  });
  refreshRoomAfterJoin(ctx, args.roomId);
}

export function leaveRoom(ctx: WriteCtx, args: RoomSubjectArgs): void {
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail('room_not_found');
  const seat = findSeat(ctx, args.roomId, subject);
  if (!seat) fail('seat_not_found');
  if (seat.status.tag === SeatStatus.Left.tag) return;
  ctx.db.lobbyRoomSeat.seatId.update({
    ...seat,
    status: SeatStatus.Left,
    ready: false,
    leftAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  });
  markRoomAbandoned(ctx, args.roomId);
}

export function closeRoom(ctx: WriteCtx, args: RoomSubjectArgs): void {
  const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail('room_not_found');
  if (!findSeat(ctx, args.roomId, subject)) fail('seat_not_found');
  if (room.status.tag === RoomStatus.Closed.tag) return;
  ctx.db.lobbyRoom.roomId.update({
    ...room,
    status: RoomStatus.Closed,
    updatedAt: ctx.timestamp,
    closedAt: ctx.timestamp,
  });
}

export const join_queue = spacetimedb.reducer(
  {
    pool: t.string(),
    matchSize: t.u32(),
    attributesJson: t.option(t.string()),
    ttlSeconds: t.option(t.u32()),
  },
  (ctx, args) => {
    joinQueue(ctx, {
      pool: args.pool,
      subject: subjectForSender(ctx),
      matchSize: args.matchSize,
      attributesJson: args.attributesJson,
      ttlSeconds: args.ttlSeconds,
    });
  }
);

export const join_ranked_queue = spacetimedb.reducer(
  {
    pool: t.string(),
    matchSize: t.u32(),
    attributesJson: t.option(t.string()),
    ttlSeconds: t.option(t.u32()),
    ratingPool: t.option(t.string()),
  },
  (ctx, args) => {
    joinRankedQueue(ctx, {
      pool: args.pool,
      subject: subjectForSender(ctx),
      matchSize: args.matchSize,
      attributesJson: args.attributesJson,
      ttlSeconds: args.ttlSeconds,
      ratingPool: args.ratingPool,
    });
  }
);

export const cancel_ticket = spacetimedb.reducer(
  { ticketId: t.string() },
  (ctx, args) => {
    cancelTicket(ctx, {
      ticketId: args.ticketId,
      subject: subjectForSender(ctx),
    });
  }
);

export const join_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    joinRoom(ctx, { roomId: args.roomId, subject: subjectForSender(ctx) });
  }
);

export const leave_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    leaveRoom(ctx, { roomId: args.roomId, subject: subjectForSender(ctx) });
  }
);

export const close_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    closeRoom(ctx, { roomId: args.roomId, subject: subjectForSender(ctx) });
  }
);

export const set_rating = spacetimedb.reducer(
  {
    pool: t.string(),
    subject: t.string(),
    rating: t.i32(),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    const pool = normalizeName(args.pool, 'pool', MAX_POOL_LENGTH);
    const subject = normalizeName(args.subject, 'subject', MAX_SUBJECT_LENGTH);
    const rating = validateRating(args.rating);
    const existing = getOrCreateRating(ctx, pool, subject);
    ctx.db.lobbySubjectRating.ratingId.update({
      ...existing,
      rating,
      ratingOrder: BigInt(-rating),
      updatedAt: ctx.timestamp,
    });
  }
);

export const expire_tickets = spacetimedb.reducer(
  { limit: t.option(t.u32()) },
  (ctx, args) => {
    requireAdmin(ctx);
    const limit = args.limit ?? DEFAULT_EXPIRE_LIMIT;
    if (limit < 1 || limit > MAX_EXPIRE_LIMIT) fail('invalid_expire_limit');
    expireQueuedTickets(ctx, limit);
  }
);

export const update_config = spacetimedb.reducer(
  {
    defaultTicketTtlSeconds: t.u32(),
    maxMatchSize: t.u32(),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    if (
      args.defaultTicketTtlSeconds < 1 ||
      args.defaultTicketTtlSeconds > 24 * 60 * 60
    ) {
      fail('invalid_default_ttl_seconds');
    }
    if (args.maxMatchSize < 1 || args.maxMatchSize > 128)
      fail('invalid_max_match_size');
    const config = getConfig(ctx);
    ctx.db.lobbyConfig.singleton.update({
      ...config,
      defaultTicketTtlSeconds: args.defaultTicketTtlSeconds,
      maxMatchSize: args.maxMatchSize,
      updatedAt: ctx.timestamp,
    });
  }
);

export const add_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    if (ctx.db.lobbyAdminIdentity.identity.find(args.identity) != null) return;
    ctx.db.lobbyAdminIdentity.insert({
      identity: args.identity,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
);

export const remove_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    const row = ctx.db.lobbyAdminIdentity.identity.find(args.identity);
    if (!row) return;
    if (ctx.db.lobbyAdminIdentity.count() <= 1n)
      fail('cannot_remove_last_admin');
    ctx.db.lobbyAdminIdentity.delete(row);
  }
);

export const get_lobby_status = spacetimedb.procedure({}, t.string(), ctx => {
  const status = ctx.withTx(tx => {
    const queuedTickets = countUpTo(
      tx.db.lobbyQueueTicket.byStatus.filter(TicketStatus.Queued),
      MAX_MATCH_CANDIDATES
    );
    const readyRooms = countUpTo(
      tx.db.lobbyRoom.byStatus.filter(RoomStatus.Ready),
      MAX_MATCH_CANDIDATES
    );
    const activeRooms = countUpTo(
      tx.db.lobbyRoom.byStatus.filter(RoomStatus.Active),
      MAX_MATCH_CANDIDATES
    );
    const config = getConfig(tx);
    return {
      defaultTicketTtlSeconds: config.defaultTicketTtlSeconds,
      maxMatchSize: config.maxMatchSize,
      queuedTickets,
      readyRooms,
      activeRooms,
    };
  });
  return JSON.stringify(status);
});

export {
  lobbyAdminMatchResults,
  lobbyAdminRoomSeats,
  lobbyAdminRooms,
  lobbyAdminTickets,
  lobbyQueueSummary,
  lobbyRankedLeaderboard,
  myLobbyRatings,
  myLobbyRoomSeats,
  myLobbyRooms,
  myLobbyTickets,
} from './views';
