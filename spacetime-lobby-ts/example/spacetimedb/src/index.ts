import { SenderError, t } from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

import {
  DUEL_POOL,
  AI_DUEL_POOL_PREFIX,
  RATING_POOL,
  MATCH_SIZE,
  DISPLAY_NAME_MAX,
  shipClass,
  ShipClass,
  maneuverSlot,
  ManeuverSlot,
  DuelStatus,
  spacetimedb,
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

function fail(message: string): never {
  throw new SenderError(`duel.${message}`);
}

function subjectFor(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

function displaySubject(subject: string): string {
  return `Pilot ${subject.slice(0, 6).toUpperCase()}`;
}

function aiSubjectFor(subject: string): string {
  return `ai:${subject}`;
}

function aiPoolFor(subject: string): string {
  return `${AI_DUEL_POOL_PREFIX}:${subject.slice(0, 32)}`;
}

function isDuelPool(pool: string): boolean {
  return pool === DUEL_POOL || pool.startsWith(`${AI_DUEL_POOL_PREFIX}:`);
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

function ensureAiPilot(ctx: WriteCtx, subject: string) {
  const existing = ensurePilot(ctx, subject);
  const seed = hashSeed(subject);
  const ship = [
    ShipClass.Bulwark,
    ShipClass.Interceptor,
    ShipClass.Phantom,
    ShipClass.Artillery,
  ][seed % 4];
  const displayName = 'Arena AI';
  const next = {
    ...existing,
    displayName,
    shipClass: ship,
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

function hashSeed(input: string): number {
  let h = 2166136261;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

function roll(seed: string): number {
  let x = hashSeed(seed) || 1;
  x ^= x << 13;
  x ^= x >>> 17;
  x ^= x << 5;
  return (x >>> 0) % 10_000;
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
  const prefix = `${roomId.toString()}:${round}:${attacker.subject}:${defender.subject}`;
  const dodgeBps = clamp(
    defender.dodgeBps + defenderMove.dodgeBonusBps,
    0,
    9000
  );
  if (roll(`${prefix}:dodge`) < dodgeBps) {
    log(
      ctx,
      roomId,
      round,
      `${defender.displayName}'s ${defender.shipClass.tag} evades ${attackerMove.name}.`
    );
    return defender;
  }
  const critBps = clamp(attacker.critBps + attackerMove.critBonusBps, 0, 9000);
  const crit = roll(`${prefix}:crit`) < critBps;
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

function aiSlotFor(
  roomId: bigint,
  round: number,
  subject: string
): ManeuverSlotValue {
  const slots = [
    ManeuverSlot.Primary,
    ManeuverSlot.Defensive,
    ManeuverSlot.Risky,
  ];
  return slots[
    hashSeed(`${roomId.toString()}:${round}:${subject}:ai-move`) % slots.length
  ];
}

function ensureAiChoices(
  ctx: WriteCtx,
  roomId: bigint,
  round: number,
  combatants: CombatantRow[]
): void {
  for (const combatant of combatants) {
    if (!combatant.subject.startsWith('ai:')) continue;
    if (choiceFor(ctx, roomId, round, combatant.subject)) continue;
    upsertManeuverChoice(
      ctx,
      roomId,
      round,
      combatant.subject,
      aiSlotFor(roomId, round, combatant.subject),
      combatant.shipClass
    );
  }
}

function completeDuel(
  ctx: WriteCtx,
  d: DuelRow,
  round: number,
  winnerSubject: string,
  reporterSubject: string
): void {
  ctx.db.duel.roomId.update({
    ...d,
    status: DuelStatus.Complete,
    round,
    winnerSubject,
    updatedAt: ctx.timestamp,
  });
  const winner = ctx.db.duelCombatant.combatantId.find(
    combatantId(d.roomId, winnerSubject)
  );
  log(
    ctx,
    d.roomId,
    round,
    `${winner?.displayName ?? 'A pilot'} wins the duel.`
  );
  lobby.reportMatchResult(ctx.as.lobby, {
    roomId: d.roomId,
    subject: reporterSubject,
    winnerSubject,
  });
  lobby.closeRoom(ctx.as.lobby, { roomId: d.roomId, subject: reporterSubject });
}

function maybeResolveRound(
  ctx: WriteCtx,
  roomId: bigint,
  reporterSubject: string
): void {
  const d = refreshDuelStatus(ctx, roomId);
  if (
    d.status.tag === DuelStatus.Complete.tag ||
    d.status.tag === DuelStatus.Abandoned.tag
  )
    return;
  if (d.status.tag !== DuelStatus.Active.tag) return;
  const combatants = sortedCombatants(ctx, roomId);
  if (combatants.length < MATCH_SIZE) fail('combatants_missing');
  const round = d.round + 1;
  ensureAiChoices(ctx, roomId, round, combatants);
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
    completeDuel(ctx, d, round, alive[0].subject, reporterSubject);
  } else if (alive.length === 0) {
    const winner =
      updatedCombatants[0].hull >= updatedCombatants[1].hull
        ? updatedCombatants[0]
        : updatedCombatants[1];
    ctx.db.duel.roomId.update({
      ...d,
      status: DuelStatus.Complete,
      round,
      winnerSubject: winner.subject,
      updatedAt: ctx.timestamp,
    });
    log(
      ctx,
      roomId,
      round,
      `${winner.displayName} wins by emergency adjudication.`
    );
    lobby.reportMatchResult(ctx.as.lobby, {
      roomId,
      subject: reporterSubject,
      winnerSubject: winner.subject,
    });
    lobby.closeRoom(ctx.as.lobby, { roomId, subject: reporterSubject });
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

export const set_display_name = spacetimedb.reducer(
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

export const select_ship = spacetimedb.reducer({ shipClass }, (ctx, args) => {
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

export const find_duel = spacetimedb.reducer({}, ctx => {
  const subject = subjectFor(ctx);
  const p = ensurePilot(ctx, subject);
  const result = lobby.joinRankedQueue(ctx.as.lobby, {
    pool: DUEL_POOL,
    subject,
    matchSize: MATCH_SIZE,
    ratingPool: RATING_POOL,
    attributesJson: JSON.stringify({ shipClass: p.shipClass.tag }),
    ttlSeconds: 120,
  });
  if (result.roomId !== undefined) ensureDuelForRoom(ctx, result.roomId);
});

export const fallback_to_ai = spacetimedb.reducer({}, ctx => {
  const subject = subjectFor(ctx);
  const p = ensurePilot(ctx, subject);
  const publicTickets = [
    ...ctx.db.lobby.lobbyQueueTicket.bySubject.filter(subject),
  ].filter(
    ticket =>
      ticket.pool === DUEL_POOL &&
      ticket.status.tag === lobby.TicketStatus.Queued.tag
  );
  for (const ticket of publicTickets) {
    lobby.cancelTicket(ctx.as.lobby, { ticketId: ticket.ticketId, subject });
  }

  const aiSubject = aiSubjectFor(subject);
  const aiPilot = ensureAiPilot(ctx, aiSubject);
  const pool = aiPoolFor(subject);
  lobby.joinRankedQueue(ctx.as.lobby, {
    pool,
    subject,
    matchSize: MATCH_SIZE,
    ratingPool: RATING_POOL,
    attributesJson: JSON.stringify({
      shipClass: p.shipClass.tag,
      fallback: 'human',
    }),
    ttlSeconds: 120,
  });
  const result = lobby.joinRankedQueue(ctx.as.lobby, {
    pool,
    subject: aiSubject,
    matchSize: MATCH_SIZE,
    ratingPool: RATING_POOL,
    attributesJson: JSON.stringify({
      shipClass: aiPilot.shipClass.tag,
      fallback: 'ai',
    }),
    ttlSeconds: 120,
  });
  if (result.roomId === undefined) fail('ai_match_failed');
  lobby.joinRoom(ctx.as.lobby, { roomId: result.roomId, subject });
  lobby.joinRoom(ctx.as.lobby, { roomId: result.roomId, subject: aiSubject });
  ensureDuelForRoom(ctx, result.roomId);
  refreshDuelStatus(ctx, result.roomId);
  log(
    ctx,
    result.roomId,
    0,
    'No rival found. Arena AI accepted the challenge.'
  );
});

export const join_duel_room = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    ensurePilot(ctx, subject);
    lobby.joinRoom(ctx.as.lobby, { roomId: args.roomId, subject });
    refreshDuelStatus(ctx, args.roomId);
  }
);

export const choose_maneuver = spacetimedb.reducer(
  { roomId: t.u64(), slot: maneuverSlot },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    if (!hasSeat(ctx, args.roomId, subject)) fail('not_in_room');
    const d = refreshDuelStatus(ctx, args.roomId);
    if (d.status.tag === DuelStatus.Complete.tag) return;
    if (d.status.tag === DuelStatus.Abandoned.tag) fail('duel_abandoned');
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
    maybeResolveRound(ctx, args.roomId, subject);
  }
);

export const advance_duel = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    const combatant = ctx.db.duelCombatant.combatantId.find(
      combatantId(args.roomId, subject)
    );
    if (!combatant) fail('combatant_missing');
    upsertManeuverChoice(
      ctx,
      args.roomId,
      (ctx.db.duel.roomId.find(args.roomId)?.round ?? 0) + 1,
      subject,
      ManeuverSlot.Primary,
      combatant.shipClass
    );
    maybeResolveRound(ctx, args.roomId, subject);
  }
);

export const leave_duel = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    const d = ctx.db.duel.roomId.find(args.roomId);
    if (!d || d.status.tag === DuelStatus.Complete.tag) return;
    if (!hasSeat(ctx, args.roomId, subject)) fail('not_in_room');
    const opponent = seatsForRoom(ctx, args.roomId).find(
      seat => seat.subject !== subject
    );
    if (!opponent) {
      lobby.leaveRoom(ctx.as.lobby, { roomId: args.roomId, subject });
      ctx.db.duel.roomId.update({
        ...d,
        status: DuelStatus.Abandoned,
        updatedAt: ctx.timestamp,
      });
      log(ctx, args.roomId, d.round, 'A pilot left. Duel abandoned.');
      return;
    }
    const winner = ctx.db.duelCombatant.combatantId.find(
      combatantId(args.roomId, opponent.subject)
    );
    const loser = ctx.db.duelCombatant.combatantId.find(
      combatantId(args.roomId, subject)
    );
    ctx.db.duel.roomId.update({
      ...d,
      status: DuelStatus.Complete,
      winnerSubject: opponent.subject,
      updatedAt: ctx.timestamp,
    });
    log(
      ctx,
      args.roomId,
      d.round,
      `${loser?.displayName ?? 'A pilot'} forfeits. ${winner?.displayName ?? 'Opponent'} wins.`
    );
    lobby.reportMatchResult(ctx.as.lobby, {
      roomId: args.roomId,
      subject,
      winnerSubject: opponent.subject,
    });
    lobby.closeRoom(ctx.as.lobby, { roomId: args.roomId, subject });
  }
);

export const queue_again = spacetimedb.reducer(
  { roomId: t.option(t.u64()) },
  (ctx, args) => {
    const subject = subjectFor(ctx);
    ensurePilot(ctx, subject);
    if (args.roomId !== undefined && hasSeat(ctx, args.roomId, subject)) {
      const d = ctx.db.duel.roomId.find(args.roomId);
      if (d && d.status.tag !== DuelStatus.Complete.tag) {
        ctx.db.duel.roomId.update({
          ...d,
          status: DuelStatus.Abandoned,
          updatedAt: ctx.timestamp,
        });
      }
      lobby.closeRoom(ctx.as.lobby, { roomId: args.roomId, subject });
    }
    const p = ensurePilot(ctx, subject);
    lobby.joinRankedQueue(ctx.as.lobby, {
      pool: DUEL_POOL,
      subject,
      matchSize: MATCH_SIZE,
      ratingPool: RATING_POOL,
      attributesJson: JSON.stringify({ shipClass: p.shipClass.tag }),
      ttlSeconds: 120,
    });
  }
);

export const init = spacetimedb.init(ctx => {
  lobby.installLobby(ctx.as.lobby);
  seedCatalog(ctx);
});
