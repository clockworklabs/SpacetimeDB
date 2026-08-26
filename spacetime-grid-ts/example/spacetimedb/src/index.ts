import { t, SenderError, type ProcedureCtx } from 'spacetimedb/server';
import { Timestamp, type Identity } from 'spacetimedb';
import * as auth from '@spacetimedb/auth/submodule';
import * as gridSubmodule from '@spacetimedb/grid/submodule';
import { getCallerUserId } from '@spacetimedb/auth/submodule';
import {
  GRID_KIND_HEX,
  GRID_ORIENTATION_FLAT,
  GRID_MODE_COLLABORATIVE,
  computePathImpl,
  cellsInRangeImpl,
} from '@spacetimedb/grid';
import { distance } from '@spacetimedb/grid/math';

import {
  MatchStatus,
  AI_BOT_USER_ID,
  AI_BOT_NAME,
  spacetimedb,
  type Schema,
  type WriteCtx,
} from './schema';
export { default } from './schema';
export * from './auth-adapter';

function throwSenderError(msg: string): never {
  throw new SenderError(msg);
}

function requireUserId(ctx: ProcedureCtx<Schema>): string {
  const userId = getCallerUserId(ctx.as.auth);
  if (!userId) throwSenderError('grid.not_authenticated');
  return userId;
}

export * from './views';

export const init = spacetimedb.init(ctx => {
  auth.installAuth(ctx.as.auth);
  gridSubmodule.installGrid(ctx.as.grid);

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

export const whoami = spacetimedb.procedure(
  {},
  t.object('WhoAmI', {
    userId: t.option(t.string()),
    senderIdentityHex: t.string(),
  }),
  ctx => {
    const userId = getCallerUserId(ctx.as.auth);
    return {
      userId: userId ?? undefined,
      senderIdentityHex: (ctx.sender as Identity).toHexString(),
    };
  }
);

// The playable area is a HEXAGON of radius R centered at axial (R, R).
// The grid submodule allocates a (2R+1) x (2R+1) rectangle because its bounds
// checker uses rectangular coordinates. Cells outside the playable hex are
// impassable, which keeps A* and Dijkstra inside the SpacetimeDB-logo shape.
const GRID_RADIUS = 5;
const GRID_DIAMETER = 2 * GRID_RADIUS + 1; // 11
const DEFAULT_COST = 1;

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
  const M = 0xffffffffn;
  return (): number => {
    state = (state * 1103515245n + 12345n) & M;
    return Number(state) / Number(M);
  };
}

export const create_match = spacetimedb.procedure(
  { vsAi: t.bool() },
  t.object('CreateMatchResult', { matchId: t.u64(), gridId: t.u64() }),
  (ctx, args) => {
    const userId = requireUserId(ctx);
    return ctx.withTx(tx => {
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
        createdAt: ctx.timestamp,
        updatedAt: ctx.timestamp,
      });

      // 2. Seed terrain. Cells outside the hex shape are 'void' + impassable
      //    so neither A* nor Dijkstra crosses them. Spawn cells stay clear.
      const spawnKeys = new Set(
        [...PLAYER_SPAWNS, ...OPPONENT_SPAWNS].map(s => `${s.x},${s.y}`)
      );
      const seedMicros = ctx.timestamp.microsSinceUnixEpoch as bigint;
      const rand = rng(seedMicros);
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
      //    waits for someone to call join_match.
      const matchInserted = tx.db.match.insert({
        matchId: 0n,
        status: args.vsAi ? MatchStatus.Active : MatchStatus.Waiting,
        currentSeatIdx: 0,
        turnNumber: 1,
        winnerUserId: undefined,
        gridId: gridRowInserted.id,
        createdAt: ctx.timestamp,
        updatedAt: ctx.timestamp,
      });

      insertParticipant(tx, matchInserted.matchId, userId, 0, 0, ctx.timestamp);
      placeStartingUnits(
        tx,
        ctx.timestamp,
        matchInserted.matchId,
        gridRowInserted.id,
        userId,
        PLAYER_SPAWNS
      );

      if (args.vsAi) {
        insertParticipant(
          tx,
          matchInserted.matchId,
          AI_BOT_USER_ID,
          1,
          1,
          ctx.timestamp
        );
        placeStartingUnits(
          tx,
          ctx.timestamp,
          matchInserted.matchId,
          gridRowInserted.id,
          AI_BOT_USER_ID,
          OPPONENT_SPAWNS
        );
      }

      return { matchId: matchInserted.matchId, gridId: gridRowInserted.id };
    });
  }
);

export const join_match = spacetimedb.procedure(
  { matchId: t.u64() },
  t.unit(),
  (ctx, { matchId }) => {
    const userId = requireUserId(ctx);
    ctx.withTx(tx => {
      const m = tx.db.match.matchId.find(matchId);
      if (!m) throwSenderError(`grid.match_not_found:${matchId}`);
      if (m.status.tag !== 'Waiting')
        throwSenderError(`grid.match_not_joinable:${m.status.tag}`);
      for (const p of tx.db.matchParticipant.matchId.filter(matchId)) {
        if (p.userId === userId) throwSenderError(`grid.match_self_join`);
      }

      insertParticipant(tx, m.matchId, userId, 1, 1, ctx.timestamp);
      placeStartingUnits(
        tx,
        ctx.timestamp,
        m.matchId,
        m.gridId,
        userId,
        OPPONENT_SPAWNS
      );

      tx.db.match.matchId.update({
        ...m,
        status: MatchStatus.Active,
        updatedAt: ctx.timestamp,
      });
    });
    return {};
  }
);

export const end_turn = spacetimedb.procedure(
  { matchId: t.u64() },
  t.unit(),
  (ctx, { matchId }) => {
    const userId = requireUserId(ctx);
    ctx.withTx(tx => {
      const m = tx.db.match.matchId.find(matchId);
      if (!m) throwSenderError(`grid.match_not_found:${matchId}`);
      if (m.status.tag !== 'Active')
        throwSenderError(`grid.match_not_active:${m.status.tag}`);
      const seats = participantsBySeat(tx, matchId);
      if (userIdAt(seats, m.currentSeatIdx) !== userId)
        throwSenderError(`grid.not_your_turn`);

      // Two-seat rotation. Generalizes via (currentSeatIdx + 1) % seats.size.
      const nextIdx = (m.currentSeatIdx + 1) % seats.size;
      const nextUserId = userIdAt(seats, nextIdx);
      for (const u of tx.db.playerUnit.matchId.filter(matchId)) {
        if (u.ownerUserId === nextUserId) {
          tx.db.playerUnit.entityId.update({
            ...u,
            hasMoved: false,
            hasAttacked: false,
          });
        }
      }
      tx.db.match.matchId.update({
        ...m,
        currentSeatIdx: nextIdx,
        turnNumber: nextIdx === 0 ? m.turnNumber + 1 : m.turnNumber,
        updatedAt: ctx.timestamp,
      });
    });
    return {};
  }
);

function insertParticipant(
  tx: WriteCtx,
  matchId: bigint,
  userId: string,
  seatIdx: number,
  team: number,
  timestamp: Timestamp
): void {
  tx.db.matchParticipant.insert({
    id: 0n,
    matchId,
    userId,
    seatIdx,
    team,
    joinedAt: timestamp,
  });
}

function participantsBySeat(
  tx: WriteCtx,
  matchId: bigint
): Map<number, string> {
  const seats = new Map<number, string>();
  for (const p of tx.db.matchParticipant.matchId.filter(matchId)) {
    seats.set(p.seatIdx, p.userId);
  }
  return seats;
}

function userIdAt(
  seats: Map<number, string>,
  seatIdx: number
): string | undefined {
  return seats.get(seatIdx);
}

function participantTeams(tx: WriteCtx, matchId: bigint): Map<string, number> {
  const teams = new Map<string, number>();
  for (const p of tx.db.matchParticipant.matchId.filter(matchId)) {
    teams.set(p.userId, p.team);
  }
  return teams;
}

function placeStartingUnits(
  tx: WriteCtx,
  timestamp: Timestamp,
  matchId: bigint,
  gridId: bigint,
  ownerUserId: string,
  spawns: Array<{ x: number; y: number; typeId: string }>
): void {
  for (const s of spawns) {
    const type = tx.db.unitType.typeId.find(s.typeId);
    if (!type) throwSenderError(`grid.unknown_unit_type:${s.typeId}`);
    const entity = tx.db.grid.gridEntity.insert({
      id: 0n,
      gridId,
      ownerUserId,
      x: s.x,
      y: s.y,
      kind: s.typeId,
      blocksMovement: true,
      label: undefined,
      createdAt: timestamp,
      updatedAt: timestamp,
    });
    tx.db.playerUnit.insert({
      entityId: entity.id,
      matchId,
      ownerUserId,
      typeId: s.typeId,
      currentHp: type.hp,
      hasMoved: false,
      hasAttacked: false,
      createdAt: timestamp,
    });
  }
}

export const move_unit = spacetimedb.procedure(
  { entityId: t.u64(), toX: t.i32(), toY: t.i32() },
  t.object('MoveUnitResult', {
    // The exact A* path from start to end, including both endpoints. The client
    // animates the unit along this path to the destination.
    path: t.array(t.object('MoveStep', { x: t.i32(), y: t.i32() })),
  }),
  (ctx, args) => {
    const userId = requireUserId(ctx);

    // Validate ownership and turn state, then capture coordinates for pathfinding.
    // computePathImpl opens its own transaction, so run it after this transaction.
    let entityX = 0,
      entityY = 0;
    let gridId = 0n;
    let typeMovement = 0;
    ctx.withTx(tx => {
      const unit = tx.db.playerUnit.entityId.find(args.entityId);
      if (!unit) throwSenderError(`grid.unit_not_found:${args.entityId}`);
      if (unit.ownerUserId !== userId) throwSenderError(`grid.not_unit_owner`);
      if (unit.hasMoved) throwSenderError(`grid.already_moved`);

      const m = tx.db.match.matchId.find(unit.matchId);
      if (!m) throwSenderError(`grid.match_not_found:${unit.matchId}`);
      if (m.status.tag !== 'Active')
        throwSenderError(`grid.match_not_active:${m.status.tag}`);
      const seats = participantsBySeat(tx, unit.matchId);
      if (userIdAt(seats, m.currentSeatIdx) !== userId)
        throwSenderError(`grid.not_your_turn`);

      const entity = tx.db.grid.gridEntity.id.find(args.entityId);
      if (!entity) throwSenderError(`grid.entity_not_found:${args.entityId}`);

      const type = tx.db.unitType.typeId.find(unit.typeId);
      if (!type) throwSenderError(`grid.unknown_unit_type:${unit.typeId}`);

      entityX = entity.x;
      entityY = entity.y;
      gridId = entity.gridId;
      typeMovement = type.movement;
    });

    const path = computePathImpl(
      ctx.as.grid,
      {
        gridId,
        startX: entityX,
        startY: entityY,
        endX: args.toX,
        endY: args.toY,
        storeFor: undefined,
        maxExpansions: undefined,
      },
      userId
    ) as {
      found: boolean;
      cells: Array<{ x: number; y: number }>;
      cost: number;
    };
    if (!path.found)
      throwSenderError(`grid.no_path_to:${args.toX},${args.toY}`);
    if (path.cost > typeMovement)
      throwSenderError(`grid.move_too_far:${path.cost}>${typeMovement}`);

    // Apply the move. Procedure serialization bounds TOCTOU, and the transaction
    // verifies hasMoved immediately before mutation.
    ctx.withTx(tx => {
      const unit = tx.db.playerUnit.entityId.find(args.entityId);
      const entity = tx.db.grid.gridEntity.id.find(args.entityId);
      if (!unit || !entity) throwSenderError(`grid.unit_vanished`);
      if (unit.hasMoved) throwSenderError(`grid.already_moved`);
      tx.db.grid.gridEntity.id.update({
        ...entity,
        x: args.toX,
        y: args.toY,
        updatedAt: ctx.timestamp,
      });
      tx.db.playerUnit.entityId.update({ ...unit, hasMoved: true });
    });
    return { path: path.cells };
  }
);

export const attack_unit = spacetimedb.procedure(
  { attackerId: t.u64(), targetId: t.u64() },
  t.unit(),
  (ctx, args) => {
    const userId = requireUserId(ctx);
    ctx.withTx(tx => {
      const attacker = tx.db.playerUnit.entityId.find(args.attackerId);
      const target = tx.db.playerUnit.entityId.find(args.targetId);
      if (!attacker) throwSenderError(`grid.unit_not_found:${args.attackerId}`);
      if (!target) throwSenderError(`grid.unit_not_found:${args.targetId}`);
      if (attacker.ownerUserId !== userId)
        throwSenderError(`grid.not_unit_owner`);
      if (attacker.hasAttacked) throwSenderError(`grid.already_attacked`);
      if (target.ownerUserId === userId)
        throwSenderError(`grid.cant_attack_self`);
      if (attacker.matchId !== target.matchId)
        throwSenderError(`grid.cross_match_attack`);

      const m = tx.db.match.matchId.find(attacker.matchId);
      if (!m) throwSenderError(`grid.match_not_found:${attacker.matchId}`);
      if (m.status.tag !== 'Active') throwSenderError(`grid.match_not_active`);
      const seats = participantsBySeat(tx, attacker.matchId);
      if (userIdAt(seats, m.currentSeatIdx) !== userId)
        throwSenderError(`grid.not_your_turn`);

      const attackerEntity = tx.db.grid.gridEntity.id.find(args.attackerId);
      const targetEntity = tx.db.grid.gridEntity.id.find(args.targetId);
      if (!attackerEntity || !targetEntity)
        throwSenderError(`grid.entity_missing`);

      const type = tx.db.unitType.typeId.find(attacker.typeId);
      if (!type) throwSenderError(`grid.unknown_unit_type:${attacker.typeId}`);

      const dist = distance(
        'hex',
        { x: attackerEntity.x, y: attackerEntity.y },
        { x: targetEntity.x, y: targetEntity.y }
      );
      if (dist > type.attackRange) {
        throwSenderError(
          `grid.target_out_of_range:${dist}>${type.attackRange}`
        );
      }

      const newHp = target.currentHp - type.attackDmg;
      tx.db.playerUnit.entityId.update({ ...attacker, hasAttacked: true });

      if (newHp <= 0) {
        tx.db.playerUnit.delete(target);
        tx.db.grid.gridEntity.delete(targetEntity);

        const teamByUser = participantTeams(tx, attacker.matchId);
        const myTeam = teamByUser.get(userId);
        const remaining = [
          ...tx.db.playerUnit.matchId.filter(attacker.matchId),
        ].filter(u => teamByUser.get(u.ownerUserId) !== myTeam);
        if (remaining.length === 0) {
          tx.db.match.matchId.update({
            ...m,
            status: MatchStatus.Ended,
            winnerUserId: userId,
            updatedAt: ctx.timestamp,
          });
        }
      } else {
        tx.db.playerUnit.entityId.update({ ...target, currentHp: newHp });
      }
    });
    return {};
  }
);

// AI opponent. Triggered by the client after end_turn flips to AI's turn.
// Greedy heuristic: for each AI unit (highest-damage first), attack the
// lowest-HP enemy in range; otherwise move toward the nearest enemy and
// attack after the move if newly in range. Then flips the turn back.

type AiUnit = {
  entityId: bigint;
  typeId: string;
  currentHp: number;
  x: number;
  y: number;
  hasMoved: boolean;
  hasAttacked: boolean;
};
type EnemyUnit = {
  entityId: bigint;
  ownerUserId: string;
  currentHp: number;
  x: number;
  y: number;
};
type UnitTypeSnap = {
  movement: number;
  attackRange: number;
  attackDmg: number;
  hp: number;
};

export const ai_take_turn = spacetimedb.procedure(
  { matchId: t.u64() },
  t.object('AiTakeTurnResult', {
    // One event per acting unit, in execution order. Each event may have a
    // movePath (the A* path the unit walked) and/or an attack (with target
    // snapshot so the client can ghost-render the victim until the attack
    // visually fires after the move animation). Lets the client sequence:
    //   move animation -> brief pause -> attack flash -> target HP drop / death.
    events: t.array(
      t.object('AiTurnEvent', {
        entityId: t.u64(),
        movePath: t.option(
          t.array(t.object('AiPathStep', { x: t.i32(), y: t.i32() }))
        ),
        attack: t.option(
          t.object('AiAttackInfo', {
            targetId: t.u64(),
            damage: t.i32(),
            killed: t.bool(),
            // Target snapshot AT the moment of attack (so the client knows where
            // to draw the ghost while it's pending and what HP to show).
            targetX: t.i32(),
            targetY: t.i32(),
            targetOwner: t.string(),
            targetTypeId: t.string(),
            targetPreHp: t.i32(),
          })
        ),
      })
    ),
  }),
  (ctx, args) => {
    // Any signed-in human in the match can trigger the AI to play. The
    // procedure validates the AI turn before acting, so invalid calls are no-ops.
    requireUserId(ctx);
    type AttackInfo = {
      targetId: bigint;
      damage: number;
      killed: boolean;
      targetX: number;
      targetY: number;
      targetOwner: string;
      targetTypeId: string;
      targetPreHp: number;
    };
    // Fields are `| undefined` (not optional) because spacetimedb's t.option
    // serializer requires them to be present in the runtime shape.
    type TurnEvent = {
      entityId: bigint;
      movePath: Array<{ x: number; y: number }> | undefined;
      attack: AttackInfo | undefined;
    };
    const events: TurnEvent[] = [];

    let gridId = 0n;
    const aiUnits: AiUnit[] = [];
    const enemyUnits: EnemyUnit[] = [];
    const typeIdx = new Map<string, UnitTypeSnap>();
    ctx.withTx(tx => {
      const m = tx.db.match.matchId.find(args.matchId);
      if (!m) throwSenderError(`grid.match_not_found:${args.matchId}`);
      if (m.status.tag !== 'Active')
        throwSenderError(`grid.match_not_active:${m.status.tag}`);
      const seats = participantsBySeat(tx, args.matchId);
      if (userIdAt(seats, m.currentSeatIdx) !== AI_BOT_USER_ID)
        throwSenderError(`grid.not_ai_turn`);
      gridId = m.gridId;
      for (const u of tx.db.playerUnit.matchId.filter(args.matchId)) {
        const e = tx.db.grid.gridEntity.id.find(u.entityId);
        if (!e) continue;
        if (u.ownerUserId === AI_BOT_USER_ID) {
          aiUnits.push({
            entityId: u.entityId,
            typeId: u.typeId,
            currentHp: u.currentHp,
            x: e.x,
            y: e.y,
            hasMoved: u.hasMoved,
            hasAttacked: u.hasAttacked,
          });
        } else {
          enemyUnits.push({
            entityId: u.entityId,
            ownerUserId: u.ownerUserId,
            currentHp: u.currentHp,
            x: e.x,
            y: e.y,
          });
        }
      }
      for (const t of tx.db.unitType.iter()) {
        typeIdx.set(t.typeId, {
          movement: t.movement,
          attackRange: t.attackRange,
          attackDmg: t.attackDmg,
          hp: t.hp,
        });
      }
    });

    const hexDist = (ax: number, ay: number, bx: number, by: number) =>
      distance('hex', { x: ax, y: ay }, { x: bx, y: by });

    // Apply one attack atomically. Returns { info, ended } where `info` is
    // the snapshot needed to animate the attack on the client. A null `info`
    // means no attack fired, and `ended` indicates the match concluded.
    const tryAttack = (
      aiUnit: AiUnit,
      target: EnemyUnit,
      dmg: number
    ): { info: AttackInfo | null; ended: boolean } => {
      let ended = false;
      let info: AttackInfo | null = null;
      ctx.withTx(tx => {
        const attacker = tx.db.playerUnit.entityId.find(aiUnit.entityId);
        const tgt = tx.db.playerUnit.entityId.find(target.entityId);
        if (!attacker || !tgt || attacker.hasAttacked) return;
        const newHp = tgt.currentHp - dmg;
        const tEnt = tx.db.grid.gridEntity.id.find(target.entityId);
        // Capture target snapshot BEFORE the delete/update so the client
        // can ghost-render it during the move animation.
        info = {
          targetId: target.entityId,
          damage: dmg,
          killed: newHp <= 0,
          targetX: tEnt ? tEnt.x : target.x,
          targetY: tEnt ? tEnt.y : target.y,
          targetOwner: tgt.ownerUserId,
          targetTypeId: tgt.typeId,
          targetPreHp: tgt.currentHp,
        };
        tx.db.playerUnit.entityId.update({ ...attacker, hasAttacked: true });
        if (newHp <= 0) {
          tx.db.playerUnit.delete(tgt);
          if (tEnt) tx.db.grid.gridEntity.delete(tEnt);
          const teamByUser = participantTeams(tx, args.matchId);
          const aiTeam = teamByUser.get(AI_BOT_USER_ID);
          const remaining = [
            ...tx.db.playerUnit.matchId.filter(args.matchId),
          ].filter(u => teamByUser.get(u.ownerUserId) !== aiTeam);
          if (remaining.length === 0) {
            const mm = tx.db.match.matchId.find(args.matchId);
            if (mm) {
              tx.db.match.matchId.update({
                ...mm,
                status: MatchStatus.Ended,
                winnerUserId: AI_BOT_USER_ID,
                updatedAt: ctx.timestamp,
              });
              ended = true;
            }
          }
        } else {
          tx.db.playerUnit.entityId.update({ ...tgt, currentHp: newHp });
        }
      });
      // Mirror in our local snapshot for subsequent units' planning.
      aiUnit.hasAttacked = true;
      target.currentHp -= dmg;
      if (target.currentHp <= 0) {
        const idx = enemyUnits.indexOf(target);
        if (idx >= 0) enemyUnits.splice(idx, 1);
      }
      return { info, ended };
    };

    // Best target in attack range: lowest currentHp (greedy lethal-first).
    const pickTarget = (aiUnit: AiUnit, range: number): EnemyUnit | null => {
      const inRange = enemyUnits
        .filter(e => e.currentHp > 0)
        .filter(e => hexDist(aiUnit.x, aiUnit.y, e.x, e.y) <= range);
      if (inRange.length === 0) return null;
      inRange.sort((a, b) => a.currentHp - b.currentHp);
      return inRange[0];
    };

    // High-damage units act first so the kills land before the chip-damage.
    aiUnits.sort(
      (a, b) =>
        (typeIdx.get(b.typeId)?.attackDmg ?? 0) -
        (typeIdx.get(a.typeId)?.attackDmg ?? 0)
    );

    for (const aiUnit of aiUnits) {
      const type = typeIdx.get(aiUnit.typeId);
      if (!type) continue;

      const evt: TurnEvent = {
        entityId: aiUnit.entityId,
        movePath: undefined,
        attack: undefined,
      };

      if (!aiUnit.hasAttacked) {
        const target = pickTarget(aiUnit, type.attackRange);
        if (target) {
          const r = tryAttack(aiUnit, target, type.attackDmg);
          if (r.info) evt.attack = r.info;
          if (r.ended) {
            events.push(evt);
            return { events };
          }
          events.push(evt);
          continue; // already attacked; skip move this turn
        }
      }

      if (!aiUnit.hasMoved && enemyUnits.length > 0) {
        const cells = (
          cellsInRangeImpl(
            ctx.as.grid,
            {
              gridId,
              originX: aiUnit.x,
              originY: aiUnit.y,
              maxCost: type.movement,
            },
            AI_BOT_USER_ID
          ) as { cells: Array<{ x: number; y: number; cost: number }> }
        ).cells;

        // Avoid stepping onto a tile occupied by another known unit (defensive;
        // blocksMovement on entities should already prevent this).
        const blocked = new Set<string>();
        for (const u of aiUnits)
          if (u.entityId !== aiUnit.entityId) blocked.add(`${u.x},${u.y}`);
        for (const e of enemyUnits) blocked.add(`${e.x},${e.y}`);

        const movable = cells.filter(
          c => c.cost > 0 && !blocked.has(`${c.x},${c.y}`)
        );
        if (movable.length > 0) {
          let best = movable[0];
          let bestScore = Infinity;
          for (const c of movable) {
            const closest = Math.min(
              ...enemyUnits.map(e => hexDist(c.x, c.y, e.x, e.y))
            );
            // Primary: closeness to enemy. Tiebreak: prefer cheaper paths.
            const score = closest * 100 + c.cost;
            if (score < bestScore) {
              bestScore = score;
              best = c;
            }
          }
          // Capture the A* path BEFORE the move so the client can animate it.
          const fromX = aiUnit.x,
            fromY = aiUnit.y;
          const pathRes = computePathImpl(
            ctx.as.grid,
            {
              gridId,
              startX: fromX,
              startY: fromY,
              endX: best.x,
              endY: best.y,
              storeFor: undefined,
              maxExpansions: undefined,
            },
            AI_BOT_USER_ID
          ) as {
            found: boolean;
            cells: Array<{ x: number; y: number }>;
            cost: number;
          };
          ctx.withTx(tx => {
            const u = tx.db.playerUnit.entityId.find(aiUnit.entityId);
            const e = tx.db.grid.gridEntity.id.find(aiUnit.entityId);
            if (!u || !e || u.hasMoved) return;
            tx.db.grid.gridEntity.id.update({
              ...e,
              x: best.x,
              y: best.y,
              updatedAt: ctx.timestamp,
            });
            tx.db.playerUnit.entityId.update({ ...u, hasMoved: true });
          });
          aiUnit.x = best.x;
          aiUnit.y = best.y;
          aiUnit.hasMoved = true;
          evt.movePath =
            pathRes.found && pathRes.cells.length >= 2
              ? pathRes.cells
              : [
                  { x: fromX, y: fromY },
                  { x: best.x, y: best.y },
                ];

          if (!aiUnit.hasAttacked) {
            const target = pickTarget(aiUnit, type.attackRange);
            if (target) {
              const r = tryAttack(aiUnit, target, type.attackDmg);
              if (r.info) evt.attack = r.info;
              if (r.ended) {
                events.push(evt);
                return { events };
              }
            }
          }
        }
      }

      if (evt.movePath || evt.attack) events.push(evt);
    }

    ctx.withTx(tx => {
      const m = tx.db.match.matchId.find(args.matchId);
      if (!m || m.status.tag !== 'Active') return;
      const seats = participantsBySeat(tx, args.matchId);
      const nextIdx = (m.currentSeatIdx + 1) % seats.size;
      const nextUserId = userIdAt(seats, nextIdx);
      for (const u of tx.db.playerUnit.matchId.filter(args.matchId)) {
        if (u.ownerUserId === nextUserId) {
          tx.db.playerUnit.entityId.update({
            ...u,
            hasMoved: false,
            hasAttacked: false,
          });
        }
      }
      tx.db.match.matchId.update({
        ...m,
        currentSeatIdx: nextIdx,
        turnNumber: nextIdx === 0 ? m.turnNumber + 1 : m.turnNumber,
        updatedAt: ctx.timestamp,
      });
    });

    return { events };
  }
);

// Query helpers exposed as procedures (so the client can preview
// movement range / paths without subscribing to entity_path).

export const get_cells_in_range = spacetimedb.procedure(
  { gridId: t.u64(), originX: t.i32(), originY: t.i32(), maxCost: t.i32() },
  t.object('CellsInRangeResult', {
    cells: t.array(
      t.object('ReachableCell', {
        x: t.i32(),
        y: t.i32(),
        cost: t.i32(),
      })
    ),
  }),
  (ctx, args) => {
    const userId = requireUserId(ctx);
    return cellsInRangeImpl(ctx.as.grid, args, userId) as {
      cells: Array<{ x: number; y: number; cost: number }>;
    };
  }
);
