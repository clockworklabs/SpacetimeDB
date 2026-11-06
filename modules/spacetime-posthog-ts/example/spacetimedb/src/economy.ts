import type {
  EconRow,
  ProductRow,
  ScenarioRow,
  VariantRow,
  WriteCtx,
} from './schema';

export const SUPPLY_PRICE = {
  compute: 50,
  context: 20,
  memory: 40,
} as const;
export type SupplyKind = keyof typeof SUPPLY_PRICE;

export const START_CASH = 20_000n;
export const START_INVENTORY = {
  compute: 120,
  context: 200,
  memory: 60,
};
export const START_REPUTATION = 50;
export const MAX_MACHINE_LEVEL = 5;
export const PATIENCE_TICKS = 3;
export const RUSH_CYCLE_TICKS = 45;
export const RUSH_LENGTH_TICKS = 12;
export const RUSH_MULTIPLIER = 2.3;
export const REPUTATION_ON_SALE = 1;
export const REPUTATION_ON_STOCKOUT = 2;
export const REPUTATION_ON_RENEGE = 3;

type ServiceEconomy = Pick<EconRow, 'workers'>;
type QueueEconomy = Pick<EconRow, 'seats'>;
type UpgradeEconomy = Pick<
  EconRow,
  'workers' | 'machineLevel' | 'storageLevel' | 'seats'
>;
type SupplyProduct = Pick<ProductRow, 'category'>;
type SupplyVariant = Pick<VariantRow, 'contextTokens' | 'reasoning'>;

const COUNTER_BASE = 6;
const UPGRADE_PRICE = {
  worker: 6_000,
  machine: 8_000,
  counter: 5_000,
  storage: 7_000,
};
const STORAGE_BASE = { compute: 150, context: 250, memory: 100 };

export function storageCapacity(kind: SupplyKind, level: number): number {
  return Math.round(STORAGE_BASE[kind] * (1 + 0.5 * level));
}

export function clampReputation(value: number): number {
  return Math.max(0, Math.min(100, value));
}

export function serviceCapacity(economy: ServiceEconomy): number {
  return Math.max(1, economy.workers);
}

export function maximumQueueLength(economy: QueueEconomy): number {
  return COUNTER_BASE + economy.seats * 2;
}

export function arrivalDemand(baseTraffic: number, reputation: number): number {
  return Math.max(1, Math.round(baseTraffic * (0.5 + reputation / 100)));
}

export function upgradeCost(kind: string, economy: UpgradeEconomy): bigint {
  if (kind === 'worker') return BigInt(UPGRADE_PRICE.worker * economy.workers);
  if (kind === 'machine')
    return BigInt(UPGRADE_PRICE.machine * (economy.machineLevel + 1));
  if (kind === 'storage')
    return BigInt(UPGRADE_PRICE.storage * (economy.storageLevel + 1));
  return BigInt(UPGRADE_PRICE.counter * (economy.seats + 1));
}

export function ensureEconomy(ctx: WriteCtx, owner: string): EconRow {
  const existing = ctx.db.econ.owner.find(owner);
  if (existing) return existing;
  const row = {
    owner,
    cashCents: START_CASH,
    computeUnits: START_INVENTORY.compute,
    contextUnits: START_INVENTORY.context,
    memoryUnits: START_INVENTORY.memory,
    suppliesSpentCents: 0n,
    stockouts: 0,
    reputation: START_REPUTATION,
    workers: 1,
    machineLevel: 0,
    seats: 0,
    storageLevel: 0,
    reneged: 0,
    updatedAt: ctx.timestamp,
  };
  ctx.db.econ.insert(row);
  return row;
}

export function supplyCost(
  product: SupplyProduct,
  variant: SupplyVariant,
  machineLevel = 0
): { compute: number; context: number; memory: number } {
  const efficiency = Math.max(0.6, 1 - 0.08 * machineLevel);
  return {
    context: Math.max(
      1,
      Math.round(Math.ceil(variant.contextTokens / 20_000) * efficiency)
    ),
    compute: Math.max(
      1,
      Math.round((1 + Math.ceil(variant.reasoning / 3)) * efficiency)
    ),
    memory: Math.max(
      1,
      Math.round((product.category === 'memory' ? 6 : 1) * efficiency)
    ),
  };
}

function hashSeed(input: string): number {
  let hash = 2_166_136_261;
  for (let index = 0; index < input.length; index++) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return hash >>> 0;
}

export function seededRandom(seed: string): () => number {
  let value = hashSeed(seed) || 1;
  return () => {
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    return ((value >>> 0) % 10_000) / 10_000;
  };
}

export function chooseProfile(
  random: () => number,
  scenario: ScenarioRow
): string {
  const weighted = [
    ['cheap', Math.max(10, scenario.priceSensitivity)],
    ['rushed', Math.max(10, scenario.rushBias)],
    ['research', Math.max(10, scenario.researchBias)],
    ['visual', Math.max(10, scenario.visualBias)],
    ['memory', Math.max(10, scenario.memoryBias)],
    ['premium', Math.max(10, scenario.premiumBias)],
  ] as const;
  const total = weighted.reduce((sum, row) => sum + row[1], 0);
  let selection = random() * total;
  for (const [profile, weight] of weighted) {
    selection -= weight;
    if (selection <= 0) return profile;
  }
  return 'cheap';
}

export function variantScore(
  product: ProductRow,
  variant: VariantRow,
  scenario: ScenarioRow,
  profile: string
): number {
  let score = product.baseAppeal;
  score += variant.featured ? 24 : 0;
  score += Math.floor(variant.discountBps / 250);

  if (profile === 'cheap')
    score += Math.max(0, 55 - Math.floor(variant.priceCents / 120));
  if (profile === 'rushed') score += variant.latency * 12;
  if (profile === 'research')
    score += Math.floor(variant.contextTokens / 6_000) + variant.reasoning * 7;
  if (profile === 'visual' && product.category === 'multimodal') score += 44;
  if (profile === 'memory' && product.category === 'memory') score += 44;
  if (profile === 'premium')
    score += variant.reasoning * 8 + variant.latency * 5;

  const pricePenalty = Math.floor(
    (variant.priceCents * scenario.priceSensitivity) / 16_000
  );
  return Math.max(1, score - pricePenalty);
}

export function purchaseProbability(
  selected: { productRow: ProductRow; variantRow: VariantRow; score: number },
  scenario: ScenarioRow,
  profile: string
): number {
  let probability = 18 + Math.floor(selected.score / 4);
  if (selected.variantRow.featured) probability += 8;
  if (selected.variantRow.discountBps > 0)
    probability += Math.floor(selected.variantRow.discountBps / 200);
  if (profile === 'cheap')
    probability -= Math.floor(selected.variantRow.priceCents / 250);
  if (profile === 'premium') probability += 10;
  probability -= Math.floor(scenario.volatility / 5);
  return Math.max(5, Math.min(92, probability));
}

export function pricePaid(variant: VariantRow): number {
  return Math.max(
    0,
    Math.floor((variant.priceCents * (10_000 - variant.discountBps)) / 10_000)
  );
}

export function nonSaleReason(
  product: ProductRow,
  variant: VariantRow,
  scenario: ScenarioRow,
  profile: string,
  inStock: boolean,
  shortage: string
): string {
  if (!inStock) return `short_${shortage}`;

  const price =
    Math.floor((variant.priceCents * scenario.priceSensitivity) / 16_000) +
    (profile === 'cheap' ? Math.floor(variant.priceCents / 250) : 0);
  const speedWeight =
    profile === 'rushed' ? 8 : scenario.rushBias >= 55 ? 4 : 0;
  const speed = (10 - variant.latency) * speedWeight;

  let preference = '';
  if (profile === 'visual' && product.category !== 'multimodal')
    preference = 'want_vision';
  else if (profile === 'memory' && product.category !== 'memory')
    preference = 'want_memory';
  else if (profile === 'research' && variant.reasoning < 6)
    preference = 'want_smart';
  else if (profile === 'premium' && variant.reasoning < 6)
    preference = 'want_premium';

  const preferenceWeight = preference ? 30 : 0;
  const dominant = Math.max(price, speed, preferenceWeight);
  if (dominant < 8) return 'meh';
  if (dominant === preferenceWeight) return preference;
  if (dominant === speed) return 'slow';
  return 'price';
}
