import {
  schema,
  table,
  t,
  type Infer,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import * as posthog from '@spacetimedb/posthog/submodule';

export const MAX_SYNC_ROWS = 100;
export const MAX_TICKS_PER_CALL = 25;
export const MAX_ACTIVITY_ROWS = 120;
export const MAX_SESSION_ROWS = 250;
export const MAX_PURCHASE_ROWS = 120;

// Template catalog. init_session copies these into per-session rows.
export const productTemplate = table(
  { name: 'product_template', public: false },
  {
    productId: t.string().primaryKey(),
    name: t.string(),
    category: t.string(),
    description: t.string(),
    baseAppeal: t.u32(),
    active: t.bool(),
  }
);

export const variantTemplate = table(
  { name: 'variant_template', public: false },
  {
    variantId: t.string().primaryKey(),
    productId: t.string().index(),
    name: t.string(),
    flavor: t.string(),
    contextTokens: t.u32(),
    reasoning: t.u32(),
    latency: t.u32(),
    priceCents: t.u32(),
    discountBps: t.u32(),
    active: t.bool(),
    featured: t.bool(),
  }
);

// Per-session catalog; key = `${owner}|${id}` so ids can repeat across sessions.
export const product = table(
  {
    name: 'product',
    public: false,
    indexes: [{ accessor: 'byOwner', algorithm: 'btree', columns: ['owner'] }],
  },
  {
    key: t.string().primaryKey(),
    owner: t.string(),
    productId: t.string().index(),
    name: t.string(),
    category: t.string(),
    description: t.string(),
    baseAppeal: t.u32(),
    active: t.bool(),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

export const variant = table(
  {
    name: 'variant',
    public: false,
    indexes: [{ accessor: 'byOwner', algorithm: 'btree', columns: ['owner'] }],
  },
  {
    key: t.string().primaryKey(),
    owner: t.string(),
    variantId: t.string().index(),
    productId: t.string().index(),
    name: t.string(),
    flavor: t.string(),
    contextTokens: t.u32(),
    reasoning: t.u32(),
    latency: t.u32(),
    priceCents: t.u32(),
    baselinePriceCents: t.u32(),
    discountBps: t.u32(),
    active: t.bool(),
    featured: t.bool(),
    updatedAt: t.timestamp(),
  }
);

export const scenario = table(
  { name: 'scenario', public: false },
  {
    scenarioId: t.string().primaryKey(),
    name: t.string().index(),
    description: t.string(),
    trafficPerTick: t.u32(),
    priceSensitivity: t.u32(),
    rushBias: t.u32(),
    researchBias: t.u32(),
    visualBias: t.u32(),
    memoryBias: t.u32(),
    premiumBias: t.u32(),
    volatility: t.u32(),
  }
);

export const simConfig = table(
  { name: 'sim_config', public: false },
  {
    owner: t.string().primaryKey(),
    scenarioId: t.string(),
    tick: t.u64(),
    experimentKey: t.string(),
    experimentVariant: t.option(t.string()),
    updatedAt: t.timestamp(),
  }
);

export const metrics = table(
  { name: 'metrics', public: false },
  {
    owner: t.string().primaryKey(),
    tick: t.u64(),
    views: t.u64(),
    carts: t.u64(),
    checkouts: t.u64(),
    purchases: t.u64(),
    abandons: t.u64(),
    revenueCents: t.u64(),
    updatedAt: t.timestamp(),
  }
);

// Per-session cash, supply inventory, reputation, and capacity upgrades.
export const econ = table(
  { name: 'econ', public: false },
  {
    owner: t.string().primaryKey(),
    cashCents: t.u64(),
    computeUnits: t.u32(),
    contextUnits: t.u32(),
    memoryUnits: t.u32(),
    suppliesSpentCents: t.u64(),
    stockouts: t.u32(),
    reputation: t.u32(),
    workers: t.u32(),
    machineLevel: t.u32(),
    seats: t.u32(),
    storageLevel: t.u32(),
    reneged: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const botSession = table(
  {
    name: 'bot_session',
    public: false,
    indexes: [
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
    ],
  },
  {
    sessionId: t.u64().primaryKey().autoInc(),
    owner: t.string().index(),
    botId: t.string().index(),
    tick: t.u64().index(),
    profile: t.string().index(),
    scenarioId: t.string().index(),
    productId: t.option(t.string()),
    variantId: t.option(t.string()),
    stage: t.string().index(),
    revenueCents: t.u32(),
    reason: t.string(),
    createdAt: t.timestamp(),
  }
);

export const purchase = table(
  {
    name: 'purchase',
    public: false,
    indexes: [
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
    ],
  },
  {
    purchaseId: t.u64().primaryKey().autoInc(),
    owner: t.string().index(),
    sessionId: t.u64().index(),
    tick: t.u64().index(),
    botId: t.string().index(),
    profile: t.string().index(),
    productId: t.string().index(),
    variantId: t.string().index(),
    pricePaidCents: t.u32(),
    createdAt: t.timestamp(),
  }
);

export const activity = table(
  {
    name: 'activity',
    public: false,
    indexes: [
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
    ],
  },
  {
    activityId: t.u64().primaryKey().autoInc(),
    owner: t.string().index(),
    tick: t.u64().index(),
    kind: t.string().index(),
    message: t.string(),
    profile: t.option(t.string()),
    productId: t.option(t.string()),
    variantId: t.option(t.string()),
    amountCents: t.option(t.u32()),
    createdAt: t.timestamp(),
  }
);

// Waiting queue; lowest queueId is served first.
export const waitingBot = table(
  {
    name: 'waiting_bot',
    public: false,
  },
  {
    queueId: t.u64().primaryKey().autoInc(),
    owner: t.string().index(),
    botId: t.string(),
    profile: t.string().index(),
    scenarioId: t.string().index(),
    productId: t.string(),
    variantId: t.string(),
    wants: t.string(),
    thrifty: t.bool(),
    arrivedTick: t.u64(),
    createdAt: t.timestamp(),
  }
);

export const cafeDeliveryLogViewRow = posthog.t.object(
  'ContextCafeDeliveryLogRow',
  {
    deliveryId: posthog.t.string(),
    source: posthog.t.string(),
    distinctId: posthog.t.string(),
    event: posthog.t.string(),
    ok: posthog.t.bool(),
    statusCode: posthog.t.u16(),
    responseBody: posthog.t.string(),
    attemptedAt: posthog.t.timestamp(),
  }
);

export const cafeAnalyticsSummaryRow = posthog.t.object(
  'ContextCafeAnalyticsSummaryRow',
  {
    queued: posthog.t.u64(),
    delivered: posthog.t.u64(),
    failed: posthog.t.u64(),
  }
);

export const spacetimedb = schema({
  posthog,
  productTemplate,
  variantTemplate,
  product,
  variant,
  scenario,
  simConfig,
  metrics,
  econ,
  botSession,
  purchase,
  activity,
  waitingBot,
});

export type ProductInput = {
  productId: string;
  name: string;
  category: string;
  description: string;
  baseAppeal: number;
  active?: boolean;
};

export type VariantInput = {
  variantId: string;
  productId: string;
  name: string;
  flavor: string;
  contextTokens: number;
  reasoning: number;
  latency: number;
  priceCents: number;
  discountBps?: number;
  active?: boolean;
  featured?: boolean;
};

export type ScenarioInput = {
  scenarioId: string;
  name: string;
  description: string;
  trafficPerTick: number;
  priceSensitivity: number;
  rushBias: number;
  researchBias: number;
  visualBias: number;
  memoryBias: number;
  premiumBias: number;
  volatility: number;
};

export type Schema = InferSchema<typeof spacetimedb>;
export type WriteCtx = ReducerCtx<Schema>;
export type VariantRow = Infer<typeof variant.rowType>;
export type ProductRow = Infer<typeof product.rowType>;
export type ScenarioRow = Infer<typeof scenario.rowType>;
export type MetricRow = Infer<typeof metrics.rowType>;
export type EconRow = Infer<typeof econ.rowType>;

export default spacetimedb;
