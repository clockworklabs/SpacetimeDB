import { Timestamp } from 'spacetimedb';
import { Range, SenderError, type Infer } from 'spacetimedb/server';
import {
  RoomStatus,
  SeatStatus,
  TicketStatus,
  lobbyQueueTicket,
  lobbyRoom,
  lobbyStatus,
  lobbySubjectRating,
  lobbySweepTick,
  spacetimedb,
  t,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema';
import { errors } from '../errors';
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
const MAX_TICKET_ID_LENGTH = 200;
const MAX_JSON_LENGTH = 4096;
const MAX_TTL_SECONDS = 24 * 60 * 60;
const MAX_MATCH_SIZE = 128;
const MAX_MATCH_CANDIDATES = 5000;
const SWEEP_BATCH = 500;
const ONE_SECOND_MICROS = 1_000_000n;

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
  /** Rating pool the result updates. Defaults to `pool`. */
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
  /** Omit for a draw. */
  winnerSubject?: string | undefined;
};

export type JoinQueueResult = {
  ticketId: string;
  roomId?: bigint | undefined;
};

function fail(code: string): never {
  throw new SenderError(code);
}

function normalizeName(value: string, code: string, max: number): string {
  const out = value.trim();
  if (!out || out.length > max) fail(code);
  return out;
}

function validateAttributesJson(value: string | undefined): string | undefined {
  const out = value?.trim();
  if (!out) return undefined;
  if (out.length > MAX_JSON_LENGTH) fail(errors.invalidAttributesJson);
  let parsed: unknown;
  try {
    parsed = JSON.parse(out);
  } catch {
    fail(errors.invalidAttributesJson);
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    fail(errors.invalidAttributesJson);
  }
  return out;
}

function ratingId(pool: string, subject: string): string {
  return lobbyCompositeKey(pool, subject);
}

function getOrCreateRating(ctx: WriteCtx, pool: string, subject: string) {
  const existing = ctx.db.lobbySubjectRating.ratingId.find(
    ratingId(pool, subject)
  );
  if (existing) return existing;
  return ctx.db.lobbySubjectRating.insert({
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
  });
}

function getConfig(ctx: WriteCtx) {
  const config = ctx.db.lobbyConfig.singleton.find(true);
  if (!config) fail(errors.configMissing);
  return config;
}

function isAdmin(ctx: WriteCtx | ViewModuleCtx): boolean {
  return ctx.db.lobbyAdminIdentity.identity.find(ctx.sender) != null;
}

function requireAdmin(ctx: WriteCtx): void {
  if (!isAdmin(ctx)) fail(errors.notAuthorized);
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

function seatsForRoom(ctx: WriteCtx, roomId: bigint) {
  return take(ctx.db.lobbyRoomSeat.byRoom.filter(roomId), MAX_MATCH_SIZE);
}

function findSeat(ctx: WriteCtx, roomId: bigint, subject: string) {
  for (const seat of ctx.db.lobbyRoomSeat.byRoomSubject.filter([
    roomId,
    subject,
  ]))
    return seat;
  return undefined;
}

function finishRoom(
  ctx: WriteCtx,
  room: RoomRow,
  status: typeof RoomStatus.Closed | typeof RoomStatus.Abandoned
): void {
  ctx.db.lobbyRoom.roomId.update({
    ...room,
    status,
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
  const now = ctx.timestamp.microsSinceUnixEpoch;
  const queued = take(
    ctx.db.lobbyQueueTicket.byPoolStatusCreatedAt.filter([
      pool,
      TicketStatus.Queued,
      new Range(),
    ]),
    MAX_MATCH_CANDIDATES
  ).filter(
    ticket =>
      ticket.ranked === ranked &&
      ticket.matchSize === matchSize &&
      ticket.expiresAtMicros > now
  );
  if (queued.length < matchSize) return undefined;

  const selected = ranked
    ? rankedSelection(queued, matchSize, now)
    : queued.slice(0, matchSize);
  if (!selected) return undefined;

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

function enqueue(
  ctx: WriteCtx,
  args: JoinQueueArgs,
  ranked: { ratingPool: string } | undefined
): JoinQueueResult {
  const config = getConfig(ctx);
  const pool = normalizeName(args.pool, errors.invalidPool, MAX_POOL_LENGTH);
  const subject = normalizeName(
    args.subject,
    errors.invalidSubject,
    MAX_SUBJECT_LENGTH
  );
  const matchSize = args.matchSize;
  if (
    !Number.isInteger(matchSize) ||
    matchSize < 1 ||
    matchSize > config.maxMatchSize
  ) {
    fail(errors.invalidMatchSize);
  }
  const ttlSeconds = args.ttlSeconds ?? config.defaultTicketTtlSeconds;
  if (
    !Number.isInteger(ttlSeconds) ||
    ttlSeconds < 1 ||
    ttlSeconds > MAX_TTL_SECONDS
  ) {
    fail(errors.invalidTtlSeconds);
  }
  const attributesJson = validateAttributesJson(args.attributesJson);
  const ratingPool =
    ranked &&
    normalizeName(ranked.ratingPool, errors.invalidPool, MAX_POOL_LENGTH);

  // A subject waits in one queue at a time.
  for (const ticket of [
    ...ctx.db.lobbyQueueTicket.bySubjectStatus.filter([
      subject,
      TicketStatus.Queued,
    ]),
  ]) {
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
    ranked: ranked !== undefined,
    rating: ratingPool
      ? (ctx.db.lobbySubjectRating.ratingId.find(ratingId(ratingPool, subject))
          ?.rating ?? DEFAULT_RATING)
      : undefined,
    ratingPool,
    partyId: undefined,
    attributesJson,
    roomId: undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    expiresAtMicros:
      ctx.timestamp.microsSinceUnixEpoch +
      BigInt(ttlSeconds) * ONE_SECOND_MICROS,
  });

  const roomId = attemptMatch(ctx, pool, matchSize, ranked !== undefined);
  return { ticketId, roomId };
}

/** Queues `subject` for an unranked match, replacing its queued ticket. */
export function joinQueueForSubject(
  ctx: WriteCtx,
  args: JoinQueueArgs
): JoinQueueResult {
  return enqueue(ctx, args, undefined);
}

/**
 * Queues `subject` for a ranked two-player match. Rating pools appear on the
 * public leaderboard, so only the host chooses them.
 */
export function joinRankedQueueForSubject(
  ctx: WriteCtx,
  args: JoinRankedQueueArgs
): JoinQueueResult {
  return enqueue(ctx, args, { ratingPool: args.ratingPool ?? args.pool });
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

/**
 * Records the result of an active two-player room, updates both Elo ratings,
 * and closes the room. The host determines the winner from its own game
 * state; seats that left still receive the result.
 */
export function reportMatchResult(
  ctx: WriteCtx,
  args: ReportMatchResultArgs
): void {
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail(errors.roomNotFound);
  if (room.status.tag !== RoomStatus.Active.tag) fail(errors.roomNotActive);
  const seats = seatsForRoom(ctx, args.roomId);
  if (seats.length !== 2) fail(errors.resultRequiresTwoSeats);

  const [seatA, seatB] = seats.sort((a, b) => a.seatIndex - b.seatIndex);
  const winnerSubject =
    args.winnerSubject === undefined
      ? undefined
      : normalizeName(
          args.winnerSubject,
          errors.invalidWinnerSubject,
          MAX_SUBJECT_LENGTH
        );
  if (
    winnerSubject !== undefined &&
    winnerSubject !== seatA.subject &&
    winnerSubject !== seatB.subject
  ) {
    fail(errors.winnerNotInRoom);
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
  finishRoom(ctx, room, RoomStatus.Closed);
}

export function cancelTicketForSubject(
  ctx: WriteCtx,
  args: TicketSubjectArgs
): void {
  const ticketId = normalizeName(
    args.ticketId,
    errors.invalidTicketId,
    MAX_TICKET_ID_LENGTH
  );
  const ticket = ctx.db.lobbyQueueTicket.ticketId.find(ticketId);
  if (!ticket) fail(errors.ticketNotFound);
  if (ticket.subject !== args.subject) fail(errors.notTicketOwner);
  if (ticket.status.tag !== TicketStatus.Queued.tag) {
    fail(errors.ticketNotQueued);
  }
  ctx.db.lobbyQueueTicket.ticketId.update({
    ...ticket,
    status: TicketStatus.Cancelled,
    updatedAt: ctx.timestamp,
  });
}

/** Marks `subject`'s reserved seat joined. The room activates once every seat joins. */
export function joinRoomForSubject(ctx: WriteCtx, args: RoomSubjectArgs): void {
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail(errors.roomNotFound);
  if (isTerminalRoom(room)) fail(errors.roomClosed);
  const seat = findSeat(ctx, args.roomId, args.subject);
  if (!seat) fail(errors.seatNotFound);
  if (seat.status.tag === SeatStatus.Left.tag) fail(errors.seatLeft);
  if (seat.status.tag === SeatStatus.Joined.tag) return;
  ctx.db.lobbyRoomSeat.seatId.update({
    ...seat,
    status: SeatStatus.Joined,
    joinedAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  });
  if (
    room.status.tag === RoomStatus.Ready.tag &&
    seatsForRoom(ctx, args.roomId).every(
      other => other.status.tag === SeatStatus.Joined.tag
    )
  ) {
    ctx.db.lobbyRoom.roomId.update({
      ...room,
      status: RoomStatus.Active,
      updatedAt: ctx.timestamp,
    });
  }
}

/**
 * Marks `subject`'s seat left. Leaving a ready room abandons it. An active
 * room stays active, so the host can still report its result, until every
 * seat has left.
 */
export function leaveRoomForSubject(
  ctx: WriteCtx,
  args: RoomSubjectArgs
): void {
  const room = ctx.db.lobbyRoom.roomId.find(args.roomId);
  if (!room) fail(errors.roomNotFound);
  const seat = findSeat(ctx, args.roomId, args.subject);
  if (!seat) fail(errors.seatNotFound);
  if (seat.status.tag === SeatStatus.Left.tag || isTerminalRoom(room)) return;
  ctx.db.lobbyRoomSeat.seatId.update({
    ...seat,
    status: SeatStatus.Left,
    leftAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  });
  if (
    room.status.tag === RoomStatus.Ready.tag ||
    seatsForRoom(ctx, args.roomId).every(
      other => other.status.tag === SeatStatus.Left.tag
    )
  ) {
    finishRoom(ctx, room, RoomStatus.Abandoned);
  }
}

/** Closes a room without a result. Use `reportMatchResult` to finish a ranked match. */
export function closeRoom(ctx: WriteCtx, roomId: bigint): void {
  const room = ctx.db.lobbyRoom.roomId.find(roomId);
  if (!room) fail(errors.roomNotFound);
  if (isTerminalRoom(room)) return;
  finishRoom(ctx, room, RoomStatus.Closed);
}

function olderThan(ctx: WriteCtx, seconds: number) {
  return new Range(undefined, {
    tag: 'included',
    value: new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch - BigInt(seconds) * ONE_SECOND_MICROS
    ),
  });
}

function deleteRoom(ctx: WriteCtx, room: RoomRow): void {
  for (const seat of seatsForRoom(ctx, room.roomId)) {
    if (seat.ticketId !== undefined) {
      ctx.db.lobbyQueueTicket.ticketId.delete(seat.ticketId);
    }
    ctx.db.lobbyRoomSeat.delete(seat);
  }
  ctx.db.lobbyRoom.delete(room);
}

/** One bounded cleanup pass. Runs on the schedule `install` creates. */
function sweep(ctx: WriteCtx): void {
  const config = getConfig(ctx);
  const now = ctx.timestamp.microsSinceUnixEpoch;

  for (const ticket of take(
    ctx.db.lobbyQueueTicket.byStatusExpiresAt.filter([
      TicketStatus.Queued,
      new Range(undefined, { tag: 'included', value: now }),
    ]),
    SWEEP_BATCH
  )) {
    ctx.db.lobbyQueueTicket.ticketId.update({
      ...ticket,
      status: TicketStatus.Expired,
      updatedAt: ctx.timestamp,
    });
  }

  const retained = olderThan(ctx, config.retentionSeconds);
  for (const status of [TicketStatus.Cancelled, TicketStatus.Expired]) {
    for (const ticket of take(
      ctx.db.lobbyQueueTicket.byStatusUpdatedAt.filter([status, retained]),
      SWEEP_BATCH
    )) {
      ctx.db.lobbyQueueTicket.delete(ticket);
    }
  }

  for (const room of take(
    ctx.db.lobbyRoom.byStatusUpdatedAt.filter([
      RoomStatus.Ready,
      olderThan(ctx, config.readyTimeoutSeconds),
    ]),
    SWEEP_BATCH
  )) {
    finishRoom(ctx, room, RoomStatus.Abandoned);
  }

  for (const status of [RoomStatus.Closed, RoomStatus.Abandoned]) {
    for (const room of take(
      ctx.db.lobbyRoom.byStatusUpdatedAt.filter([status, retained]),
      SWEEP_BATCH
    )) {
      deleteRoom(ctx, room);
    }
  }

  for (const result of take(
    ctx.db.lobbyMatchResult.byReportedAt.filter(retained),
    SWEEP_BATCH
  )) {
    ctx.db.lobbyMatchResult.delete(result);
  }
}

export const joinQueue = spacetimedb.reducer(
  {
    pool: t.string(),
    matchSize: t.u32(),
    attributesJson: t.option(t.string()),
    ttlSeconds: t.option(t.u32()),
  },
  (ctx, args) => {
    joinQueueForSubject(ctx, { ...args, subject: ctx.sender.toHexString() });
  }
);

export const cancelTicket = spacetimedb.reducer(
  { ticketId: t.string() },
  (ctx, args) => {
    cancelTicketForSubject(ctx, {
      ticketId: args.ticketId,
      subject: ctx.sender.toHexString(),
    });
  }
);

export const joinRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    joinRoomForSubject(ctx, {
      roomId: args.roomId,
      subject: ctx.sender.toHexString(),
    });
  }
);

export const leaveRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    leaveRoomForSubject(ctx, {
      roomId: args.roomId,
      subject: ctx.sender.toHexString(),
    });
  }
);

export const setRating = spacetimedb.reducer(
  {
    pool: t.string(),
    subject: t.string(),
    rating: t.i32(),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    const pool = normalizeName(args.pool, errors.invalidPool, MAX_POOL_LENGTH);
    const subject = normalizeName(
      args.subject,
      errors.invalidSubject,
      MAX_SUBJECT_LENGTH
    );
    if (args.rating < MIN_RATING || args.rating > MAX_RATING) {
      fail(errors.invalidRating);
    }
    const existing = getOrCreateRating(ctx, pool, subject);
    ctx.db.lobbySubjectRating.ratingId.update({
      ...existing,
      rating: args.rating,
      ratingOrder: BigInt(-args.rating),
      updatedAt: ctx.timestamp,
    });
  }
);

export const updateConfig = spacetimedb.reducer(
  {
    defaultTicketTtlSeconds: t.u32(),
    maxMatchSize: t.u32(),
    readyTimeoutSeconds: t.u32(),
    retentionSeconds: t.u32(),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    if (
      args.defaultTicketTtlSeconds < 1 ||
      args.defaultTicketTtlSeconds > MAX_TTL_SECONDS ||
      args.maxMatchSize < 1 ||
      args.maxMatchSize > MAX_MATCH_SIZE ||
      args.readyTimeoutSeconds < 1 ||
      args.retentionSeconds < 1
    ) {
      fail(errors.invalidConfig);
    }
    ctx.db.lobbyConfig.singleton.update({
      ...getConfig(ctx),
      ...args,
      updatedAt: ctx.timestamp,
    });
  }
);

export const addLobbyAdmin = spacetimedb.reducer(
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

export const removeLobbyAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    const row = ctx.db.lobbyAdminIdentity.identity.find(args.identity);
    if (!row) return;
    if (ctx.db.lobbyAdminIdentity.count() <= 1n) {
      fail(errors.cannotRemoveLastAdmin);
    }
    ctx.db.lobbyAdminIdentity.delete(row);
  }
);

export const lobbySweep = spacetimedb.reducer(
  { onSchedule: lobbySweepTick },
  { arg: lobbySweepTick.rowType },
  ctx => {
    sweep(ctx);
  }
);

export const getLobbyStatus = spacetimedb.procedure({}, lobbyStatus, ctx =>
  ctx.withTx(tx => {
    const config = getConfig(tx);
    return {
      defaultTicketTtlSeconds: config.defaultTicketTtlSeconds,
      maxMatchSize: config.maxMatchSize,
      queuedTickets: countUpTo(
        tx.db.lobbyQueueTicket.byStatus.filter(TicketStatus.Queued),
        MAX_MATCH_CANDIDATES
      ),
      readyRooms: countUpTo(
        tx.db.lobbyRoom.byStatus.filter(RoomStatus.Ready),
        MAX_MATCH_CANDIDATES
      ),
      activeRooms: countUpTo(
        tx.db.lobbyRoom.byStatus.filter(RoomStatus.Active),
        MAX_MATCH_CANDIDATES
      ),
    };
  })
);

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
