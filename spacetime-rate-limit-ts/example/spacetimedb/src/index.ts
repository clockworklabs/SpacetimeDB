import { t, Range, SenderError, type ViewCtx } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { ScheduleAt, Timestamp } from 'spacetimedb';
import {
  COOLANT_UNLOCK_LEVEL,
  PLAYER_COLORS,
  POWER_PER_UPGRADE,
  SURGE_UNLOCK_LEVEL,
  TAP_LIMIT,
  hasCoolantFlush,
  hasSurgeBurst,
  overheatRecoverAt,
  playerColor,
  playerName,
  roomTuning,
  tapLimitForState,
  upgradeOffer,
  upgradeWindowForState,
  type UpgradeLane,
} from './reactor-rules';

const ONE_SECOND_MICROS = 1_000_000n;

const TAP_SCOPE = 'reactor.tap';
const OVERCHARGE_SCOPE = 'reactor.overcharge';
const UPGRADE_SCOPE = 'reactor.upgrade';
const REPAIR_SCOPE = 'reactor.repair';
const SHARED_REACTOR_KEY = 'room';

const TAP_WINDOW_SECONDS = 6;
const OVERCHARGE_LIMIT = 3;
const OVERCHARGE_WINDOW_SECONDS = 25;
const UPGRADE_LIMIT = 2;
const REPAIR_LIMIT = 1;
const REPAIR_WINDOW_SECONDS = 18;

const DEFAULT_RETAIN_EVENTS = 2000;
const DEFAULT_EVENT_PRUNE_BATCH = 500;
const DEFAULT_RETAIN_REACTOR_EVENTS = 80;
const ACTIVE_CREW_WINDOW_SECONDS = 60;
const MAX_CREW_SCALING = 6;
const ADMIN_EVENT_VIEW_LIMIT = 1000;

import {
  rateLimitEvent,
  reactorEvent,
  reactorRoomState,
  rateLimitDemoSweepTick,
  setSweepReducer,
  spacetimedb,
  type Schema,
  type Tx,
} from './schema';
export default spacetimedb;

type ReactorStateRow = NonNullable<
  ReturnType<Tx['db']['reactorRoomState']['singleton']['find']>
>;
type ReactorPlayerRow = NonNullable<
  ReturnType<Tx['db']['reactorPlayerState']['identity']['find']>
>;

function takeRows<T>(rows: Iterable<T>, limit: number): T[] {
  const result: T[] = [];
  for (const row of rows) {
    if (result.length >= limit) break;
    result.push(row);
  }
  return result;
}

const reactorLimitStatusRow = t.object('ReactorLimitStatusRow', {
  scope: t.string(),
  label: t.string(),
  limit: t.u32(),
  windowSeconds: t.u32(),
  used: t.u32(),
  remaining: t.u32(),
  resetAt: t.option(t.timestamp()),
});

const reactorShopItem = t.object('ReactorShopItemRow', {
  slot: t.u32(),
  id: t.string(),
  name: t.string(),
  description: t.string(),
  effect: t.string(),
  cost: t.u64(),
  available: t.bool(),
});

const reactorPlayer = t.object('ReactorPlayerRow', {
  identity: t.identity(),
  displayName: t.string(),
  color: t.string(),
  contributedEnergy: t.u64(),
  taps: t.u32(),
  surges: t.u32(),
  coolantUses: t.u32(),
  upgradesBought: t.u32(),
  joinedAt: t.timestamp(),
  updatedAt: t.timestamp(),
});

const reactorActionResult = t.object('ReactorActionResult', {
  allowed: t.bool(),
  action: t.string(),
  message: t.string(),
  energy: t.u64(),
  energyDelta: t.i64(),
  retryAfterSeconds: t.u32(),
  resetAt: t.timestamp(),
});

function isAdmin(ctx: ViewCtx<Schema>): boolean {
  return (
    ctx.db.rateLimit.rateLimitAdminIdentity.identity.find(ctx.sender) != null
  );
}

function requireAdmin(ctx: Tx): void {
  if (
    ctx.as.rateLimit.db.rateLimitAdminIdentity.identity.find(ctx.sender) == null
  ) {
    throw new SenderError('rate_limit.not_authorized');
  }
}

function actorKey(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

function limitKeyForScope(
  scope: string,
  ctx: { sender: { toHexString(): string } }
): string {
  return scope === UPGRADE_SCOPE ? SHARED_REACTOR_KEY : actorKey(ctx);
}

function bucketKey(scope: string, key: string): string {
  return rateLimit.buildRateLimitKey(scope, key);
}

function clampU32(value: number, min = 0, max = 0xffff_ffff): number {
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

function requireUpgradeLane(value: string): UpgradeLane {
  if (
    value === 'power' ||
    value === 'cooling' ||
    value === 'capacity' ||
    value === 'charges' ||
    value === 'bay'
  )
    return value;
  throw new SenderError('reactor.invalid_upgrade');
}

function activeCrewCount(tx: Tx): number {
  const activeAfter =
    tx.timestamp.microsSinceUnixEpoch -
    BigInt(ACTIVE_CREW_WINDOW_SECONDS) * ONE_SECOND_MICROS;
  let count = 0;
  for (const _row of tx.db.reactorPlayerState.updatedAt.filter(
    new Range({ tag: 'included', value: new Timestamp(activeAfter) })
  ))
    count++;
  return clampU32(Math.max(1, count), 1, MAX_CREW_SCALING);
}

function tunedState(tx: Tx, state: ReactorStateRow): ReactorStateRow {
  const tuning = roomTuning(state, activeCrewCount(tx));
  const heat = clampU32(state.heat, 0, tuning.heatCapacity);
  return {
    ...state,
    ...tuning,
    heat,
    overheated:
      state.overheated && heat > Math.floor(tuning.heatCapacity * 0.45),
  };
}

function requirePlayerColor(color: string): string {
  if (PLAYER_COLORS.includes(color)) return color;
  throw new SenderError('reactor.invalid_player_color');
}

function ensurePlayer(tx: Tx): ReactorPlayerRow {
  const existing = tx.db.reactorPlayerState.identity.find(tx.sender);
  if (existing) return putPlayer(tx, existing);
  const created = {
    identity: tx.sender,
    displayName: playerName(tx.sender),
    color: playerColor(tx.sender),
    contributedEnergy: 0n,
    taps: 0,
    surges: 0,
    coolantUses: 0,
    upgradesBought: 0,
    joinedAt: tx.timestamp,
    updatedAt: tx.timestamp,
  };
  tx.db.reactorPlayerState.insert(created);
  return created;
}

function putPlayer(tx: Tx, player: ReactorPlayerRow): ReactorPlayerRow {
  const next = { ...player, updatedAt: tx.timestamp };
  tx.db.reactorPlayerState.identity.update(next);
  return next;
}

function ensureState(tx: Tx): ReactorStateRow {
  ensurePlayer(tx);
  const existing = tx.db.reactorRoomState.singleton.find(true);
  if (existing) return existing;
  const tuningBase = {
    coolingUpgradeCount: 0,
    capacityUpgradeCount: 0,
  };
  const tuning = roomTuning(tuningBase, activeCrewCount(tx));
  const created = {
    singleton: true,
    energy: 0n,
    reactorLevel: 1,
    upgradeCount: 0,
    powerUpgradeCount: 0,
    coolingUpgradeCount: 0,
    capacityUpgradeCount: 0,
    chargeUpgradeCount: 0,
    bayUpgradeCount: 0,
    combo: 0,
    bestCombo: 0,
    heat: 0,
    heatCapacity: tuning.heatCapacity,
    coolingPerSecond: tuning.coolingPerSecond,
    tapHeatGain: tuning.tapHeatGain,
    overheated: false,
    updatedAt: tx.timestamp,
  };
  tx.db.reactorRoomState.insert(created);
  return created;
}

function putState(tx: Tx, state: ReactorStateRow): ReactorStateRow {
  const next = { ...state, updatedAt: tx.timestamp };
  tx.db.reactorRoomState.singleton.update(next);
  return next;
}

function cooledState(tx: Tx, state: ReactorStateRow): ReactorStateRow {
  const tuned = tunedState(tx, state);
  const elapsedSeconds = Number(
    (tx.timestamp.microsSinceUnixEpoch - tuned.updatedAt.microsSinceUnixEpoch) /
      ONE_SECOND_MICROS
  );
  if (
    elapsedSeconds <= 0 &&
    tuned.heat === state.heat &&
    tuned.heatCapacity === state.heatCapacity &&
    tuned.coolingPerSecond === state.coolingPerSecond &&
    tuned.tapHeatGain === state.tapHeatGain &&
    tuned.overheated === state.overheated
  )
    return state;

  const heat = Math.max(
    0,
    tuned.heat - Math.max(0, elapsedSeconds) * tuned.coolingPerSecond
  );
  const overheated = tuned.overheated && heat > overheatRecoverAt(tuned);
  if (
    heat === state.heat &&
    overheated === state.overheated &&
    tuned.heatCapacity === state.heatCapacity &&
    tuned.coolingPerSecond === state.coolingPerSecond &&
    tuned.tapHeatGain === state.tapHeatGain
  )
    return state;

  return putState(tx, {
    ...tuned,
    heat,
    overheated,
  });
}

function currentState(tx: Tx): ReactorStateRow {
  return cooledState(tx, ensureState(tx));
}

function pruneRateLimitEvents(
  tx: Tx,
  retainEvents: number,
  pruneBatch: number
): number {
  const total = Number(tx.db.rateLimitEvent.count());
  if (total <= retainEvents) return 0;

  const toDelete = Math.min(pruneBatch, total - retainEvents);
  let deleted = 0;
  for (const row of tx.db.rateLimitEvent.createdAt.filter(new Range())) {
    if (deleted >= toDelete) break;
    tx.db.rateLimitEvent.delete(row);
    deleted++;
  }
  return deleted;
}

function pruneReactorEvents(tx: Tx): void {
  const rows = [...tx.db.reactorEvent.iter()].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  );
  const extra = rows.length - DEFAULT_RETAIN_REACTOR_EVENTS;
  if (extra <= 0) return;
  for (let i = 0; i < extra; i++) tx.db.reactorEvent.delete(rows[i]);
}

function toU32(name: string, value: number): number {
  if (!Number.isInteger(value) || value <= 0 || value > 0xffff_ffff) {
    throw new Error(`rate_limit.invalid_${name}`);
  }
  return value;
}

function recordLimitHit(
  tx: Tx,
  args: {
    scope: string;
    key: string;
    limit: number;
    windowSeconds: number;
    cost: number;
    allowed: boolean;
    used: number;
    remaining: number;
    retryAfterSeconds: number;
    resetAt: Tx['timestamp'];
  }
) {
  tx.db.rateLimitEvent.insert({
    id: 0n,
    scope: args.scope,
    key: args.key,
    allowed: args.allowed,
    limit: args.limit,
    used: args.used,
    remaining: args.remaining,
    retryAfterSeconds: args.retryAfterSeconds,
    windowSeconds: args.windowSeconds,
    cost: args.cost,
    resetAt: args.resetAt,
    createdAt: tx.timestamp,
  });
}

function recordReactorEvent(
  tx: Tx,
  args: {
    kind: string;
    scope: string;
    message: string;
    allowed: boolean;
    energyDelta?: bigint;
    retryAfterSeconds?: number;
  }
) {
  const player = ensurePlayer(tx);
  tx.db.reactorEvent.insert({
    id: 0n,
    identity: tx.sender,
    actorName: player.displayName,
    actorColor: player.color,
    kind: args.kind,
    scope: args.scope,
    message: args.message,
    allowed: args.allowed,
    energyDelta: args.energyDelta ?? 0n,
    retryAfterSeconds: args.retryAfterSeconds ?? 0,
    createdAt: tx.timestamp,
  });
  pruneReactorEvents(tx);
}

function consumeAction(
  tx: Tx,
  scope: string,
  actorKey: string,
  limit: number,
  windowSeconds: number,
  cost = 1
) {
  return rateLimit.consumeRateLimit(tx.as.rateLimit, {
    key: rateLimit.buildRateLimitKey(scope, actorKey),
    scope,
    limit,
    windowSeconds,
    cost,
  });
}

function emptyActionResult(tx: Tx, action: string, message: string) {
  const state = currentState(tx);
  return {
    allowed: true,
    action,
    message,
    energy: state.energy,
    energyDelta: 0n,
    retryAfterSeconds: 0,
    resetAt: tx.timestamp,
  };
}

export const init = spacetimedb.init(ctx => {
  rateLimit.installRateLimit(ctx.as.rateLimit);
  if (ctx.db.rateLimitDemoConfig.singleton.find(true) == null) {
    ctx.db.rateLimitDemoConfig.insert({
      singleton: true,
      retainEvents: DEFAULT_RETAIN_EVENTS,
      eventPruneBatch: DEFAULT_EVENT_PRUNE_BATCH,
      updatedAt: ctx.timestamp,
    });
  }
  if (ctx.db.rateLimitDemoSweepTick.count() === 0n) {
    ctx.db.rateLimitDemoSweepTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(30n * ONE_SECOND_MICROS),
    });
  }
});

export const reactorState = spacetimedb.view(
  { name: 'reactor_state', public: true },
  t.array(reactorRoomState.rowType),
  ctx => {
    const row = ctx.db.reactorRoomState.singleton.find(true);
    return row ? [row] : [];
  }
);

export const reactorEvents = spacetimedb.view(
  { name: 'reactor_events', public: true },
  t.array(reactorEvent.rowType),
  ctx =>
    [...ctx.db.reactorEvent.iter()]
      .sort((a, b) => (a.id < b.id ? 1 : a.id > b.id ? -1 : 0))
      .slice(0, DEFAULT_RETAIN_REACTOR_EVENTS)
);

export const reactorPlayers = spacetimedb.view(
  { name: 'reactor_players', public: true },
  t.array(reactorPlayer),
  ctx =>
    [...ctx.db.reactorPlayerState.iter()]
      .sort((a, b) => {
        if (a.contributedEnergy === b.contributedEnergy)
          return a.displayName.localeCompare(b.displayName);
        return a.contributedEnergy < b.contributedEnergy ? 1 : -1;
      })
      .slice(0, 16)
);

export const reactorLimitStatus = spacetimedb.view(
  { name: 'reactor_limit_status', public: true },
  t.array(reactorLimitStatusRow),
  ctx => {
    const state = ctx.db.reactorRoomState.singleton.find(true);
    const tapLimit = state ? tapLimitForState(state) : TAP_LIMIT;
    const upgradeWindowSeconds = upgradeWindowForState(state);
    const specs = [
      {
        scope: TAP_SCOPE,
        label: 'Tap Coils',
        limit: tapLimit,
        windowSeconds: TAP_WINDOW_SECONDS,
      },
      {
        scope: OVERCHARGE_SCOPE,
        label: 'Overcharger',
        limit: OVERCHARGE_LIMIT,
        windowSeconds: OVERCHARGE_WINDOW_SECONDS,
      },
      {
        scope: UPGRADE_SCOPE,
        label: 'Upgrade Bay',
        limit: UPGRADE_LIMIT,
        windowSeconds: upgradeWindowSeconds,
      },
      {
        scope: REPAIR_SCOPE,
        label: 'Repair Drones',
        limit: REPAIR_LIMIT,
        windowSeconds: REPAIR_WINDOW_SECONDS,
      },
    ];
    return specs.map(spec => {
      const key = limitKeyForScope(spec.scope, ctx);
      const bucket = ctx.db.rateLimit.rateLimitBucket.key.find(
        bucketKey(spec.scope, key)
      );
      const used = Number(bucket?.count ?? 0);
      return {
        ...spec,
        used,
        remaining: clampU32(spec.limit - used),
        resetAt: bucket?.expiresAt,
      };
    });
  }
);

export const reactorShop = spacetimedb.view(
  { name: 'reactor_shop', public: true },
  t.array(reactorShopItem),
  ctx => {
    const state = ctx.db.reactorRoomState.singleton.find(true);
    const base = state ?? {
      powerUpgradeCount: 0,
      coolingUpgradeCount: 0,
      capacityUpgradeCount: 0,
      chargeUpgradeCount: 0,
      bayUpgradeCount: 0,
    };
    return (['power', 'cooling', 'capacity', 'charges', 'bay'] as const).map(
      lane => {
        const offer = upgradeOffer(base, lane);
        return {
          slot: offer.slot,
          id: offer.id,
          name: offer.name,
          description: offer.description,
          effect: offer.effect,
          cost: offer.cost,
          available: true,
        };
      }
    );
  }
);

export const rateLimitEventsAdmin = spacetimedb.view(
  { name: 'rate_limit_events_admin', public: true },
  t.array(rateLimitEvent.rowType),
  ctx =>
    isAdmin(ctx)
      ? takeRows(ctx.db.rateLimitEvent.iter(), ADMIN_EVENT_VIEW_LIMIT)
      : []
);

export const start_reactor = spacetimedb.procedure(
  {},
  reactorActionResult,
  ctx => {
    let out: ReturnType<typeof emptyActionResult> | null = null;
    ctx.withTx(tx => {
      ensurePlayer(tx);
      out = emptyActionResult(tx, 'start', 'Reactor online.');
    });
    if (!out) throw new Error('reactor.start_tx_failed');
    return out;
  }
);

export const tap_reactor = spacetimedb.procedure(
  {},
  reactorActionResult,
  ctx => {
    const key = actorKey(ctx);
    let out: ReturnType<typeof emptyActionResult> | null = null;
    ctx.withTx(tx => {
      const tapLimit = tapLimitForState(currentState(tx));
      const result = consumeAction(
        tx,
        TAP_SCOPE,
        key,
        tapLimit,
        TAP_WINDOW_SECONDS
      );
      recordLimitHit(tx, {
        ...result,
        limit: tapLimit,
        windowSeconds: TAP_WINDOW_SECONDS,
        cost: 1,
      });
      const state = currentState(tx);
      if (!result.allowed) {
        const next = putState(tx, {
          ...state,
          combo: 0,
        });
        const message = `Tap coils cooling for ${result.retryAfterSeconds}s.`;
        recordReactorEvent(tx, {
          kind: 'tap_rate_limited',
          scope: TAP_SCOPE,
          message,
          allowed: false,
          retryAfterSeconds: result.retryAfterSeconds,
        });
        out = {
          allowed: false,
          action: 'tap',
          message,
          energy: next.energy,
          energyDelta: 0n,
          retryAfterSeconds: result.retryAfterSeconds,
          resetAt: result.resetAt,
        };
        return;
      }
      if (state.overheated) {
        const message = 'Core is overheated.';
        recordReactorEvent(tx, {
          kind: 'tap_heat_blocked',
          scope: TAP_SCOPE,
          message,
          allowed: false,
        });
        out = {
          allowed: false,
          action: 'tap',
          message,
          energy: state.energy,
          energyDelta: 0n,
          retryAfterSeconds: 0,
          resetAt: result.resetAt,
        };
        return;
      }

      const gain = BigInt(state.reactorLevel);
      const player = ensurePlayer(tx);
      const combo = clampU32(state.combo + 1);
      const heat = clampU32(
        state.heat + state.tapHeatGain,
        0,
        state.heatCapacity
      );
      const next = putState(tx, {
        ...state,
        energy: state.energy + gain,
        combo,
        bestCombo: Math.max(state.bestCombo, combo),
        heat,
        overheated: heat >= state.heatCapacity,
      });
      putPlayer(tx, {
        ...player,
        contributedEnergy: player.contributedEnergy + gain,
        taps: clampU32(player.taps + 1),
      });
      const message =
        heat >= state.heatCapacity
          ? 'Energy gained, but the core overheated.'
          : `+${gain.toString()} energy.`;
      recordReactorEvent(tx, {
        kind: heat >= state.heatCapacity ? 'tap_overheated' : 'tap',
        scope: TAP_SCOPE,
        message,
        allowed: true,
        energyDelta: gain,
      });
      out = {
        allowed: true,
        action: 'tap',
        message,
        energy: next.energy,
        energyDelta: gain,
        retryAfterSeconds: 0,
        resetAt: result.resetAt,
      };
    });
    if (!out) throw new Error('reactor.tap_tx_failed');
    return out;
  }
);

export const overcharge = spacetimedb.procedure(
  {},
  reactorActionResult,
  ctx => {
    let locked: {
      allowed: boolean;
      action: string;
      message: string;
      energy: bigint;
      energyDelta: bigint;
      retryAfterSeconds: number;
      resetAt: Tx['timestamp'];
    } | null = null;
    ctx.withTx(tx => {
      const state = currentState(tx);
      if (hasSurgeBurst(state)) return;
      const message = `Install Plasma Coils level ${SURGE_UNLOCK_LEVEL} to unlock Surge Burst.`;
      recordReactorEvent(tx, {
        kind: 'system_locked',
        scope: OVERCHARGE_SCOPE,
        message,
        allowed: false,
      });
      locked = {
        allowed: false,
        action: 'overcharge',
        message,
        energy: state.energy,
        energyDelta: 0n,
        retryAfterSeconds: 0,
        resetAt: tx.timestamp,
      };
    });
    if (locked) return locked;

    const key = actorKey(ctx);
    let out: ReturnType<typeof emptyActionResult> | null = null;
    ctx.withTx(tx => {
      const result = consumeAction(
        tx,
        OVERCHARGE_SCOPE,
        key,
        OVERCHARGE_LIMIT,
        OVERCHARGE_WINDOW_SECONDS
      );
      recordLimitHit(tx, {
        ...result,
        windowSeconds: OVERCHARGE_WINDOW_SECONDS,
        cost: 1,
      });
      const state = currentState(tx);
      if (!result.allowed || state.overheated) {
        const message = result.allowed
          ? 'Core is still cooling from the last overheat.'
          : `Surge vents cooling down for ${result.retryAfterSeconds}s.`;
        const next = putState(tx, {
          ...state,
          combo: 0,
          overheated: state.overheated,
        });
        recordReactorEvent(tx, {
          kind: 'overcharge_blocked',
          scope: OVERCHARGE_SCOPE,
          message,
          allowed: false,
          retryAfterSeconds: result.retryAfterSeconds,
        });
        out = {
          allowed: false,
          action: 'overcharge',
          message,
          energy: next.energy,
          energyDelta: 0n,
          retryAfterSeconds: result.retryAfterSeconds,
          resetAt: result.resetAt,
        };
        return;
      }

      const gain = BigInt(state.reactorLevel * 10);
      const player = ensurePlayer(tx);
      const surgeHeat = Math.max(28, Math.floor(state.heatCapacity * 0.25));
      const heat = clampU32(state.heat + surgeHeat, 0, state.heatCapacity);
      const combo = clampU32(state.combo + 3);
      const next = putState(tx, {
        ...state,
        energy: state.energy + gain,
        combo,
        bestCombo: Math.max(state.bestCombo, combo),
        heat,
        overheated: heat >= state.heatCapacity,
      });
      putPlayer(tx, {
        ...player,
        contributedEnergy: player.contributedEnergy + gain,
        surges: clampU32(player.surges + 1),
      });
      const message =
        heat >= state.heatCapacity
          ? `Surge Burst yielded +${gain.toString()} and blew the safeties.`
          : `Surge Burst yielded +${gain.toString()} energy.`;
      recordReactorEvent(tx, {
        kind:
          heat >= state.heatCapacity ? 'overcharge_overheated' : 'overcharge',
        scope: OVERCHARGE_SCOPE,
        message,
        allowed: true,
        energyDelta: gain,
      });
      out = {
        allowed: true,
        action: 'overcharge',
        message,
        energy: next.energy,
        energyDelta: gain,
        retryAfterSeconds: 0,
        resetAt: result.resetAt,
      };
    });
    if (!out) throw new Error('reactor.overcharge_tx_failed');
    return out;
  }
);

export const buy_upgrade = spacetimedb.procedure(
  { upgradeId: t.string() },
  reactorActionResult,
  (ctx, args) => {
    const lane = requireUpgradeLane(args.upgradeId);
    const key = limitKeyForScope(UPGRADE_SCOPE, ctx);
    let out: ReturnType<typeof emptyActionResult> | null = null;
    ctx.withTx(tx => {
      const upgradeWindowSeconds = upgradeWindowForState(
        tx.db.reactorRoomState.singleton.find(true)
      );
      const result = consumeAction(
        tx,
        UPGRADE_SCOPE,
        key,
        UPGRADE_LIMIT,
        upgradeWindowSeconds
      );
      recordLimitHit(tx, {
        ...result,
        windowSeconds: upgradeWindowSeconds,
        cost: 1,
      });
      const state = currentState(tx);
      const offer = upgradeOffer(state, lane);
      const cost = offer.cost;
      if (!result.allowed) {
        const message = `Upgrade bay cooling down for ${result.retryAfterSeconds}s.`;
        recordReactorEvent(tx, {
          kind: 'upgrade_blocked',
          scope: UPGRADE_SCOPE,
          message,
          allowed: false,
          retryAfterSeconds: result.retryAfterSeconds,
        });
        out = {
          allowed: false,
          action: 'upgrade',
          message,
          energy: state.energy,
          energyDelta: 0n,
          retryAfterSeconds: result.retryAfterSeconds,
          resetAt: result.resetAt,
        };
        return;
      }
      if (state.energy < cost) {
        const message = `Need ${cost.toString()} energy for ${offer.name}.`;
        recordReactorEvent(tx, {
          kind: 'upgrade_insufficient_energy',
          scope: UPGRADE_SCOPE,
          message,
          allowed: false,
        });
        out = {
          allowed: false,
          action: 'upgrade',
          message,
          energy: state.energy,
          energyDelta: 0n,
          retryAfterSeconds: 0,
          resetAt: result.resetAt,
        };
        return;
      }

      const player = ensurePlayer(tx);
      const upgradeCount = clampU32(state.upgradeCount + 1);
      const powerUpgradeCount =
        lane === 'power'
          ? clampU32(state.powerUpgradeCount + 1)
          : state.powerUpgradeCount;
      const coolingUpgradeCount =
        lane === 'cooling'
          ? clampU32(state.coolingUpgradeCount + 1)
          : state.coolingUpgradeCount;
      const capacityUpgradeCount =
        lane === 'capacity'
          ? clampU32(state.capacityUpgradeCount + 1)
          : state.capacityUpgradeCount;
      const chargeUpgradeCount =
        lane === 'charges'
          ? clampU32(state.chargeUpgradeCount + 1)
          : state.chargeUpgradeCount;
      const bayUpgradeCount =
        lane === 'bay'
          ? clampU32(state.bayUpgradeCount + 1)
          : state.bayUpgradeCount;
      const tuned = tunedState(tx, {
        ...state,
        upgradeCount,
        powerUpgradeCount,
        coolingUpgradeCount,
        capacityUpgradeCount,
        chargeUpgradeCount,
        bayUpgradeCount,
        reactorLevel: clampU32(1 + powerUpgradeCount * POWER_PER_UPGRADE),
      });
      const next = putState(tx, {
        ...tuned,
        energy: state.energy - cost,
        heat: Math.max(0, tuned.heat - 18),
        overheated: false,
      });
      putPlayer(tx, {
        ...player,
        upgradesBought: clampU32(player.upgradesBought + 1),
      });
      const delta = -cost;
      const message = `${offer.name} installed. ${offer.effect}.`;
      recordReactorEvent(tx, {
        kind: 'upgrade',
        scope: UPGRADE_SCOPE,
        message,
        allowed: true,
        energyDelta: delta,
      });
      out = {
        allowed: true,
        action: 'upgrade',
        message,
        energy: next.energy,
        energyDelta: delta,
        retryAfterSeconds: 0,
        resetAt: result.resetAt,
      };
    });
    if (!out) throw new Error('reactor.upgrade_tx_failed');
    return out;
  }
);

export const repair_reactor = spacetimedb.procedure(
  {},
  reactorActionResult,
  ctx => {
    let locked: {
      allowed: boolean;
      action: string;
      message: string;
      energy: bigint;
      energyDelta: bigint;
      retryAfterSeconds: number;
      resetAt: Tx['timestamp'];
    } | null = null;
    ctx.withTx(tx => {
      const state = currentState(tx);
      if (hasCoolantFlush(state)) return;
      const message = `Install Thermal Vents level ${COOLANT_UNLOCK_LEVEL} to unlock Coolant Flush.`;
      recordReactorEvent(tx, {
        kind: 'system_locked',
        scope: REPAIR_SCOPE,
        message,
        allowed: false,
      });
      locked = {
        allowed: false,
        action: 'repair',
        message,
        energy: state.energy,
        energyDelta: 0n,
        retryAfterSeconds: 0,
        resetAt: tx.timestamp,
      };
    });
    if (locked) return locked;

    const key = actorKey(ctx);
    let out: ReturnType<typeof emptyActionResult> | null = null;
    ctx.withTx(tx => {
      const result = consumeAction(
        tx,
        REPAIR_SCOPE,
        key,
        REPAIR_LIMIT,
        REPAIR_WINDOW_SECONDS
      );
      recordLimitHit(tx, {
        ...result,
        windowSeconds: REPAIR_WINDOW_SECONDS,
        cost: 1,
      });
      const state = currentState(tx);
      if (!result.allowed) {
        const message = `Coolant system cooling down for ${result.retryAfterSeconds}s.`;
        recordReactorEvent(tx, {
          kind: 'repair_blocked',
          scope: REPAIR_SCOPE,
          message,
          allowed: false,
          retryAfterSeconds: result.retryAfterSeconds,
        });
        out = {
          allowed: false,
          action: 'repair',
          message,
          energy: state.energy,
          energyDelta: 0n,
          retryAfterSeconds: result.retryAfterSeconds,
          resetAt: result.resetAt,
        };
        return;
      }

      const next = putState(tx, {
        ...state,
        combo: 0,
        heat: Math.max(
          0,
          state.heat - Math.max(65, Math.floor(state.heatCapacity * 0.55))
        ),
        overheated: false,
      });
      const player = ensurePlayer(tx);
      putPlayer(tx, {
        ...player,
        coolantUses: clampU32(player.coolantUses + 1),
      });
      const message = 'Coolant flushed.';
      recordReactorEvent(tx, {
        kind: 'repair',
        scope: REPAIR_SCOPE,
        message,
        allowed: true,
      });
      out = {
        allowed: true,
        action: 'repair',
        message,
        energy: next.energy,
        energyDelta: 0n,
        retryAfterSeconds: 0,
        resetAt: result.resetAt,
      };
    });
    if (!out) throw new Error('reactor.repair_tx_failed');
    return out;
  }
);

export const runSweep = spacetimedb.procedure(
  { maxRows: t.option(t.u32()) },
  t.u32(),
  (ctx, args) => {
    const maxRows =
      args.maxRows === undefined
        ? undefined
        : toU32('sweep_batch', Number(args.maxRows));
    const deleted = rateLimit.runSweep(ctx.as.rateLimit, { maxRows });
    ctx.withTx(tx => {
      const demo = tx.db.rateLimitDemoConfig.singleton.find(true);
      const retainEvents = Number(demo?.retainEvents ?? DEFAULT_RETAIN_EVENTS);
      const pruneBatch = Number(
        demo?.eventPruneBatch ?? DEFAULT_EVENT_PRUNE_BATCH
      );
      pruneRateLimitEvents(tx, retainEvents, pruneBatch);
    });
    return deleted;
  }
);

export const set_player_color = spacetimedb.reducer(
  { color: t.string() },
  (ctx, args) => {
    const color = requirePlayerColor(args.color);
    const player = ensurePlayer(ctx);
    putPlayer(ctx, {
      ...player,
      color,
    });
  }
);

export const resetDemo = spacetimedb.reducer({}, ctx => {
  requireAdmin(ctx);
  rateLimit.resetBuckets(ctx.as.rateLimit, {});
  for (const row of ctx.db.rateLimitEvent.iter())
    ctx.db.rateLimitEvent.delete(row);
  for (const row of ctx.db.reactorEvent.iter()) ctx.db.reactorEvent.delete(row);
  for (const row of ctx.db.reactorPlayerState.iter())
    ctx.db.reactorPlayerState.delete(row);
  for (const row of ctx.db.reactorRoomState.iter())
    ctx.db.reactorRoomState.delete(row);
});

export const updateConfig = spacetimedb.reducer(
  {
    sweepBatch: t.option(t.u32()),
    retainEvents: t.option(t.u32()),
    eventPruneBatch: t.option(t.u32()),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    if (args.sweepBatch !== undefined) {
      rateLimit.updateConfig(ctx.as.rateLimit, {
        sweepBatch: toU32('sweep_batch', Number(args.sweepBatch)),
      });
    }
    if (args.retainEvents !== undefined || args.eventPruneBatch !== undefined) {
      const demo = ctx.db.rateLimitDemoConfig.singleton.find(true);
      if (!demo) throw new Error('rate_limit.demo_config_missing');
      ctx.db.rateLimitDemoConfig.singleton.update({
        ...demo,
        retainEvents:
          args.retainEvents === undefined
            ? demo.retainEvents
            : toU32('retain_events', Number(args.retainEvents)),
        eventPruneBatch:
          args.eventPruneBatch === undefined
            ? demo.eventPruneBatch
            : toU32('event_prune_batch', Number(args.eventPruneBatch)),
        updatedAt: ctx.timestamp,
      });
    }
  }
);

export const rate_limit_demo_sweep = spacetimedb.reducer(
  { arg: rateLimitDemoSweepTick.rowType },
  (ctx, _args) => {
    const demo = ctx.db.rateLimitDemoConfig.singleton.find(true);
    const retainEvents = Number(demo?.retainEvents ?? DEFAULT_RETAIN_EVENTS);
    const pruneBatch = Number(
      demo?.eventPruneBatch ?? DEFAULT_EVENT_PRUNE_BATCH
    );
    pruneRateLimitEvents(ctx, retainEvents, pruneBatch);
  }
);

setSweepReducer(rate_limit_demo_sweep);
