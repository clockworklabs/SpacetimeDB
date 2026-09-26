export const TAP_LIMIT = 8;
export const UPGRADE_WINDOW_SECONDS = 20;
export const COOLANT_UNLOCK_LEVEL = 2;
export const SURGE_UNLOCK_LEVEL = 2;

const BASE_HEAT_CAPACITY = 100;
const BASE_HEAT_COOL_PER_SECOND = 4;
const BASE_TAP_HEAT_GAIN = 12;
const MAX_CREW_SCALING = 6;
const HEAT_CAPACITY_PER_EXTRA_CREW = 75;
const COOLING_PER_EXTRA_CREW = 4;
const POWER_UPGRADE_BASE_COST = 12;
const COOLING_UPGRADE_BASE_COST = 16;
const CAPACITY_UPGRADE_BASE_COST = 14;
const CHARGE_UPGRADE_BASE_COST = 18;
const BAY_UPGRADE_BASE_COST = 20;
const POWER_UPGRADE_COST_STEP = 18;
const COOLING_UPGRADE_COST_STEP = 18;
const CAPACITY_UPGRADE_COST_STEP = 20;
const CHARGE_UPGRADE_COST_STEP = 22;
const BAY_UPGRADE_COST_STEP = 24;
export const POWER_PER_UPGRADE = 1;
const COOLING_PER_UPGRADE = 2;
const CAPACITY_PER_UPGRADE = 30;
const CHARGES_PER_UPGRADE = 2;
const UPGRADE_WINDOW_REDUCTION_SECONDS = 3;
const MIN_UPGRADE_WINDOW_SECONDS = 5;

export const PLAYER_COLORS = [
  '#22c7b8',
  '#ffce5c',
  '#52df8f',
  '#ff6a66',
  '#aee8ff',
  '#d28cff',
  '#ff9f6e',
  '#8ddf65',
];

export type UpgradeLane = 'power' | 'cooling' | 'capacity' | 'charges' | 'bay';

export interface UpgradeState {
  powerUpgradeCount: number;
  coolingUpgradeCount: number;
  capacityUpgradeCount: number;
  chargeUpgradeCount: number;
  bayUpgradeCount: number;
}

export interface UpgradeOffer {
  slot: number;
  id: UpgradeLane;
  name: string;
  description: string;
  effect: string;
  cost: bigint;
  level: number;
}

function upgradeLevel(state: UpgradeState, lane: UpgradeLane): number {
  if (lane === 'power') return Number(state.powerUpgradeCount);
  if (lane === 'cooling') return Number(state.coolingUpgradeCount);
  if (lane === 'charges') return Number(state.chargeUpgradeCount);
  if (lane === 'bay') return Number(state.bayUpgradeCount);
  return Number(state.capacityUpgradeCount);
}

function upgradeCost(lane: UpgradeLane, level: number): bigint {
  if (lane === 'power')
    return BigInt(POWER_UPGRADE_BASE_COST + level * POWER_UPGRADE_COST_STEP);
  if (lane === 'cooling')
    return BigInt(
      COOLING_UPGRADE_BASE_COST + level * COOLING_UPGRADE_COST_STEP
    );
  if (lane === 'charges')
    return BigInt(CHARGE_UPGRADE_BASE_COST + level * CHARGE_UPGRADE_COST_STEP);
  if (lane === 'bay')
    return BigInt(BAY_UPGRADE_BASE_COST + level * BAY_UPGRADE_COST_STEP);
  return BigInt(
    CAPACITY_UPGRADE_BASE_COST + level * CAPACITY_UPGRADE_COST_STEP
  );
}

export function upgradeWindowForState(
  state: Pick<UpgradeState, 'bayUpgradeCount'> | null | undefined
): number {
  const level = Number(state?.bayUpgradeCount ?? 0);
  return Math.max(
    MIN_UPGRADE_WINDOW_SECONDS,
    UPGRADE_WINDOW_SECONDS - level * UPGRADE_WINDOW_REDUCTION_SECONDS
  );
}

export function upgradeOffer(
  state: UpgradeState,
  lane: UpgradeLane
): UpgradeOffer {
  const level = upgradeLevel(state, lane);
  if (lane === 'power') {
    return {
      slot: 0,
      id: lane,
      name: 'Plasma Coils',
      description:
        level + 1 === SURGE_UNLOCK_LEVEL
          ? 'More tap energy. Unlocks Surge Burst.'
          : level >= SURGE_UNLOCK_LEVEL
            ? 'More tap energy. Stronger surges.'
            : 'More energy per tap.',
      effect: `+${POWER_PER_UPGRADE} energy per tap${level + 1 === SURGE_UNLOCK_LEVEL ? ' + unlock Surge Burst' : ''}`,
      cost: upgradeCost(lane, level),
      level,
    };
  }
  if (lane === 'cooling') {
    return {
      slot: 1,
      id: lane,
      name: 'Thermal Vents',
      description:
        level + 1 === COOLANT_UNLOCK_LEVEL
          ? 'Faster cooling. Unlocks Coolant Flush.'
          : level >= COOLANT_UNLOCK_LEVEL
            ? 'Faster cooling. Quicker Coolant recharge.'
            : 'Faster passive cooling.',
      effect: `+${COOLING_PER_UPGRADE} cooling / sec${level + 1 === COOLANT_UNLOCK_LEVEL ? ' + unlock Coolant Flush' : ''}`,
      cost: upgradeCost(lane, level),
      level,
    };
  }
  if (lane === 'charges') {
    return {
      slot: 3,
      id: lane,
      name: 'Tap Batteries',
      description: 'More taps before a recharge.',
      effect: `+${CHARGES_PER_UPGRADE} tap charges`,
      cost: upgradeCost(lane, level),
      level,
    };
  }
  if (lane === 'bay') {
    return {
      slot: 4,
      id: lane,
      name: 'Upgrade Bay',
      description: 'Shorter cooldown between installs.',
      effect: `-${UPGRADE_WINDOW_REDUCTION_SECONDS}s shop cooldown`,
      cost: upgradeCost(lane, level),
      level,
    };
  }
  return {
    slot: 2,
    id: lane,
    name: 'Heat Sinks',
    description: 'More heat before the core overheats.',
    effect: `+${CAPACITY_PER_UPGRADE} heat capacity`,
    cost: upgradeCost(lane, level),
    level,
  };
}

export function tapLimitForState(
  state: Pick<UpgradeState, 'chargeUpgradeCount'>
): number {
  return TAP_LIMIT + Number(state.chargeUpgradeCount) * CHARGES_PER_UPGRADE;
}

export function hasCoolantFlush(
  state: Pick<UpgradeState, 'coolingUpgradeCount'>
): boolean {
  return state.coolingUpgradeCount >= COOLANT_UNLOCK_LEVEL;
}

export function hasSurgeBurst(
  state: Pick<UpgradeState, 'powerUpgradeCount'>
): boolean {
  return state.powerUpgradeCount >= SURGE_UNLOCK_LEVEL;
}

export function overheatRecoverAt(state: { heatCapacity: number }): number {
  return Math.floor(state.heatCapacity * 0.45);
}

export function roomTuning(
  state: Pick<UpgradeState, 'coolingUpgradeCount' | 'capacityUpgradeCount'>,
  activeCrew: number
): { heatCapacity: number; coolingPerSecond: number; tapHeatGain: number } {
  const crew = Math.max(1, Math.min(MAX_CREW_SCALING, Math.trunc(activeCrew)));
  const clampU32 = (value: number) =>
    Math.max(0, Math.min(0xffff_ffff, Math.trunc(value)));
  return {
    heatCapacity: clampU32(
      BASE_HEAT_CAPACITY +
        Number(state.capacityUpgradeCount) * CAPACITY_PER_UPGRADE +
        Math.max(0, crew - 1) * HEAT_CAPACITY_PER_EXTRA_CREW
    ),
    coolingPerSecond: clampU32(
      BASE_HEAT_COOL_PER_SECOND +
        Number(state.coolingUpgradeCount) * COOLING_PER_UPGRADE +
        Math.max(0, crew - 1) * COOLING_PER_EXTRA_CREW
    ),
    tapHeatGain: BASE_TAP_HEAT_GAIN,
  };
}

export function playerName(identity: { toHexString(): string }): string {
  return `Crewmate ${identity.toHexString().slice(0, 6).toUpperCase()}`;
}

export function playerColor(identity: { toHexString(): string }): string {
  const hex = identity.toHexString().slice(-8);
  const index = Number.parseInt(hex, 16) % PLAYER_COLORS.length;
  return PLAYER_COLORS[index]!;
}
