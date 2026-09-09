import { t } from 'spacetimedb/server';
import * as posthog from '@spacetimedb/posthog/submodule';

import {
  MAX_TICKS_PER_CALL,
  MAX_ACTIVITY_ROWS,
  MAX_SESSION_ROWS,
  MAX_PURCHASE_ROWS,
  spacetimedb,
  type WriteCtx,
  type VariantRow,
  type ProductRow,
  type ScenarioRow,
  type MetricRow,
} from './schema';
import {
  MAX_MACHINE_LEVEL,
  PATIENCE_TICKS,
  REPUTATION_ON_RENEGE,
  REPUTATION_ON_SALE,
  REPUTATION_ON_STOCKOUT,
  RUSH_CYCLE_TICKS,
  RUSH_LENGTH_TICKS,
  RUSH_MULTIPLIER,
  START_CASH,
  START_INVENTORY,
  START_REPUTATION,
  SUPPLY_PRICE,
  arrivalDemand,
  chooseProfile,
  clampReputation,
  ensureEconomy,
  maximumQueueLength,
  nonSaleReason,
  pricePaid,
  purchaseProbability,
  seededRandom,
  serviceCapacity,
  storageCapacity,
  supplyCost,
  upgradeCost,
  variantScore,
  type SupplyKind,
} from './economy';
import { clampU32, fail, requireId } from './validation';
export { default } from './schema';
export * from './catalog';

function keyFor(owner: string, id: string): string {
  return `${owner}|${id}`;
}

function findProduct(ctx: WriteCtx, owner: string, productId: string) {
  return ctx.db.product.key.find(keyFor(owner, productId));
}

function findVariant(ctx: WriteCtx, owner: string, variantId: string) {
  return ctx.db.variant.key.find(keyFor(owner, variantId));
}

function ensureConfig(ctx: WriteCtx, owner: string) {
  const existing = ctx.db.simConfig.owner.find(owner);
  if (existing) return existing;
  const firstScenario = [...ctx.db.scenario.iter()][0];
  const scenarioId = firstScenario?.scenarioId ?? 'steady';
  const row = {
    owner,
    scenarioId,
    tick: 0n,
    experimentKey: 'context-cafe-offer',
    experimentVariant: undefined,
    updatedAt: ctx.timestamp,
  };
  ctx.db.simConfig.insert(row);
  return row;
}

function ensureMetrics(ctx: WriteCtx, owner: string): MetricRow {
  const existing = ctx.db.metrics.owner.find(owner);
  if (existing) return existing;
  const row = {
    owner,
    tick: 0n,
    views: 0n,
    carts: 0n,
    checkouts: 0n,
    purchases: 0n,
    abandons: 0n,
    revenueCents: 0n,
    updatedAt: ctx.timestamp,
  };
  ctx.db.metrics.insert(row);
  return row;
}

function addActivity(
  ctx: WriteCtx,
  owner: string,
  tick: bigint,
  kind: string,
  message: string,
  detail: {
    profile?: string;
    productId?: string;
    variantId?: string;
    amountCents?: number;
  } = {}
): void {
  ctx.db.activity.insert({
    activityId: 0n,
    owner,
    tick,
    kind,
    message,
    profile: detail.profile,
    productId: detail.productId,
    variantId: detail.variantId,
    amountCents: detail.amountCents,
    createdAt: ctx.timestamp,
  });
}

function enqueueCafeEvent(
  ctx: WriteCtx,
  distinctId: string,
  event: string,
  props: Record<string, unknown>
): void {
  posthog.enqueueEvent(ctx.as.posthog, {
    distinctId,
    event,
    propertiesJson: JSON.stringify({
      source: 'context_cafe',
      ...props,
    }),
    idempotencyKey: undefined,
  });
}

function selectVariant(
  ctx: WriteCtx,
  owner: string,
  rand: () => number,
  scenarioRow: ScenarioRow,
  profile: string
) {
  const candidates = [...ctx.db.variant.byOwner.filter(owner)]
    .filter((variantRow: VariantRow) => variantRow.active)
    .map((variantRow: VariantRow) => {
      const productRow = findProduct(ctx, owner, variantRow.productId);
      if (!productRow || !productRow.active) return undefined;
      return {
        productRow,
        variantRow,
        score: variantScore(productRow, variantRow, scenarioRow, profile),
      };
    })
    .filter(Boolean) as Array<{
    productRow: ProductRow;
    variantRow: VariantRow;
    score: number;
  }>;

  if (candidates.length === 0) return undefined;
  const total = candidates.reduce((sum, row) => sum + row.score, 0);
  let pick = rand() * total;
  for (const candidate of candidates) {
    pick -= candidate.score;
    if (pick <= 0) return candidate;
  }
  return candidates[0];
}

import { newestFirst } from './recent';

function trimRecent(ctx: WriteCtx, owner: string): void {
  for (const row of newestFirst([...ctx.db.activity.owner.filter(owner)]).slice(
    MAX_ACTIVITY_ROWS
  ))
    ctx.db.activity.delete(row);
  for (const row of newestFirst([
    ...ctx.db.botSession.owner.filter(owner),
  ]).slice(MAX_SESSION_ROWS))
    ctx.db.botSession.delete(row);
  for (const row of newestFirst([...ctx.db.purchase.owner.filter(owner)]).slice(
    MAX_PURCHASE_ROWS
  ))
    ctx.db.purchase.delete(row);
}

// Seed this caller's per-session catalog + config/metrics. Idempotent.
export const init_session = spacetimedb.reducer({}, ctx => {
  const owner = ctx.sender.toHexString();
  const alreadySeeded = [...ctx.db.product.byOwner.filter(owner)].length > 0;
  if (!alreadySeeded) {
    for (const tpl of [...ctx.db.productTemplate.iter()]) {
      ctx.db.product.insert({
        key: keyFor(owner, tpl.productId),
        owner,
        productId: tpl.productId,
        name: tpl.name,
        category: tpl.category,
        description: tpl.description,
        baseAppeal: tpl.baseAppeal,
        active: tpl.active,
        createdAt: ctx.timestamp,
        updatedAt: ctx.timestamp,
      });
    }
    for (const tpl of [...ctx.db.variantTemplate.iter()]) {
      ctx.db.variant.insert({
        key: keyFor(owner, tpl.variantId),
        owner,
        variantId: tpl.variantId,
        productId: tpl.productId,
        name: tpl.name,
        flavor: tpl.flavor,
        contextTokens: tpl.contextTokens,
        reasoning: tpl.reasoning,
        latency: tpl.latency,
        priceCents: tpl.priceCents,
        baselinePriceCents: tpl.priceCents,
        discountBps: tpl.discountBps,
        active: tpl.active,
        featured: tpl.featured,
        updatedAt: ctx.timestamp,
      });
    }
  }
  ensureConfig(ctx, owner);
  ensureMetrics(ctx, owner);
  ensureEconomy(ctx, owner);
});

export const reset_simulation = spacetimedb.reducer(
  { scenarioId: t.string() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const scenarioId = requireId(args.scenarioId, 'scenario_id');
    if (!ctx.db.scenario.scenarioId.find(scenarioId)) fail('unknown_scenario');
    for (const row of [...ctx.db.botSession.owner.filter(owner)])
      ctx.db.botSession.delete(row);
    for (const row of [...ctx.db.purchase.owner.filter(owner)])
      ctx.db.purchase.delete(row);
    for (const row of [...ctx.db.activity.owner.filter(owner)])
      ctx.db.activity.delete(row);
    for (const row of [...ctx.db.waitingBot.owner.filter(owner)])
      ctx.db.waitingBot.delete(row);
    const config = ensureConfig(ctx, owner);
    ctx.db.simConfig.owner.update({
      ...config,
      scenarioId,
      tick: 0n,
      updatedAt: ctx.timestamp,
    });
    const metric = ensureMetrics(ctx, owner);
    ctx.db.metrics.owner.update({
      ...metric,
      tick: 0n,
      views: 0n,
      carts: 0n,
      checkouts: 0n,
      purchases: 0n,
      abandons: 0n,
      revenueCents: 0n,
      updatedAt: ctx.timestamp,
    });
    const money = ensureEconomy(ctx, owner);
    ctx.db.econ.owner.update({
      ...money,
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
    });
    addActivity(ctx, owner, 0n, 'reset', 'Simulation reset.');
  }
);

export const select_scenario = spacetimedb.reducer(
  { scenarioId: t.string() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const scenarioId = requireId(args.scenarioId, 'scenario_id');
    const scenarioRow = ctx.db.scenario.scenarioId.find(scenarioId);
    if (!scenarioRow) fail('unknown_scenario');
    const config = ensureConfig(ctx, owner);
    ctx.db.simConfig.owner.update({
      ...config,
      scenarioId,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      config.tick,
      'scenario_selected',
      `Scenario set to ${scenarioRow.name}.`
    );
  }
);

export const set_product_active = spacetimedb.reducer(
  { productId: t.string(), active: t.bool() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const productId = requireId(args.productId, 'product_id');
    const row = findProduct(ctx, owner, productId);
    if (!row) fail('unknown_product');
    ctx.db.product.key.update({
      ...row,
      active: args.active,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      args.active ? 'product_enabled' : 'product_disabled',
      `${row.name} ${args.active ? 'enabled' : 'disabled'}.`,
      { productId }
    );
    enqueueCafeEvent(
      ctx,
      owner,
      args.active ? 'product_enabled' : 'product_disabled',
      { product_id: productId, product_name: row.name }
    );
  }
);

export const set_variant_active = spacetimedb.reducer(
  { variantId: t.string(), active: t.bool() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const variantId = requireId(args.variantId, 'variant_id');
    const row = findVariant(ctx, owner, variantId);
    if (!row) fail('unknown_variant');
    ctx.db.variant.key.update({
      ...row,
      active: args.active,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      args.active ? 'variant_enabled' : 'variant_disabled',
      `${row.name} ${args.active ? 'enabled' : 'disabled'}.`,
      { productId: row.productId, variantId }
    );
  }
);

export const set_variant_price = spacetimedb.reducer(
  { variantId: t.string(), priceCents: t.u32() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const row = findVariant(
      ctx,
      owner,
      requireId(args.variantId, 'variant_id')
    );
    if (!row) fail('unknown_variant');
    const priceCents = clampU32(args.priceCents, 'price_cents', 0, 250_000);
    ctx.db.variant.key.update({ ...row, priceCents, updatedAt: ctx.timestamp });
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      'price_changed',
      `${row.name} price changed to $${(priceCents / 100).toFixed(2)}.`,
      {
        productId: row.productId,
        variantId: row.variantId,
        amountCents: priceCents,
      }
    );
    enqueueCafeEvent(ctx, owner, 'price_changed', {
      variant_id: row.variantId,
      product_id: row.productId,
      price_cents: priceCents,
      old_price_cents: row.priceCents,
    });
  }
);

export const set_variant_discount = spacetimedb.reducer(
  { variantId: t.string(), discountBps: t.u32() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const row = findVariant(
      ctx,
      owner,
      requireId(args.variantId, 'variant_id')
    );
    if (!row) fail('unknown_variant');
    const discountBps = clampU32(args.discountBps, 'discount_bps', 0, 9000);
    ctx.db.variant.key.update({
      ...row,
      discountBps,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      'discount_changed',
      `${row.name} discount set to ${discountBps / 100}%.`,
      { productId: row.productId, variantId: row.variantId }
    );
    enqueueCafeEvent(ctx, owner, 'discount_changed', {
      variant_id: row.variantId,
      product_id: row.productId,
      discount_bps: discountBps,
    });
  }
);

export const set_featured_variant = spacetimedb.reducer(
  { variantId: t.string() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const variantId = requireId(args.variantId, 'variant_id');
    const selected = findVariant(ctx, owner, variantId);
    if (!selected) fail('unknown_variant');
    for (const row of [...ctx.db.variant.byOwner.filter(owner)]) {
      ctx.db.variant.key.update({
        ...row,
        featured: row.variantId === variantId,
        updatedAt: ctx.timestamp,
      });
    }
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      'featured_variant_changed',
      `${selected.name} is now the featured recipe.`,
      { productId: selected.productId, variantId }
    );
    enqueueCafeEvent(ctx, owner, 'featured_variant_changed', {
      variant_id: variantId,
      product_id: selected.productId,
    });
  }
);

export const set_experiment_variant = spacetimedb.reducer(
  { key: t.string(), variant: t.option(t.string()) },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const config = ensureConfig(ctx, owner);
    ctx.db.simConfig.owner.update({
      ...config,
      experimentKey: args.key.trim() || config.experimentKey,
      experimentVariant: args.variant,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      config.tick,
      'experiment_variant_checked',
      `Experiment ${args.key || config.experimentKey}: ${args.variant ?? 'control'}.`
    );
  }
);

export const buy_supply = spacetimedb.reducer(
  { kind: t.string(), units: t.u32() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const kind = requireId(args.kind, 'supply_kind');
    if (kind !== 'compute' && kind !== 'context' && kind !== 'memory')
      fail('invalid_supply_kind');
    const money = ensureEconomy(ctx, owner);
    const field =
      kind === 'compute'
        ? 'computeUnits'
        : kind === 'context'
          ? 'contextUnits'
          : 'memoryUnits';
    // Cap the purchase at the storeroom's available capacity.
    const headroom =
      storageCapacity(kind as SupplyKind, money.storageLevel) - money[field];
    if (headroom <= 0) fail('storage_full');
    const units = Math.min(clampU32(args.units, 'units', 1, 1000), headroom);
    const cost = BigInt(units * SUPPLY_PRICE[kind as SupplyKind]);
    if (money.cashCents < cost) fail('insufficient_cash');
    ctx.db.econ.owner.update({
      ...money,
      cashCents: money.cashCents - cost,
      [field]: money[field] + units,
      suppliesSpentCents: money.suppliesSpentCents + cost,
      updatedAt: ctx.timestamp,
    });
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      'supply_purchased',
      `Bought ${units} ${kind} ($${(Number(cost) / 100).toFixed(2)}).`,
      { amountCents: Number(cost) }
    );
    enqueueCafeEvent(ctx, owner, 'supply_purchased', {
      kind,
      units,
      cost_cents: Number(cost),
    });
  }
);

export const buy_upgrade = spacetimedb.reducer(
  { kind: t.string() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const kind = requireId(args.kind, 'upgrade_kind');
    if (
      kind !== 'worker' &&
      kind !== 'machine' &&
      kind !== 'counter' &&
      kind !== 'storage'
    )
      fail('invalid_upgrade_kind');
    const money = ensureEconomy(ctx, owner);
    if (kind === 'machine' && money.machineLevel >= MAX_MACHINE_LEVEL)
      fail('machine_maxed');
    const cost = upgradeCost(kind, money);
    if (money.cashCents < cost) fail('insufficient_cash');
    const next = {
      ...money,
      cashCents: money.cashCents - cost,
      workers: money.workers + (kind === 'worker' ? 1 : 0),
      machineLevel: money.machineLevel + (kind === 'machine' ? 1 : 0),
      seats: money.seats + (kind === 'counter' ? 1 : 0),
      storageLevel: money.storageLevel + (kind === 'storage' ? 1 : 0),
      updatedAt: ctx.timestamp,
    };
    ctx.db.econ.owner.update(next);
    const label =
      kind === 'worker'
        ? `Hired a worker (now ${next.workers} serving/tick)`
        : kind === 'machine'
          ? `Upgraded machines (level ${next.machineLevel})`
          : kind === 'storage'
            ? `Expanded storeroom (holds ${storageCapacity('context', next.storageLevel)} context)`
            : `Added counter space (holds ${maximumQueueLength(next)})`;
    addActivity(
      ctx,
      owner,
      ensureConfig(ctx, owner).tick,
      'upgrade_purchased',
      `${label}: $${(Number(cost) / 100).toFixed(2)}.`,
      { amountCents: Number(cost) }
    );
    enqueueCafeEvent(ctx, owner, 'upgrade_purchased', {
      kind,
      cost_cents: Number(cost),
      workers: next.workers,
      machine_level: next.machineLevel,
      seats: next.seats,
      storage_level: next.storageLevel,
    });
  }
);

export const simulate_tick = spacetimedb.reducer(
  { ticks: t.u32(), seed: t.string() },
  (ctx, args) => {
    const owner = ctx.sender.toHexString();
    const tickCount = clampU32(args.ticks, 'ticks', 1, MAX_TICKS_PER_CALL);
    const config = ensureConfig(ctx, owner);
    const scenarioRow = ctx.db.scenario.scenarioId.find(config.scenarioId);
    if (!scenarioRow) fail('unknown_scenario');
    let metric = ensureMetrics(ctx, owner);
    const money = { ...ensureEconomy(ctx, owner) };
    let finalTick = config.tick;

    const WANTS: Record<string, string> = {
      cheap: 'a deal',
      rushed: 'speed',
      research: 'long context',
      visual: 'vision',
      memory: 'memory',
      premium: 'top quality',
    };

    for (let i = 0; i < tickCount; i++) {
      finalTick += 1n;
      const rand = seededRandom(
        `${args.seed}:${finalTick.toString()}:${config.scenarioId}`
      );
      const baseTraffic = scenarioRow.trafficPerTick;
      let tickViews = 0n;
      let tickCarts = 0n;
      let tickCheckouts = 0n;
      let tickPurchases = 0n;
      let tickAbandons = 0n;
      let tickRevenue = 0n;

      // Serve up to capacity bots from the front; each leaves the queue either way.
      const queue = [...ctx.db.waitingBot.owner.filter(owner)].sort((a, b) =>
        a.queueId < b.queueId ? -1 : a.queueId > b.queueId ? 1 : 0
      );
      const capacity = serviceCapacity(money);
      let served = 0;
      for (const front of queue.slice(0, capacity)) {
        served++;
        ctx.db.waitingBot.delete(front);
        const variantRow = findVariant(ctx, owner, front.variantId);
        const productRow = variantRow
          ? findProduct(ctx, owner, variantRow.productId)
          : undefined;

        if (
          !variantRow ||
          !variantRow.active ||
          !productRow ||
          !productRow.active
        ) {
          // The selected item is unavailable when the customer reaches the counter.
          tickAbandons++;
          ctx.db.botSession.insert({
            sessionId: 0n,
            owner,
            botId: front.botId,
            tick: finalTick,
            profile: front.profile,
            scenarioId: scenarioRow.scenarioId,
            productId: front.productId,
            variantId: front.variantId,
            stage: 'abandoned',
            revenueCents: 0,
            reason: 'unavailable',
            createdAt: ctx.timestamp,
          });
          addActivity(
            ctx,
            owner,
            finalTick,
            'checkout_abandoned',
            `${front.profile} bot left because its selection was unavailable.`,
            {
              profile: front.profile,
              productId: front.productId,
              variantId: front.variantId,
            }
          );
          enqueueCafeEvent(ctx, front.botId, 'checkout_abandoned', {
            tick: finalTick.toString(),
            scenario_id: scenarioRow.scenarioId,
            bot_profile: front.profile,
            product_id: front.productId,
            variant_id: front.variantId,
            price_cents: 0,
            reason: 'unavailable',
          });
        } else {
          const selected = {
            productRow,
            variantRow,
            score: variantScore(
              productRow,
              variantRow,
              scenarioRow,
              front.profile
            ),
          };
          const paid = pricePaid(variantRow);
          const cartChance = Math.min(94, 35 + Math.floor(selected.score / 3));
          const didCart = rand() * 100 < cartChance;
          const didCheckout = didCart && rand() * 100 < 82;
          const wouldBuy =
            didCheckout &&
            rand() * 100 <
              purchaseProbability(selected, scenarioRow, front.profile);
          // Complete a sale only when the required supplies are available.
          const cost = supplyCost(productRow, variantRow, money.machineLevel);
          const inStock =
            money.computeUnits >= cost.compute &&
            money.contextUnits >= cost.context &&
            money.memoryUnits >= cost.memory;
          const didPurchase = wouldBuy && inStock;
          const stockedOut = wouldBuy && !inStock;
          let stage = 'viewed';
          if (didCart) {
            tickCarts++;
            stage = 'cart';
            enqueueCafeEvent(ctx, front.botId, 'product_added_to_cart', {
              tick: finalTick.toString(),
              scenario_id: scenarioRow.scenarioId,
              bot_profile: front.profile,
              product_id: productRow.productId,
              variant_id: variantRow.variantId,
              price_cents: variantRow.priceCents,
              discounted_price_cents: paid,
            });
          }
          if (didCheckout) {
            tickCheckouts++;
            stage = 'checkout';
            enqueueCafeEvent(ctx, front.botId, 'checkout_started', {
              tick: finalTick.toString(),
              scenario_id: scenarioRow.scenarioId,
              bot_profile: front.profile,
              product_id: productRow.productId,
              variant_id: variantRow.variantId,
              price_cents: paid,
            });
          }
          if (didPurchase) {
            tickPurchases++;
            tickRevenue += BigInt(paid);
            stage = 'purchased';
            // Consume supplies + bank the cash + a happy customer lifts reputation.
            money.computeUnits -= cost.compute;
            money.contextUnits -= cost.context;
            money.memoryUnits -= cost.memory;
            money.cashCents += BigInt(paid);
            money.reputation = clampReputation(
              money.reputation + REPUTATION_ON_SALE
            );
          } else if (didCheckout || didCart) {
            tickAbandons++;
            stage = 'abandoned';
          }
          if (stockedOut) {
            money.stockouts += 1;
            money.reputation = clampReputation(
              money.reputation - REPUTATION_ON_STOCKOUT
            );
          }

          // Show the purchase decision at the counter.
          const short =
            money.contextUnits < cost.context
              ? 'context'
              : money.computeUnits < cost.compute
                ? 'compute'
                : 'memory';
          const reason = didPurchase
            ? ''
            : nonSaleReason(
                productRow,
                variantRow,
                scenarioRow,
                front.profile,
                inStock,
                short
              );

          ctx.db.botSession.insert({
            sessionId: 0n,
            owner,
            botId: front.botId,
            tick: finalTick,
            profile: front.profile,
            scenarioId: scenarioRow.scenarioId,
            productId: productRow.productId,
            variantId: variantRow.variantId,
            stage,
            revenueCents: didPurchase ? paid : 0,
            reason,
            createdAt: ctx.timestamp,
          });

          if (didPurchase) {
            ctx.db.purchase.insert({
              purchaseId: 0n,
              owner,
              sessionId: 0n,
              tick: finalTick,
              botId: front.botId,
              profile: front.profile,
              productId: productRow.productId,
              variantId: variantRow.variantId,
              pricePaidCents: paid,
              createdAt: ctx.timestamp,
            });
            addActivity(
              ctx,
              owner,
              finalTick,
              'purchase_completed',
              `${front.profile} bot bought ${variantRow.name}.`,
              {
                profile: front.profile,
                productId: productRow.productId,
                variantId: variantRow.variantId,
                amountCents: paid,
              }
            );
            enqueueCafeEvent(ctx, front.botId, 'purchase_completed', {
              tick: finalTick.toString(),
              scenario_id: scenarioRow.scenarioId,
              bot_profile: front.profile,
              product_id: productRow.productId,
              product_name: productRow.name,
              variant_id: variantRow.variantId,
              variant_name: variantRow.name,
              price_cents: paid,
              discount_bps: variantRow.discountBps,
            });
          } else if (stockedOut) {
            addActivity(
              ctx,
              owner,
              finalTick,
              'stockout',
              `${front.profile} bot left because ${short} was out of stock.`,
              {
                profile: front.profile,
                productId: productRow.productId,
                variantId: variantRow.variantId,
              }
            );
            enqueueCafeEvent(ctx, front.botId, 'stockout', {
              tick: finalTick.toString(),
              scenario_id: scenarioRow.scenarioId,
              bot_profile: front.profile,
              product_id: productRow.productId,
              variant_id: variantRow.variantId,
              short_supply: short,
              price_cents: paid,
            });
          } else if (stage === 'abandoned') {
            addActivity(
              ctx,
              owner,
              finalTick,
              'checkout_abandoned',
              `${front.profile} bot bailed on ${variantRow.name}.`,
              {
                profile: front.profile,
                productId: productRow.productId,
                variantId: variantRow.variantId,
              }
            );
            enqueueCafeEvent(ctx, front.botId, 'checkout_abandoned', {
              tick: finalTick.toString(),
              scenario_id: scenarioRow.scenarioId,
              bot_profile: front.profile,
              product_id: productRow.productId,
              variant_id: variantRow.variantId,
              price_cents: paid,
              reason,
            });
          }
        }
      }

      // Customers who exceed their patience limit leave and reduce reputation.
      for (const waiting of [...ctx.db.waitingBot.owner.filter(owner)]) {
        if (finalTick - waiting.arrivedTick <= BigInt(PATIENCE_TICKS)) continue;
        ctx.db.waitingBot.delete(waiting);
        money.reneged += 1;
        money.reputation = clampReputation(
          money.reputation - REPUTATION_ON_RENEGE
        );
        tickAbandons++;
        ctx.db.botSession.insert({
          sessionId: 0n,
          owner,
          botId: waiting.botId,
          tick: finalTick,
          profile: waiting.profile,
          scenarioId: scenarioRow.scenarioId,
          productId: waiting.productId,
          variantId: waiting.variantId,
          stage: 'abandoned',
          revenueCents: 0,
          reason: 'waited',
          createdAt: ctx.timestamp,
        });
        addActivity(
          ctx,
          owner,
          finalTick,
          'reneged',
          `${waiting.profile} bot gave up waiting.`,
          {
            profile: waiting.profile,
            productId: waiting.productId,
            variantId: waiting.variantId,
          }
        );
        enqueueCafeEvent(ctx, waiting.botId, 'reneged', {
          tick: finalTick.toString(),
          scenario_id: scenarioRow.scenarioId,
          bot_profile: waiting.profile,
          waited_ticks: Number(finalTick - waiting.arrivedTick),
          reason: 'waited',
        });
      }

      // New arrivals scale with reputation (and surge during a rush), but the counter only holds so many.
      const rushing =
        finalTick % BigInt(RUSH_CYCLE_TICKS) < BigInt(RUSH_LENGTH_TICKS);
      const effTraffic = rushing ? baseTraffic * RUSH_MULTIPLIER : baseTraffic;
      const lineCap = maximumQueueLength(money);
      const demand = Math.min(
        arrivalDemand(effTraffic, money.reputation),
        lineCap
      );
      let admit = 0;
      while (
        [...ctx.db.waitingBot.owner.filter(owner)].length < demand &&
        admit < lineCap
      ) {
        admit++;
        const profile = chooseProfile(rand, scenarioRow);
        const botId = `bot-${finalTick.toString()}-${admit}`;
        const selected = selectVariant(ctx, owner, rand, scenarioRow, profile);
        if (!selected) {
          addActivity(
            ctx,
            owner,
            finalTick,
            'no_inventory',
            'A bot found no active recipes.'
          );
          break;
        }
        const { productRow, variantRow } = selected;
        tickViews++;
        enqueueCafeEvent(ctx, botId, 'product_viewed', {
          tick: finalTick.toString(),
          scenario_id: scenarioRow.scenarioId,
          bot_profile: profile,
          product_id: productRow.productId,
          product_name: productRow.name,
          variant_id: variantRow.variantId,
          variant_name: variantRow.name,
          price_cents: variantRow.priceCents,
          discount_bps: variantRow.discountBps,
          featured: variantRow.featured,
          context_tokens: variantRow.contextTokens,
          reasoning: variantRow.reasoning,
          latency: variantRow.latency,
        });
        ctx.db.waitingBot.insert({
          queueId: 0n,
          owner,
          botId,
          profile,
          scenarioId: scenarioRow.scenarioId,
          productId: productRow.productId,
          variantId: variantRow.variantId,
          wants: WANTS[profile] ?? 'a good drink',
          thrifty: profile === 'cheap' || scenarioRow.priceSensitivity >= 60,
          arrivedTick: finalTick,
          createdAt: ctx.timestamp,
        });
      }

      metric = {
        ...metric,
        tick: finalTick,
        views: metric.views + tickViews,
        carts: metric.carts + tickCarts,
        checkouts: metric.checkouts + tickCheckouts,
        purchases: metric.purchases + tickPurchases,
        abandons: metric.abandons + tickAbandons,
        revenueCents: metric.revenueCents + tickRevenue,
        updatedAt: ctx.timestamp,
      };
      ctx.db.metrics.owner.update(metric);
      ctx.db.econ.owner.update({ ...money, updatedAt: ctx.timestamp });
      enqueueCafeEvent(ctx, `sim:${owner}`, 'serve_tick_summary', {
        tick: finalTick.toString(),
        scenario_id: scenarioRow.scenarioId,
        served,
        capacity,
        rush: rushing,
        reputation: money.reputation,
        queue_length: [...ctx.db.waitingBot.owner.filter(owner)].length,
        purchases: tickPurchases.toString(),
        abandons: tickAbandons.toString(),
        revenue_cents: tickRevenue.toString(),
      });
    }

    ctx.db.simConfig.owner.update({
      ...config,
      tick: finalTick,
      updatedAt: ctx.timestamp,
    });
    trimRecent(ctx, owner);
  }
);

export * from './views';

export const init = spacetimedb.init(ctx => {
  posthog.installPostHog(ctx.as.posthog);
});
