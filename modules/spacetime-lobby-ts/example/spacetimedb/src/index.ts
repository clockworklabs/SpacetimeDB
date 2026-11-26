import { ScheduleAt, Timestamp } from 'spacetimedb';
import { Range, SenderError, t } from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

import {
  DUEL_POOL,
  AI_DUEL_POOL,
  AI_SUBJECT,
  RATING_POOL,
  MATCH_SIZE,
  DISPLAY_NAME_MAX,
  shipClass,
  ShipClass,
  maneuverSlot,
  ManeuverSlot,
  DuelStatus,
  spacetimedb,
  forfeitCheck,
  duelSweepTick,
  type WriteCtx,
  type CombatantRow,
  type ManeuverRow,
  type DuelRow,
  type ShipClassValue,
  type ManeuverSlotValue,
} from './schema';
import { MANEUVER_CATALOG, SHIP_CATALOG } from './catalog';
export { default } from './schema';
export * from './views';

const ONE_SECOND_MICROS = 1_000_000n;
// A pilot who stays disconnected this long forfeits their unfinished duels.
const DISCONNECT_GRACE_SECONDS = 30n;
const SWEEP_INTERVAL_SECONDS = 5n;
const SWEEP_BATCH = 200;

function fail(message: string): never {
  throw new SenderError(`duel.${message}`);
}

function subjectFor(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

function displaySubject(subject: string): string {
  return `Pilot ${subject.slice(0, 6).toUpperCase()}`;
}

function isDuelPool(pool: string): boolean {
  return pool === DUEL_POOL || pool === AI_DUEL_POOL;
}

function isFinished(d: DuelRow): boolean {
  return (
    d.status.tag === DuelStatus.Complete.tag ||
    d.status.tag === DuelStatus.Abandoned.tag
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

function normalizeDisplayName(value: string): string {
  const out = value.trim().replace(/\s+/g, ' ');
  if (!out) fail('invalid_display_name');
  return out.slice(0, DISPLAY_NAME_MAX);
}

function combatantId(roomId: bigint, subject: string): string {
  return `${roomId.toString()}:${subject}`;
}

function choiceId(roomId: bigint, round: number, subject: string): string {
  return `${roomId.toString()}:${round}:${subject}`;
}

function maneuverId(ship: ShipClassValue, slot: ManeuverSlotValue): string {
  return `${ship.tag}:${slot.tag}`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function shipStats(ctx: WriteCtx, cls: ShipClassValue) {
  const row = ctx.db.shipCatalog.shipId.find(cls.tag);
  if (!row) fail('ship_catalog_missing');
  return row;
}

function maneuverFor(
  ctx: WriteCtx,
  ship: ShipClassValue,
  slot: ManeuverSlotValue
) {
  const row = ctx.db.maneuverCatalog.maneuverId.find(maneuverId(ship, slot));
  if (!row) fail('maneuver_missing');
  return row;
}

function choiceFor(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  subject: string
) {
  return ctx.db.duelManeuver.choiceId.find(choiceId(roomId, round, subject));
}

function upsertManeuverChoice(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  subject: string,
  slot: ManeuverSlotValue,
  ship: ShipClassValue
) {
  const id = choiceId(roomId, round, subject);
  const maneuver = maneuverFor(ctx, ship, slot);
  const row = {
    choiceId: id,
    roomId,
    round,
    subject,
    slot,
    maneuverId: maneuver.maneuverId,
    chosenAt: ctx.timestamp,
  };
  if (ctx.db.duelManeuver.choiceId.find(id))
    ctx.db.duelManeuver.choiceId.update(row);
  else ctx.db.duelManeuver.insert(row);
  return row;
}

function seedCatalog(ctx: WriteCtx): void {
  for (const row of SHIP_CATALOG) {
    if (ctx.db.shipCatalog.shipId.find(row.shipId))
      ctx.db.shipCatalog.shipId.update(row);
    else ctx.db.shipCatalog.insert(row);
  }
  for (const row of MANEUVER_CATALOG) {
    if (ctx.db.maneuverCatalog.maneuverId.find(row.maneuverId))
      ctx.db.maneuverCatalog.maneuverId.update(row);
    else ctx.db.maneuverCatalog.insert(row);
  }
}

function ensurePilot(ctx: WriteCtx, subject: string) {
  const existing = ctx.db.pilot.subject.find(subject);
  if (existing) return existing;
  const row = {
    subject,
    displayName: displaySubject(subject),
    shipClass: ShipClass.Interceptor,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  };
  ctx.db.pilot.insert(row);
  return row;
}

// Every AI match uses one shared AI pilot and pool, so public views never
// reveal which player is facing the AI. Its ship is picked per match.
function ensureAiPilot(ctx: WriteCtx) {
  const existing = ensurePilot(ctx, AI_SUBJECT);
  const ships = Object.values(ShipClass);
  const next = {
    ...existing,
    displayName: 'Arena AI',
    shipClass: ships[ctx.random.integerInRange(0, ships.length - 1)],
    updatedAt: ctx.timestamp,
  };
  ctx.db.pilot.subject.update(next);
  return next;
}

function seatsForRoom(ctx: WriteCtx, roomId: bigint) {
  return [...ctx.db.lobby.lobbyRoomSeat.byRoom.filter(roomId)];
}

function hasSeat(ctx: WriteCtx, roomId: bigint, subject: string): boolean {
  return [...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subject)].some(
    seat => seat.roomId === roomId
  );
}

function roomFor(ctx: WriteCtx, roomId: bigint) {
  return ctx.db.lobby.lobbyRoom.roomId.find(roomId);
}

function isOpenRoom(room: ReturnType<typeof roomFor>): boolean {
  return (
    room?.status.tag === lobby.RoomStatus.Ready.tag ||
    room?.status.tag === lobby.RoomStatus.Active.tag
  );
}

function hasOpenSeat(ctx: WriteCtx, subject: string): boolean {
  return [...ctx.db.lobby.lobbyRoomSeat.bySubject.filter(subject)].some(
    seat =>
      seat.status.tag !== lobby.SeatStatus.Left.tag &&
      isOpenRoom(roomFor(ctx, seat.roomId))
  );
}

function log(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  message: string
): void {
  ctx.db.duelRoundLog.insert({
    logId: 0n,
    roomId,
    round,
    message,
    createdAt: ctx.timestamp,
  });
}

function ensureCombatant(ctx: WriteCtx, roomId: bigint, subject: string) {
  const id = combatantId(roomId, subject);
  const existing = ctx.db.duelCombatant.combatantId.find(id);
  if (existing) return existing;
  const p = ensurePilot(ctx, subject);
  const stats = shipStats(ctx, p.shipClass);
  const row = {
    combatantId: id,
    roomId,
    subject,
    displayName: p.displayName,
    shipClass: p.shipClass,
    hull: stats.hull,
    maxHull: stats.hull,
    shields: stats.shields,
    maxShields: stats.shields,
    attack: stats.attack,
    defense: stats.defense,
    speed: stats.speed,
    critBps: stats.critBps,
    dodgeBps: stats.dodgeBps,
    updatedAt: ctx.timestamp,
  };
  ctx.db.duelCombatant.insert(row);
  return row;
}

function ensureDuelForRoom(ctx: WriteCtx, roomId: bigint) {
  const existing = ctx.db.duel.roomId.find(roomId);
  if (existing) return existing;
  const room = roomFor(ctx, roomId);
  if (!room || !isDuelPool(room.pool)) fail('room_not_found');
  const seats = seatsForRoom(ctx, roomId);
  if (seats.length < MATCH_SIZE) fail('room_not_ready');
  const row = {
    roomId,
    status: DuelStatus.Configuring,
    round: 0,
    winnerSubject: undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  };
  ctx.db.duel.insert(row);
  for (const seat of seats.slice(0, MATCH_SIZE)) {
    ensureCombatant(ctx, roomId, seat.subject);
  }
  log(ctx, roomId, 0, 'Match found. Pilots are docking into the arena.');
  return row;
}

function refreshDuelStatus(ctx: WriteCtx, roomId: bigint) {
  const current = ensureDuelForRoom(ctx, roomId);
  if (current.status.tag !== DuelStatus.Configuring.tag) return current;
  const seats = seatsForRoom(ctx, roomId);
  const allJoined =
    seats.length >= MATCH_SIZE &&
    seats.every(seat => seat.status.tag === lobby.SeatStatus.Joined.tag);
  if (!allJoined) return current;
  const updated = {
    ...current,
    status: DuelStatus.Active,
    updatedAt: ctx.timestamp,
  };
  ctx.db.duel.roomId.update(updated);
  log(ctx, roomId, 0, 'Both pilots joined. Duel is live.');
  return updated;
}

// Rolls come from the transaction's RNG, so a player cannot compute them
// before submitting a maneuver.
function roll(ctx: WriteCtx): number {
  return ctx.random.integerInRange(0, 9_999);
}

function requireMapped<T>(
  values: Map<string, T>,
  key: string,
  reason: string
): T {
  const value = values.get(key);
  if (value === undefined) fail(reason);
  return value;
}

function applyDamage(target: CombatantRow, amount: number): CombatantRow {
  let remaining = Math.max(0, Math.floor(amount));
  const shieldDamage = Math.min(target.shields, remaining);
  remaining -= shieldDamage;
  return {
    ...target,
    shields: target.shields - shieldDamage,
    hull: Math.max(0, target.hull - remaining),
  };
}

function applyManeuverSetup(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  combatant: CombatantRow,
  maneuver: ManeuverRow
): CombatantRow {
  let next = combatant;
  if (maneuver.selfShieldCost > 0) {
    const cost = Math.min(next.shields, maneuver.selfShieldCost);
    next = { ...next, shields: next.shields - cost };
    if (cost > 0)
      log(
        ctx,
        roomId,
        round,
        `${next.displayName} burns ${cost} shields to power ${maneuver.name}.`
      );
  }
  if (maneuver.shieldRestore > 0 && next.shields < next.maxShields) {
    const restored = Math.min(
      maneuver.shieldRestore,
      next.maxShields - next.shields
    );
    next = { ...next, shields: next.shields + restored };
    if (restored > 0)
      log(
        ctx,
        roomId,
        round,
        `${next.displayName} restores ${restored} shields with ${maneuver.name}.`
      );
  }
  return next;
}

function attackOnce(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  attacker: CombatantRow,
  defender: CombatantRow,
  attackerMove: ManeuverRow,
  defenderMove: ManeuverRow
): CombatantRow {
  if (attacker.hull <= 0 || defender.hull <= 0) return defender;
  const dodgeBps = clamp(
    defender.dodgeBps + defenderMove.dodgeBonusBps,
    0,
    9000
  );
  if (roll(ctx) < dodgeBps) {
    log(
      ctx,
      roomId,
      round,
      `${defender.displayName}'s ${defender.shipClass.tag} evades ${attackerMove.name}.`
    );
    return defender;
  }
  const critBps = clamp(attacker.critBps + attackerMove.critBonusBps, 0, 9000);
  const crit = roll(ctx) < critBps;
  const baseDamage = Math.max(1, attacker.attack - defender.defense);
  const attackDamage = Math.max(
    1,
    Math.floor((baseDamage * Math.max(0, attackerMove.damageBps)) / 10_000)
  );
  const defenseBps = clamp(defenderMove.defenseBps, -5000, 8500);
  const mitigated = Math.max(
    1,
    Math.floor((attackDamage * (10_000 - defenseBps)) / 10_000)
  );
  const damage = crit ? Math.floor(mitigated * 1.75) : mitigated;
  const updated = applyDamage(defender, damage);
  log(
    ctx,
    roomId,
    round,
    `${attacker.displayName} uses ${attackerMove.name} on ${defender.displayName} for ${damage}${crit ? ' critical' : ''} damage.`
  );
  return updated;
}

// The AI picks from the RNG of the transaction that resolves the round, after
// the player's choice is final, so the player cannot predict it.
function ensureAiChoice(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  combatants: CombatantRow[]
): void {
  const ai = combatants.find(c => c.subject === AI_SUBJECT);
  if (!ai || choiceFor(ctx, roomId, round, AI_SUBJECT)) return;
  const slots = Object.values(ManeuverSlot);
  upsertManeuverChoice(
    ctx,
    roomId,
    round,
    AI_SUBJECT,
    slots[ctx.random.integerInRange(0, slots.length - 1)],
    ai.shipClass
  );
}

/**
 * Marks the duel won and finishes its lobby room. Ranked rooms report the
 * result so both ratings update; AI rooms close without a rating change.
 */
function finishDuel(
  ctx: WriteCtx,
  d: DuelRow,
  round: number,
  winnerSubject: string,
  message: string
): void {
  ctx.db.duel.roomId.update({
    ...d,
    status: DuelStatus.Complete,
    round,
    winnerSubject,
    updatedAt: ctx.timestamp,
  });
  log(ctx, d.roomId, round, message);
  if (roomFor(ctx, d.roomId)?.pool === AI_DUEL_POOL) {
    lobby.closeRoom(ctx.as.lobby, d.roomId);
  } else {
    lobby.reportMatchResult(ctx.as.lobby, { roomId: d.roomId, winnerSubject });
  }
}

function abandonDuel(ctx: WriteCtx, d: DuelRow, message: string): void {
  ctx.db.duel.roomId.update({
    ...d,
    status: DuelStatus.Abandoned,
    updatedAt: ctx.timestamp,
  });
  log(ctx, d.roomId, d.round, message);
}

/**
 * Ends an unfinished duel on behalf of `subject`. In an active room the
 * opponent wins and the loss is recorded. A room that is not active yet is
 * left, which abandons it without a rating change.
 */
function forfeitDuel(ctx: WriteCtx, d: DuelRow, subject: string): void {
  const room = roomFor(ctx, d.roomId);
  const opponent = seatsForRoom(ctx, d.roomId).find(
    seat => seat.subject !== subject
  );
  if (room?.status.tag === lobby.RoomStatus.Active.tag && opponent) {
    const name = (s: string) =>
      ctx.db.duelCombatant.combatantId.find(combatantId(d.roomId, s))
        ?.displayName ?? 'A pilot';
    finishDuel(
      ctx,
      d,
      d.round,
      opponent.subject,
      `${name(subject)} forfeits. ${name(opponent.subject)} wins.`
    );
    return;
  }
  if (room?.status.tag === lobby.RoomStatus.Ready.tag) {
    lobby.leaveRoomForSubject(ctx.as.lobby, { roomId: d.roomId, subject });
  }
  abandonDuel(ctx, d, 'A pilot left. Duel abandoned.');
}

function maybeResolveRound(ctx: WriteCtx, roomId: bigint): void {
  const d = refreshDuelStatus(ctx, roomId);
  if (d.status.tag !== DuelStatus.Active.tag) return;
  const combatants = sortedCombatants(ctx, roomId);
  if (combatants.length < MATCH_SIZE) fail('combatants_missing');
  const round = d.round + 1;
  ensureAiChoice(ctx, roomId, round, combatants);
  const moves = new Map<string, ManeuverRow>();
  for (const combatant of combatants) {
    const choice = choiceFor(ctx, roomId, round, combatant.subject);
    if (!choice) return;
    const move = ctx.db.maneuverCatalog.maneuverId.find(choice.maneuverId);
    if (!move) fail('maneuver_missing');
    moves.set(combatant.subject, move);
  }

  log(
    ctx,
    roomId,
    round,
    `Round ${round}. ${combatants.map(c => `${c.displayName}: ${requireMapped(moves, c.subject, 'maneuver_missing').name}`).join(' | ')}`
  );
  const next = new Map<string, CombatantRow>();
  for (const combatant of combatants) {
    next.set(
      combatant.subject,
      applyManeuverSetup(
        ctx,
        roomId,
        round,
        combatant,
        requireMapped(moves, combatant.subject, 'maneuver_missing')
      )
    );
  }
  const ordered = [...next.values()].sort((a, b) => {
    if (a.speed !== b.speed) return b.speed - a.speed;
    return a.subject.localeCompare(b.subject);
  });

  const firstInitial = ordered[0];
  const secondInitial = ordered[1];
  if (!firstInitial || !secondInitial) fail('combatants_missing');
  let first = firstInitial;
  let second = secondInitial;
  second = attackOnce(
    ctx,
    roomId,
    round,
    first,
    second,
    requireMapped(moves, first.subject, 'maneuver_missing'),
    requireMapped(moves, second.subject, 'maneuver_missing')
  );
  next.set(second.subject, second);
  if (second.hull > 0) {
    first = requireMapped(next, first.subject, 'combatants_missing');
    second = requireMapped(next, second.subject, 'combatants_missing');
    first = attackOnce(
      ctx,
      roomId,
      round,
      second,
      first,
      requireMapped(moves, second.subject, 'maneuver_missing'),
      requireMapped(moves, first.subject, 'maneuver_missing')
    );
    next.set(first.subject, first);
  }

  const updatedCombatants = [...next.values()];
  for (const combatant of updatedCombatants) {
    ctx.db.duelCombatant.combatantId.update({
      ...combatant,
      updatedAt: ctx.timestamp,
    });
  }
  const alive = updatedCombatants.filter(c => c.hull > 0);
  if (alive.length === 1) {
    finishDuel(
      ctx,
      d,
      round,
      alive[0].subject,
      `${alive[0].displayName} wins the duel.`
    );
  } else if (alive.length === 0) {
    const winner =
      updatedCombatants[0].hull >= updatedCombatants[1].hull
        ? updatedCombatants[0]
        : updatedCombatants[1];
    finishDuel(
      ctx,
      d,
      round,
      winner.subject,
      `${winner.displayName} wins by emergency adjudication.`
    );
  } else {
    ctx.db.duel.roomId.update({ ...d, round, updatedAt: ctx.timestamp });
  }
}

function sortedCombatants(ctx: WriteCtx, roomId: bigint) {
  return [...ctx.db.duelCombatant.byRoom.filter(roomId)].sort((a, b) => {
    if (a.speed !== b.speed) return b.speed - a.speed;
    return a.subject.localeCompare(b.subject);
  });
}

export const setDisplayName = spacetimedb.reducer(
  { displayName: t.string() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    const existing = ensurePilot(ctx, subject);
    const displayName = normalizeDisplayName(args.displayName);
    ctx.db.pilot.subject.update({
      ...existing,
      displayName,
      updatedAt: ctx.timestamp,
    });
    for (const combatant of [
      ...ctx.db.duelCombatant.bySubject.filter(subject),
    ]) {
      const d = ctx.db.duel.roomId.find(combatant.roomId);
      if (d && d.status.tag === DuelStatus.Configuring.tag) {
        ctx.db.duelCombatant.combatantId.update({
          ...combatant,
          displayName,
          updatedAt: ctx.timestamp,
        });
      }
    }
  }
);

export const selectShip = spacetimedb.reducer({ shipClass }, (ctx, args) => {
  const subject = subjectFor(ctx);
  const existing = ensurePilot(ctx, subject);
  ctx.db.pilot.subject.update({
    ...existing,
    shipClass: args.shipClass,
    updatedAt: ctx.timestamp,
  });
  for (const combatant of [...ctx.db.duelCombatant.bySubject.filter(subject)]) {
    const d = ctx.db.duel.roomId.find(combatant.roomId);
    if (!d || d.status.tag !== DuelStatus.Configuring.tag) continue;
    const stats = shipStats(ctx, args.shipClass);
    ctx.db.duelCombatant.combatantId.update({
      ...combatant,
      shipClass: args.shipClass,
      hull: stats.hull,
      maxHull: stats.hull,
      shields: stats.shields,
      maxShields: stats.shields,
      attack: stats.attack,
      defense: stats.defense,
      speed: stats.speed,
      critBps: stats.critBps,
      dodgeBps: stats.dodgeBps,
      updatedAt: ctx.timestamp,
    });
  }
});

function forfeitOpenDuels(ctx: WriteCtx, subject: string): void {
  for (const combatant of [...ctx.db.duelCombatant.bySubject.filter(subject)]) {
    const d = ctx.db.duel.roomId.find(combatant.roomId);
    if (d && !isFinished(d)) forfeitDuel(ctx, d, subject);
  }
}

// Queueing again forfeits any duel the player has not finished.
export const findDuel = spacetimedb.reducer({}, ctx => {
  const subject = subjectFor(ctx);
  forfeitOpenDuels(ctx, subject);
  const p = ensurePilot(ctx, subject);
  const result = lobby.joinRankedQueueForSubject(ctx.as.lobby, {
    pool: DUEL_POOL,
    subject,
    matchSize: MATCH_SIZE,
    ratingPool: RATING_POOL,
    attributesJson: JSON.stringify({ shipClass: p.shipClass.tag }),
    ttlSeconds: 120,
  });
  if (result.roomId !== undefined) ensureDuelForRoom(ctx, result.roomId);
});

// Falls back to an AI opponent only while the player has no open seat, so a
// real match that formed just before the client's timer is kept.
export const fallbackToAi = spacetimedb.reducer({}, ctx => {
  const subject = subjectFor(ctx);
  if (hasOpenSeat(ctx, subject)) return;
  const p = ensurePilot(ctx, subject);
  const aiPilot = ensureAiPilot(ctx);
  // Queueing in the AI pool replaces the player's public ticket. AI rooms
  // close without a result, so the AI pool never gains ratings.
  lobby.joinRankedQueueForSubject(ctx.as.lobby, {
    pool: AI_DUEL_POOL,
    subject,
    matchSize: MATCH_SIZE,
    ratingPool: AI_DUEL_POOL,
    attributesJson: JSON.stringify({ shipClass: p.shipClass.tag }),
    ttlSeconds: 120,
  });
  const result = lobby.joinRankedQueueForSubject(ctx.as.lobby, {
    pool: AI_DUEL_POOL,
    subject: AI_SUBJECT,
    matchSize: MATCH_SIZE,
    ratingPool: AI_DUEL_POOL,
    attributesJson: JSON.stringify({ shipClass: aiPilot.shipClass.tag }),
    ttlSeconds: 120,
  });
  if (result.roomId === undefined) fail('ai_match_failed');
  lobby.joinRoomForSubject(ctx.as.lobby, { roomId: result.roomId, subject });
  lobby.joinRoomForSubject(ctx.as.lobby, {
    roomId: result.roomId,
    subject: AI_SUBJECT,
  });
  ensureDuelForRoom(ctx, result.roomId);
  refreshDuelStatus(ctx, result.roomId);
  log(
    ctx,
    result.roomId,
    0,
    'No rival found. Arena AI accepted the challenge.'
  );
});

export const joinDuelRoom = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    ensurePilot(ctx, subject);
    lobby.joinRoomForSubject(ctx.as.lobby, { roomId: args.roomId, subject });
    refreshDuelStatus(ctx, args.roomId);
  }
);

export const chooseManeuver = spacetimedb.reducer(
  { roomId: t.u64(), slot: maneuverSlot },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    if (!hasSeat(ctx, args.roomId, subject)) fail('not_in_room');
    const d = refreshDuelStatus(ctx, args.roomId);
    if (isFinished(d)) return;
    if (d.status.tag !== DuelStatus.Active.tag) fail('duel_not_ready');
    const round = d.round + 1;
    const combatant = ctx.db.duelCombatant.combatantId.find(
      combatantId(args.roomId, subject)
    );
    if (!combatant) fail('combatant_missing');
    upsertManeuverChoice(
      ctx,
      args.roomId,
      round,
      subject,
      args.slot,
      combatant.shipClass
    );
    maybeResolveRound(ctx, args.roomId);
  }
);

export const leaveDuel = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    const d = ctx.db.duel.roomId.find(args.roomId);
    if (!d || isFinished(d)) return;
    if (!hasSeat(ctx, args.roomId, subject)) fail('not_in_room');
    forfeitDuel(ctx, d, subject);
  }
);

function isConnected(ctx: WriteCtx, subject: string): boolean {
  return !ctx.db.pilotConnection.bySubject.filter(subject).next().done;
}

export const onConnect = spacetimedb.clientConnected(ctx => {
  if (!ctx.connectionId) return;
  ctx.db.pilotConnection.insert({
    connectionId: ctx.connectionId,
    subject: subjectFor(ctx),
  });
});

// When a pilot's last connection closes, a forfeit check runs after a grace
// period, so reloading the page does not forfeit the duel.
export const onDisconnect = spacetimedb.clientDisconnected(ctx => {
  if (ctx.connectionId) {
    ctx.db.pilotConnection.connectionId.delete(ctx.connectionId);
  }
  const subject = subjectFor(ctx);
  if (isConnected(ctx, subject)) return;
  ctx.db.forfeitCheck.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(
      ctx.timestamp.microsSinceUnixEpoch +
        DISCONNECT_GRACE_SECONDS * ONE_SECOND_MICROS
    ),
    subject,
  });
});

export const forfeitDisconnected = spacetimedb.reducer(
  { onSchedule: forfeitCheck },
  { arg: forfeitCheck.rowType },
  (ctx, { arg }) => {
    if (!ctx.sender.isEqual(ctx.databaseIdentity)) fail('not_authorized');
    const subject = arg.subject;
    if (isConnected(ctx, subject)) return;
    for (const ticket of [
      ...ctx.db.lobby.lobbyQueueTicket.bySubject.filter(subject),
    ]) {
      if (ticket.status.tag !== lobby.TicketStatus.Queued.tag) continue;
      lobby.cancelTicketForSubject(ctx.as.lobby, {
        ticketId: ticket.ticketId,
        subject,
      });
    }
    forfeitOpenDuels(ctx, subject);
  }
);

/**
 * Reconciles unfinished duels with lobby changes made outside the duel
 * reducers, then deletes finished duels once the lobby's retention period
 * has passed:
 * - a room that closed or timed out abandons its duel
 * - a player who left an active room through `lobby.leave_room` forfeits
 */
export const duelSweep = spacetimedb.reducer(
  { onSchedule: duelSweepTick },
  { arg: duelSweepTick.rowType },
  ctx => {
    for (const status of [DuelStatus.Configuring, DuelStatus.Active]) {
      for (const d of take(ctx.db.duel.byStatus.filter(status), SWEEP_BATCH)) {
        const room = roomFor(ctx, d.roomId);
        if (!isOpenRoom(room)) {
          abandonDuel(ctx, d, 'The match room closed. Duel abandoned.');
          continue;
        }
        if (room?.status.tag !== lobby.RoomStatus.Active.tag) continue;
        const left = seatsForRoom(ctx, d.roomId).find(
          seat => seat.status.tag === lobby.SeatStatus.Left.tag
        );
        if (left) forfeitDuel(ctx, d, left.subject);
      }
    }

    const config = ctx.db.lobby.lobbyConfig.singleton.find(true);
    if (!config) return;
    const retained = new Range<Timestamp>(undefined, {
      tag: 'included',
      value: new Timestamp(
        ctx.timestamp.microsSinceUnixEpoch -
          BigInt(config.retentionSeconds) * ONE_SECOND_MICROS
      ),
    });
    for (const status of [DuelStatus.Complete, DuelStatus.Abandoned]) {
      for (const d of take(
        ctx.db.duel.byStatusUpdatedAt.filter([status, retained]),
        SWEEP_BATCH
      )) {
        ctx.db.duelCombatant.byRoom.delete(d.roomId);
        ctx.db.duelRoundLog.byRoom.delete(d.roomId);
        ctx.db.duelManeuver.byRoom.delete(d.roomId);
        ctx.db.duel.roomId.delete(d.roomId);
      }
    }
  }
);

export const init = spacetimedb.init(ctx => {
  lobby.install(ctx.as.lobby);
  seedCatalog(ctx);
  ctx.db.duelSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(
      SWEEP_INTERVAL_SECONDS * ONE_SECOND_MICROS
    ),
  });
});
