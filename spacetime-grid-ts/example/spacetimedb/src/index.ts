import { Router, t, SenderError, type Infer } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import * as auth from '@spacetimedb/auth/submodule';
import {
  GRID_KIND_HEX,
  GRID_ORIENTATION_FLAT,
  GRID_MODE_COLLABORATIVE,
  cellsInRange,
  cellsInRangeParams,
  cellsInRangeReturn,
  computePath,
  deleteGrid,
} from '@spacetimedb/grid';
import { distance } from '@spacetimedb/grid/math';
import { gridEntity } from '@spacetimedb/grid/submodule';

import {
  MatchStatus,
  AI_BOT_USER_ID,
  AI_BOT_NAME,
  aiTurnEvent,
  aiTurnSchedule,
  consoleSendMail,
  match,
  matchExpirySchedule,
  playerUnit,
  spacetimedb,
  type WriteCtx,
} from './schema';
export { default } from './schema';

function throwSenderError(msg: string): never {
  throw new SenderError(msg);
}

function requireUserId(ctx: WriteCtx): string {
  const userId = auth.getCallerUserId(ctx.as.auth);
  if (!userId) throwSenderError('grid.not_authenticated');
  return userId;
}

export * from './views';

export const init = spacetimedb.init(ctx => {
  auth.install(ctx.as.auth);

  const types = [
    {
      typeId: 'marine',
      name: 'Marine',
      movement: 3,
      attackRange: 1,
      attackDmg: 3,
      hp: 10,
      glyph: 'M',
    },
    {
      typeId: 'titan',
      name: 'Titan',
      movement: 4,
      attackRange: 1,
      attackDmg: 5,
      hp: 14,
      glyph: 'T',
    },
    {
      typeId: 'drone',
      name: 'Drone',
      movement: 6,
      attackRange: 2,
      attackDmg: 2,
      hp: 6,
      glyph: 'D',
    },
  ];
  for (const u of types) {
    if (!ctx.db.unitType.typeId.find(u.typeId)) ctx.db.unitType.insert(u);
  }
  // Seed the AI opponent as an NPC actor. Lives outside auth_user so it
  // can't be impersonated and shows up in actor_directory as an Npc, not a User.
  if (!ctx.db.npcActor.actorId.find(AI_BOT_USER_ID)) {
    ctx.db.npcActor.insert({
      actorId: AI_BOT_USER_ID,
      name: AI_BOT_NAME,
      image: undefined,
      createdAt: ctx.timestamp,
    });
  }
});

const authHttp = auth.client({
  sendMail: consoleSendMail,
  appName: 'Grid',
  emailVerifiedRedirect: '/?verified=1',
});

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  authHttp.passwordSignup(ctx.as.auth, req)
);
export const authPasswordLogin = spacetimedb.httpHandler((ctx, req) =>
  authHttp.passwordLogin(ctx.as.auth, req)
);
export const authMe = spacetimedb.httpHandler((ctx, req) =>
  authHttp.me(ctx.as.auth, req)
);
export const authLogout = spacetimedb.httpHandler((ctx, req) =>
  authHttp.logout(ctx.as.auth, req)
);
export const authRefresh = spacetimedb.httpHandler((ctx, req) =>
  authHttp.refresh(ctx.as.auth, req)
);
export const authGoogleStart = spacetimedb.httpHandler((ctx, req) =>
  authHttp.googleStart(ctx.as.auth, req)
);
export const authGoogleCallback = spacetimedb.httpHandler((ctx, req) =>
  authHttp.googleCallback(ctx.as.auth, req)
);
export const authGithubStart = spacetimedb.httpHandler((ctx, req) =>
  authHttp.githubStart(ctx.as.auth, req)
);
export const authGithubCallback = spacetimedb.httpHandler((ctx, req) =>
  authHttp.githubCallback(ctx.as.auth, req)
);
export const authPasswordForgot = spacetimedb.httpHandler((ctx, req) =>
  authHttp.forgotPassword(ctx.as.auth, req)
);
export const authPasswordReset = spacetimedb.httpHandler((ctx, req) =>
  authHttp.resetPassword(ctx.as.auth, req)
);
export const authEmailVerifyRequest = spacetimedb.httpHandler((ctx, req) =>
  authHttp.emailVerifyRequest(ctx.as.auth, req)
);
export const authEmailVerify = spacetimedb.httpHandler((ctx, req) =>
  authHttp.emailVerify(ctx.as.auth, req)
);

export const router = spacetimedb.httpRouter(
  new Router()
    .post('/auth/password/signup', authPasswordSignup)
    .post('/auth/password/login', authPasswordLogin)
    .post('/auth/session/refresh', authRefresh)
    .get('/auth/me', authMe)
    .post('/auth/logout', authLogout)
    .get('/auth/google/start', authGoogleStart)
    .get('/auth/google/callback', authGoogleCallback)
    .get('/auth/github/start', authGithubStart)
    .get('/auth/github/callback', authGithubCallback)
    .post('/auth/password/forgot', authPasswordForgot)
    .post('/auth/password/reset', authPasswordReset)
    .post('/auth/email/verify-request', authEmailVerifyRequest)
    .get('/auth/email/verify', authEmailVerify)
);

// The playable area is a HEXAGON of radius R centered at axial (R, R).
// The grid submodule allocates a (2R+1) x (2R+1) rectangle because its bounds
// checker uses rectangular coordinates. Cells outside the playable hex are
// impassable, which keeps A* and Dijkstra inside the SpacetimeDB-logo shape.
const GRID_RADIUS = 5;
const GRID_DIAMETER = 2 * GRID_RADIUS + 1; // 11
const DEFAULT_COST = 1;

// Matches a user may have in Waiting or Active state at once.
const MAX_UNFINISHED_MATCHES = 5;
// A Waiting match nobody joins is deleted after this long.
const WAITING_MATCH_TTL_MICROS = 30n * 60n * 1_000_000n;
// Pause before the AI plays so the player sees the turn change.
const AI_TURN_DELAY_MICROS = 500_000n;

function isInHexShape(q: number, r: number): boolean {
  const cx = GRID_RADIUS,
    cy = GRID_RADIUS;
  return (
    (Math.abs(q - cx) + Math.abs(r - cy) + Math.abs(q + r - (cx + cy))) / 2 <=
    GRID_RADIUS
  );
}

const PLAYER_SPAWNS = [
  { x: GRID_RADIUS, y: 0, typeId: 'marine' },
  { x: GRID_RADIUS - 1, y: 1, typeId: 'titan' },
  { x: GRID_RADIUS + 1, y: 0, typeId: 'drone' },
];
const OPPONENT_SPAWNS = [
  { x: GRID_RADIUS, y: 2 * GRID_RADIUS, typeId: 'marine' },
  { x: GRID_RADIUS + 1, y: 2 * GRID_RADIUS - 1, typeId: 'titan' },
  { x: GRID_RADIUS - 1, y: 2 * GRID_RADIUS, typeId: 'drone' },
];

// Deterministic-ish terrain seed (uses match createdAt micros). Cheap PRNG.
function rng(seed: bigint) {
  let state = seed === 0n ? 1n : seed;
  const UINT32_MASK = 0xffffffffn;
  return (): number => {
    state = (state * 1103515245n + 12345n) & UINT32_MASK;
    return Number(state) / Number(UINT32_MASK);
  };
}

function afterMicros(ctx: WriteCtx, micros: bigint) {
  return ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + micros);
}

function requireUnfinishedMatchSlot(ctx: WriteCtx, userId: string): void {
  let unfinished = 0;
  for (const p of ctx.db.matchParticipant.userId.filter(userId)) {
    const m = ctx.db.match.matchId.find(p.matchId);
    if (m && m.status.tag !== 'Ended') unfinished++;
  }
  if (unfinished >= MAX_UNFINISHED_MATCHES)
    throwSenderError('grid.too_many_matches');
}

// A procedure so the client receives the new match id.
export const createMatch = spacetimedb.procedure(
  { vsAi: t.bool() },
  t.object('CreateMatchResult', { matchId: t.u64(), gridId: t.u64() }),
  (ctx, args) =>
    ctx.withTx(tx => {
      const userId = requireUserId(tx);
      requireUnfinishedMatchSlot(tx, userId);

      // 1. Create the grid (collaborative so both players can move units via our own reducers).
      const gridRowInserted = tx.db.grid.grid.insert({
        id: 0n,
        ownerUserId: userId,
        name: `Match by ${userId.slice(0, 8)}`,
        kind: GRID_KIND_HEX,
        orientation: GRID_ORIENTATION_FLAT,
        width: GRID_DIAMETER,
        height: GRID_DIAMETER,
        defaultCost: DEFAULT_COST,
        connectivity: 6,
        mode: GRID_MODE_COLLABORATIVE,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });

      // 2. Seed terrain. Cells outside the hex shape are 'void' + impassable
      //    so neither A* nor Dijkstra crosses them. Spawn cells stay clear.
      const spawnKeys = new Set(
        [...PLAYER_SPAWNS, ...OPPONENT_SPAWNS].map(s => `${s.x},${s.y}`)
      );
      const rand = rng(tx.timestamp.microsSinceUnixEpoch);
      for (let y = 0; y < GRID_DIAMETER; y++) {
        for (let x = 0; x < GRID_DIAMETER; x++) {
          if (!isInHexShape(x, y)) {
            tx.db.grid.cellState.insert({
              id: 0n,
              gridId: gridRowInserted.id,
              x,
              y,
              cost: -1,
              terrain: 'void',
            });
            continue;
          }
          if (spawnKeys.has(`${x},${y}`)) continue; // keep spawns clear
          // Single tactical-obstacle type: impassable crater. Movement is
          // either 1 (regolith) or blocked  -  no slow terrain to remember.
          if (rand() < 0.14) {
            tx.db.grid.cellState.insert({
              id: 0n,
              gridId: gridRowInserted.id,
              x,
              y,
              cost: -1,
              terrain: 'crater',
            });
          }
        }
      }

      // 3. Create the match row. vs-AI starts active immediately; vs-human
      //    waits for someone to call joinMatch and expires if nobody does.
      const matchInserted = tx.db.match.insert({
        matchId: 0n,
        status: args.vsAi ? MatchStatus.Active : MatchStatus.Waiting,
        currentSeatIdx: 0,
        turnNumber: 1,
        winnerUserId: undefined,
        gridId: gridRowInserted.id,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });

      insertParticipant(tx, matchInserted.matchId, userId, 0, 0);
      placeStartingUnits(
        tx,
        matchInserted.matchId,
        gridRowInserted.id,
        userId,
        PLAYER_SPAWNS
      );

      if (args.vsAi) {
        insertParticipant(tx, matchInserted.matchId, AI_BOT_USER_ID, 1, 1);
        placeStartingUnits(
          tx,
          matchInserted.matchId,
          gridRowInserted.id,
          AI_BOT_USER_ID,
          OPPONENT_SPAWNS
        );
      } else {
        tx.db.matchExpirySchedule.insert({
          scheduledId: 0n,
          scheduledAt: afterMicros(tx, WAITING_MATCH_TTL_MICROS),
          matchId: matchInserted.matchId,
        });
      }

      return { matchId: matchInserted.matchId, gridId: gridRowInserted.id };
    })
);

export const joinMatch = spacetimedb.reducer(
  { matchId: t.u64() },
  (ctx, { matchId }) => {
    const userId = requireUserId(ctx);
    const m = ctx.db.match.matchId.find(matchId);
    if (!m) throwSenderError(`grid.match_not_found:${matchId}`);
    if (m.status.tag !== 'Waiting')
      throwSenderError(`grid.match_not_joinable:${m.status.tag}`);
    if (isSeated(ctx, matchId, userId))
      throwSenderError(`grid.match_self_join`);
    requireUnfinishedMatchSlot(ctx, userId);

    insertParticipant(ctx, m.matchId, userId, 1, 1);
    placeStartingUnits(ctx, m.matchId, m.gridId, userId, OPPONENT_SPAWNS);

    ctx.db.match.matchId.update({
      ...m,
      status: MatchStatus.Active,
      updatedAt: ctx.timestamp,
    });
  }
);

// Cancels a Waiting match (deleting it) or forfeits an Active one.
export const leaveMatch = spacetimedb.reducer(
  { matchId: t.u64() },
  (ctx, { matchId }) => {
    const userId = requireUserId(ctx);
    const m = ctx.db.match.matchId.find(matchId);
    if (!m) throwSenderError(`grid.match_not_found:${matchId}`);
    if (!isSeated(ctx, matchId, userId)) throwSenderError('grid.not_in_match');
    if (m.status.tag === 'Waiting') {
      deleteMatch(ctx, m);
    } else if (m.status.tag === 'Active') {
      const winner = [...ctx.db.matchParticipant.matchId.filter(matchId)].find(
        p => p.userId !== userId
      );
      ctx.db.match.matchId.update({
        ...m,
        status: MatchStatus.Ended,
        winnerUserId: winner?.userId,
        updatedAt: ctx.timestamp,
      });
    } else {
      throwSenderError(`grid.match_not_active:${m.status.tag}`);
    }
  }
);

// Scheduled at match creation. Scheduled reducers are private to the module.
export const expireWaitingMatch = spacetimedb.reducer(
  { onSchedule: matchExpirySchedule },
  { arg: matchExpirySchedule.rowType },
  (ctx, { arg }) => {
    const m = ctx.db.match.matchId.find(arg.matchId);
    if (m?.status.tag === 'Waiting') deleteMatch(ctx, m);
  }
);

export const endTurn = spacetimedb.reducer(
  { matchId: t.u64() },
  (ctx, { matchId }) => {
    const userId = requireUserId(ctx);
    advanceTurn(ctx, requireTurn(ctx, matchId, userId));
  }
);

type Match = Infer<typeof match.rowType>;

function insertParticipant(
  ctx: WriteCtx,
  matchId: bigint,
  userId: string,
  seatIdx: number,
  team: number
): void {
  ctx.db.matchParticipant.insert({
    id: 0n,
    matchId,
    userId,
    seatIdx,
    team,
    joinedAt: ctx.timestamp,
  });
}

function isSeated(ctx: WriteCtx, matchId: bigint, userId: string): boolean {
  for (const p of ctx.db.matchParticipant.matchId.filter(matchId)) {
    if (p.userId === userId) return true;
  }
  return false;
}

function participantsBySeat(
  ctx: WriteCtx,
  matchId: bigint
): Map<number, string> {
  const seats = new Map<number, string>();
  for (const p of ctx.db.matchParticipant.matchId.filter(matchId)) {
    seats.set(p.seatIdx, p.userId);
  }
  return seats;
}

// Returns the match if it is Active and it is userId's turn.
function requireTurn(ctx: WriteCtx, matchId: bigint, userId: string): Match {
  const m = ctx.db.match.matchId.find(matchId);
  if (!m) throwSenderError(`grid.match_not_found:${matchId}`);
  if (m.status.tag !== 'Active')
    throwSenderError(`grid.match_not_active:${m.status.tag}`);
  if (participantsBySeat(ctx, matchId).get(m.currentSeatIdx) !== userId)
    throwSenderError(`grid.not_your_turn`);
  return m;
}

// Passes the turn to the next seat and schedules the AI when its seat is next.
function advanceTurn(ctx: WriteCtx, m: Match): void {
  const seats = participantsBySeat(ctx, m.matchId);
  const nextIdx = (m.currentSeatIdx + 1) % seats.size;
  const nextUserId = seats.get(nextIdx);
  for (const u of ctx.db.playerUnit.matchId.filter(m.matchId)) {
    if (u.ownerUserId === nextUserId) {
      ctx.db.playerUnit.entityId.update({
        ...u,
        hasMoved: false,
        hasAttacked: false,
      });
    }
  }
  ctx.db.match.matchId.update({
    ...m,
    currentSeatIdx: nextIdx,
    turnNumber: nextIdx === 0 ? m.turnNumber + 1 : m.turnNumber,
    updatedAt: ctx.timestamp,
  });
  if (nextUserId === AI_BOT_USER_ID) {
    ctx.db.aiTurnSchedule.insert({
      scheduledId: 0n,
      scheduledAt: afterMicros(ctx, AI_TURN_DELAY_MICROS),
      matchId: m.matchId,
    });
  }
}

// Deletes a Waiting match with its seat, units, and grid.
function deleteMatch(ctx: WriteCtx, m: Match): void {
  for (const u of [...ctx.db.playerUnit.matchId.filter(m.matchId)]) {
    ctx.db.playerUnit.delete(u);
  }
  for (const p of [...ctx.db.matchParticipant.matchId.filter(m.matchId)]) {
    ctx.db.matchParticipant.delete(p);
  }
  const grid = ctx.db.grid.grid.id.find(m.gridId);
  if (grid) deleteGrid(ctx.as.grid, { gridId: grid.id }, grid.ownerUserId);
  ctx.db.match.delete(m);
}

function placeStartingUnits(
  ctx: WriteCtx,
  matchId: bigint,
  gridId: bigint,
  ownerUserId: string,
  spawns: Array<{ x: number; y: number; typeId: string }>
): void {
  for (const s of spawns) {
    const type = ctx.db.unitType.typeId.find(s.typeId);
    if (!type) throwSenderError(`grid.unknown_unit_type:${s.typeId}`);
    const entity = ctx.db.grid.gridEntity.insert({
      id: 0n,
      gridId,
      ownerUserId,
      x: s.x,
      y: s.y,
      kind: s.typeId,
      blocksMovement: true,
      label: undefined,
      createdAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    });
    ctx.db.playerUnit.insert({
      entityId: entity.id,
      matchId,
      ownerUserId,
      typeId: s.typeId,
      currentHp: type.hp,
      hasMoved: false,
      hasAttacked: false,
      createdAt: ctx.timestamp,
    });
  }
}

// Validates and applies the move in one transaction. A procedure so the
// client receives the A* path to animate.
export const moveUnit = spacetimedb.procedure(
  { entityId: t.u64(), toX: t.i32(), toY: t.i32() },
  t.object('MoveUnitResult', {
    // The exact A* path from start to end, including both endpoints.
    path: t.array(t.object('MoveStep', { x: t.i32(), y: t.i32() })),
  }),
  (ctx, args) =>
    ctx.withTx(tx => {
      const userId = requireUserId(tx);
      const unit = tx.db.playerUnit.entityId.find(args.entityId);
      if (!unit) throwSenderError(`grid.unit_not_found:${args.entityId}`);
      if (unit.ownerUserId !== userId) throwSenderError(`grid.not_unit_owner`);
      if (unit.hasMoved) throwSenderError(`grid.already_moved`);
      requireTurn(tx, unit.matchId, userId);

      const entity = tx.db.grid.gridEntity.id.find(args.entityId);
      if (!entity) throwSenderError(`grid.entity_not_found:${args.entityId}`);
      const type = tx.db.unitType.typeId.find(unit.typeId);
      if (!type) throwSenderError(`grid.unknown_unit_type:${unit.typeId}`);

      // Blocking entities make occupied cells impassable, so an occupied
      // destination has no path.
      const path = computePath(
        tx.as.grid,
        {
          gridId: entity.gridId,
          startX: entity.x,
          startY: entity.y,
          endX: args.toX,
          endY: args.toY,
          storeFor: undefined,
          maxExpansions: undefined,
        },
        userId
      );
      if (!path.found)
        throwSenderError(`grid.no_path_to:${args.toX},${args.toY}`);
      if (path.cost > type.movement)
        throwSenderError(`grid.move_too_far:${path.cost}>${type.movement}`);

      tx.db.grid.gridEntity.id.update({
        ...entity,
        x: args.toX,
        y: args.toY,
        updatedAt: tx.timestamp,
      });
      tx.db.playerUnit.entityId.update({ ...unit, hasMoved: true });
      return { path: path.cells };
    })
);

export const attackUnit = spacetimedb.reducer(
  { attackerId: t.u64(), targetId: t.u64() },
  (ctx, args) => {
    const userId = requireUserId(ctx);
    const attacker = ctx.db.playerUnit.entityId.find(args.attackerId);
    const target = ctx.db.playerUnit.entityId.find(args.targetId);
    if (!attacker) throwSenderError(`grid.unit_not_found:${args.attackerId}`);
    if (!target) throwSenderError(`grid.unit_not_found:${args.targetId}`);
    if (attacker.ownerUserId !== userId)
      throwSenderError(`grid.not_unit_owner`);
    if (attacker.hasAttacked) throwSenderError(`grid.already_attacked`);
    if (target.ownerUserId === userId)
      throwSenderError(`grid.cant_attack_self`);
    if (attacker.matchId !== target.matchId)
      throwSenderError(`grid.cross_match_attack`);
    requireTurn(ctx, attacker.matchId, userId);

    const attackerEntity = ctx.db.grid.gridEntity.id.find(args.attackerId);
    const targetEntity = ctx.db.grid.gridEntity.id.find(args.targetId);
    if (!attackerEntity || !targetEntity)
      throwSenderError(`grid.entity_missing`);
    const type = ctx.db.unitType.typeId.find(attacker.typeId);
    if (!type) throwSenderError(`grid.unknown_unit_type:${attacker.typeId}`);

    const dist = distance('hex', attackerEntity, targetEntity);
    if (dist > type.attackRange) {
      throwSenderError(`grid.target_out_of_range:${dist}>${type.attackRange}`);
    }
    applyAttack(ctx, attacker, target, targetEntity, type.attackDmg);
  }
);

type Unit = Infer<typeof playerUnit.rowType>;
type Entity = Infer<typeof gridEntity.rowType>;
type AiTurnEvent = Infer<typeof aiTurnEvent>;
type AttackInfo = NonNullable<AiTurnEvent['attack']>;

// Damages the target, removes it at 0 HP, and ends the match when the
// attacker's team has no enemies left. Returns the pre-damage snapshot.
function applyAttack(
  ctx: WriteCtx,
  attacker: Unit,
  target: Unit,
  targetEntity: Entity,
  damage: number
): AttackInfo {
  const info: AttackInfo = {
    targetId: target.entityId,
    damage,
    killed: target.currentHp <= damage,
    targetX: targetEntity.x,
    targetY: targetEntity.y,
    targetOwner: target.ownerUserId,
    targetTypeId: target.typeId,
    targetPreHp: target.currentHp,
  };
  ctx.db.playerUnit.entityId.update({ ...attacker, hasAttacked: true });
  if (!info.killed) {
    ctx.db.playerUnit.entityId.update({
      ...target,
      currentHp: target.currentHp - damage,
    });
    return info;
  }

  ctx.db.playerUnit.delete(target);
  ctx.db.grid.gridEntity.delete(targetEntity);
  const teams = new Map<string, number>();
  for (const p of ctx.db.matchParticipant.matchId.filter(attacker.matchId)) {
    teams.set(p.userId, p.team);
  }
  const attackerTeam = teams.get(attacker.ownerUserId);
  const enemyLeft = [
    ...ctx.db.playerUnit.matchId.filter(attacker.matchId),
  ].some(u => teams.get(u.ownerUserId) !== attackerTeam);
  const m = ctx.db.match.matchId.find(attacker.matchId);
  if (!enemyLeft && m) {
    ctx.db.match.matchId.update({
      ...m,
      status: MatchStatus.Ended,
      winnerUserId: attacker.ownerUserId,
      updatedAt: ctx.timestamp,
    });
  }
  return info;
}

// Scheduled by advanceTurn. Scheduled reducers are private to the module, so
// clients cannot trigger the AI. The events land in ai_turn_log for the
// client to animate.
export const aiTakeTurn = spacetimedb.reducer(
  { onSchedule: aiTurnSchedule },
  { arg: aiTurnSchedule.rowType },
  (ctx, { arg }) => {
    const m = ctx.db.match.matchId.find(arg.matchId);
    if (
      !m ||
      m.status.tag !== 'Active' ||
      participantsBySeat(ctx, m.matchId).get(m.currentSeatIdx) !==
        AI_BOT_USER_ID
    ) {
      return;
    }
    const log = { matchId: m.matchId, events: playAiTurn(ctx, m) };
    if (ctx.db.aiTurnLog.matchId.find(m.matchId)) {
      ctx.db.aiTurnLog.matchId.update(log);
    } else {
      ctx.db.aiTurnLog.insert(log);
    }
    const after = ctx.db.match.matchId.find(m.matchId);
    if (after?.status.tag === 'Active') advanceTurn(ctx, after);
  }
);

// Greedy heuristic: for each AI unit (highest damage first), attack the
// lowest-HP enemy in range; otherwise move toward the nearest enemy and attack
// if that brings one into range.
function playAiTurn(ctx: WriteCtx, m: Match): AiTurnEvent[] {
  const units = () =>
    [...ctx.db.playerUnit.matchId.filter(m.matchId)].flatMap(unit => {
      const entity = ctx.db.grid.gridEntity.id.find(unit.entityId);
      return entity ? [{ unit, entity }] : [];
    });
  const enemies = () =>
    units().filter(e => e.unit.ownerUserId !== AI_BOT_USER_ID);
  const pickTarget = (at: { x: number; y: number }, range: number) =>
    enemies()
      .filter(e => distance('hex', at, e.entity) <= range)
      .sort((a, b) => a.unit.currentHp - b.unit.currentHp)[0];
  const attackDmg = (typeId: string) =>
    ctx.db.unitType.typeId.find(typeId)?.attackDmg ?? 0;

  const events: AiTurnEvent[] = [];
  const aiUnits = units()
    .filter(e => e.unit.ownerUserId === AI_BOT_USER_ID)
    .sort((a, b) => attackDmg(b.unit.typeId) - attackDmg(a.unit.typeId));
  for (const { unit, entity } of aiUnits) {
    const type = ctx.db.unitType.typeId.find(unit.typeId);
    if (!type) continue;
    const event: AiTurnEvent = {
      entityId: unit.entityId,
      movePath: undefined,
      attack: undefined,
    };
    let target = pickTarget(entity, type.attackRange);

    const foes = enemies();
    if (!target && foes.length > 0) {
      // cellsInRange skips blocked and occupied cells; cost 0 is the origin.
      const { cells } = cellsInRange(
        ctx.as.grid,
        {
          gridId: m.gridId,
          originX: entity.x,
          originY: entity.y,
          maxCost: type.movement,
        },
        AI_BOT_USER_ID
      );
      let best: (typeof cells)[number] | undefined;
      let bestScore = Infinity;
      for (const c of cells) {
        if (c.cost === 0) continue;
        const closest = Math.min(
          ...foes.map(e => distance('hex', c, e.entity))
        );
        // Primary: closeness to enemy. Tiebreak: prefer cheaper paths.
        const score = closest * 100 + c.cost;
        if (score < bestScore) {
          bestScore = score;
          best = c;
        }
      }
      if (best) {
        const path = computePath(
          ctx.as.grid,
          {
            gridId: m.gridId,
            startX: entity.x,
            startY: entity.y,
            endX: best.x,
            endY: best.y,
            storeFor: undefined,
            maxExpansions: undefined,
          },
          AI_BOT_USER_ID
        );
        if (path.found) {
          ctx.db.grid.gridEntity.id.update({
            ...entity,
            x: best.x,
            y: best.y,
            updatedAt: ctx.timestamp,
          });
          ctx.db.playerUnit.entityId.update({ ...unit, hasMoved: true });
          event.movePath = path.cells;
          target = pickTarget(best, type.attackRange);
        }
      }
    }

    const attacker = ctx.db.playerUnit.entityId.find(unit.entityId);
    if (target && attacker) {
      event.attack = applyAttack(
        ctx,
        attacker,
        target.unit,
        target.entity,
        type.attackDmg
      );
    }
    if (event.movePath || event.attack) events.push(event);
  }
  return events;
}

// Movement-range preview for the selected unit. A procedure so the client
// receives the cells. Callers must hold a seat in the grid's match.
export const getCellsInRange = spacetimedb.procedure(
  cellsInRangeParams,
  cellsInRangeReturn,
  (ctx, args) =>
    ctx.withTx(tx => {
      const userId = requireUserId(tx);
      const m = [...tx.db.match.gridId.filter(args.gridId)][0];
      if (!m || !isSeated(tx, m.matchId, userId))
        throwSenderError('grid.not_in_match');
      return cellsInRange(tx.as.grid, args, userId);
    })
);
