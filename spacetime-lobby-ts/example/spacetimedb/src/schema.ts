import {
  schema,
  table,
  t,
  type Infer,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

export const DUEL_POOL = 'spaceship_duel';
export const AI_DUEL_POOL_PREFIX = 'spaceship_duel_ai';
export const RATING_POOL = DUEL_POOL;
export const MATCH_SIZE = 2;
export const DISPLAY_NAME_MAX = 32;

export const shipClass = t.enum('ShipClass', [
  'Bulwark',
  'Interceptor',
  'Phantom',
  'Artillery',
]);
export const ShipClass = {
  Bulwark: { tag: 'Bulwark' as const },
  Interceptor: { tag: 'Interceptor' as const },
  Phantom: { tag: 'Phantom' as const },
  Artillery: { tag: 'Artillery' as const },
};

export const maneuverSlot = t.enum('ManeuverSlot', [
  'Primary',
  'Defensive',
  'Risky',
]);
export const ManeuverSlot = {
  Primary: { tag: 'Primary' as const },
  Defensive: { tag: 'Defensive' as const },
  Risky: { tag: 'Risky' as const },
};

export const duelStatus = t.enum('DuelStatus', [
  'Configuring',
  'Active',
  'Complete',
  'Abandoned',
]);
export const DuelStatus = {
  Configuring: { tag: 'Configuring' as const },
  Active: { tag: 'Active' as const },
  Complete: { tag: 'Complete' as const },
  Abandoned: { tag: 'Abandoned' as const },
};

export const pilot = table(
  {
    name: 'pilot',
    public: false,
    indexes: [
      { accessor: 'byUpdatedAt', algorithm: 'btree', columns: ['updatedAt'] },
    ],
  },
  {
    subject: t.string().primaryKey(),
    displayName: t.string(),
    shipClass,
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

export const shipCatalog = table(
  {
    name: 'ship_catalog',
    public: true,
    indexes: [
      { accessor: 'byShipClass', algorithm: 'btree', columns: ['shipClass'] },
    ],
  },
  {
    shipId: t.string().primaryKey(),
    shipClass,
    role: t.string(),
    description: t.string(),
    hull: t.u32(),
    shields: t.u32(),
    attack: t.u32(),
    defense: t.u32(),
    speed: t.u32(),
    critBps: t.u32(),
    dodgeBps: t.u32(),
  }
);

export const maneuverCatalog = table(
  {
    name: 'maneuver_catalog',
    public: true,
    indexes: [
      { accessor: 'byShipClass', algorithm: 'btree', columns: ['shipClass'] },
      { accessor: 'bySlot', algorithm: 'btree', columns: ['slot'] },
    ],
  },
  {
    maneuverId: t.string().primaryKey(),
    shipClass,
    slot: maneuverSlot,
    name: t.string(),
    description: t.string(),
    damageBps: t.i32(),
    defenseBps: t.i32(),
    shieldRestore: t.u32(),
    selfShieldCost: t.u32(),
    critBonusBps: t.i32(),
    dodgeBonusBps: t.i32(),
  }
);

export const duel = table(
  {
    name: 'duel',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      { accessor: 'byUpdatedAt', algorithm: 'btree', columns: ['updatedAt'] },
    ],
  },
  {
    roomId: t.u64().primaryKey(),
    status: duelStatus,
    round: t.u32(),
    winnerSubject: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

export const duelCombatant = table(
  {
    name: 'duel_combatant',
    public: false,
    indexes: [
      { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'bySubject', algorithm: 'btree', columns: ['subject'] },
    ],
  },
  {
    combatantId: t.string().primaryKey(),
    roomId: t.u64(),
    subject: t.string(),
    displayName: t.string(),
    shipClass,
    hull: t.u32(),
    maxHull: t.u32(),
    shields: t.u32(),
    maxShields: t.u32(),
    attack: t.u32(),
    defense: t.u32(),
    speed: t.u32(),
    critBps: t.u32(),
    dodgeBps: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const duelRoundLog = table(
  {
    name: 'duel_round_log',
    public: false,
    indexes: [
      { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
    ],
  },
  {
    logId: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    round: t.u32(),
    message: t.string(),
    createdAt: t.timestamp(),
  }
);

export const duelManeuver = table(
  {
    name: 'duel_maneuver',
    public: false,
    indexes: [
      { accessor: 'byRoom', algorithm: 'btree', columns: ['roomId'] },
      { accessor: 'bySubject', algorithm: 'btree', columns: ['subject'] },
    ],
  },
  {
    choiceId: t.string().primaryKey(),
    roomId: t.u64(),
    round: t.u32(),
    subject: t.string(),
    slot: maneuverSlot,
    maneuverId: t.string(),
    chosenAt: t.timestamp(),
  }
);

export const queueSummaryRow = t.object('ExampleLobbyQueueSummaryRow', {
  pool: t.string(),
  queuedTickets: t.u32(),
  readyRooms: t.u32(),
  activeRooms: t.u32(),
});

export const ratingRow = t.object('ExampleLobbyRatingRow', {
  pool: t.string(),
  subject: t.string(),
  rating: t.i32(),
  wins: t.u32(),
  losses: t.u32(),
  draws: t.u32(),
  matches: t.u32(),
});

export const spacetimedb = schema({
  lobby,
  shipCatalog,
  maneuverCatalog,
  pilot,
  duel,
  duelCombatant,
  duelRoundLog,
  duelManeuver,
});

export type Schema = InferSchema<typeof spacetimedb>;
export type WriteCtx = ReducerCtx<Schema>;
export type ReadCtx = WriteCtx | ViewCtx<Schema>;
export type CombatantRow = Infer<typeof duelCombatant.rowType>;
export type ManeuverRow = Infer<typeof maneuverCatalog.rowType>;
export type DuelRow = Infer<typeof duel.rowType>;
export type ShipClassValue = (typeof ShipClass)[keyof typeof ShipClass];
export type ManeuverSlotValue =
  (typeof ManeuverSlot)[keyof typeof ManeuverSlot];

export default spacetimedb;
