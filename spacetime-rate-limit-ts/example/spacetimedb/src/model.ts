import { table, t } from 'spacetimedb/server';

export const rateLimitEvent = table(
  { name: 'rate_limit_event', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    scope: t.string().index(),
    key: t.string(),
    allowed: t.bool().index(),
    limit: t.u32(),
    used: t.u32(),
    remaining: t.u32(),
    retryAfterSeconds: t.u32(),
    windowSeconds: t.u32(),
    cost: t.u32(),
    resetAt: t.timestamp(),
    createdAt: t.timestamp().index(),
  }
);
export const reactorRoomState = table(
  { name: 'reactor_room_state', public: false },
  {
    singleton: t.bool().primaryKey(),
    energy: t.u64(),
    reactorLevel: t.u32(),
    upgradeCount: t.u32(),
    powerUpgradeCount: t.u32(),
    coolingUpgradeCount: t.u32(),
    capacityUpgradeCount: t.u32(),
    chargeUpgradeCount: t.u32(),
    bayUpgradeCount: t.u32(),
    combo: t.u32(),
    bestCombo: t.u32(),
    heat: t.u32(),
    heatCapacity: t.u32(),
    coolingPerSecond: t.u32(),
    tapHeatGain: t.u32(),
    overheated: t.bool(),
    updatedAt: t.timestamp(),
  }
);

export const reactorPlayerState = table(
  { name: 'reactor_player_state', public: false },
  {
    identity: t.identity().primaryKey(),
    displayName: t.string(),
    color: t.string(),
    contributedEnergy: t.u64(),
    taps: t.u32(),
    surges: t.u32(),
    coolantUses: t.u32(),
    upgradesBought: t.u32(),
    joinedAt: t.timestamp(),
    updatedAt: t.timestamp().index(),
  }
);

export const reactorEvent = table(
  { name: 'reactor_event', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    identity: t.identity().index(),
    actorName: t.string(),
    actorColor: t.string(),
    kind: t.string().index(),
    scope: t.string(),
    message: t.string(),
    allowed: t.bool().index(),
    energyDelta: t.i64(),
    retryAfterSeconds: t.u32(),
    createdAt: t.timestamp().index(),
  }
);
