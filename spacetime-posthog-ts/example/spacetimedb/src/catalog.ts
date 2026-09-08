import { t } from 'spacetimedb/server';

import {
  MAX_SYNC_ROWS,
  spacetimedb,
  type ProductInput,
  type ScenarioInput,
  type VariantInput,
} from './schema';
import { clampU32, fail, requireId } from './validation';

function parseArray<T>(json: string, field: string): T[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    fail(`invalid_${field}_json`);
  }
  if (!Array.isArray(parsed)) fail(`invalid_${field}_json`);
  if (parsed.length > MAX_SYNC_ROWS) fail(`${field}_too_large`);
  return parsed as T[];
}

export const sync_catalog = spacetimedb.reducer(
  { productsJson: t.string(), scenariosJson: t.string() },
  (ctx, args) => {
    const products = parseArray<ProductInput & { variants?: VariantInput[] }>(
      args.productsJson,
      'products'
    );
    const scenarios = parseArray<ScenarioInput>(
      args.scenariosJson,
      'scenarios'
    );

    const keepProducts = new Set<string>();
    const keepVariants = new Set<string>();
    for (const item of products) {
      const productId = requireId(item.productId, 'product_id');
      keepProducts.add(productId);
      const row = {
        productId,
        name: requireId(item.name, 'product_name'),
        category: requireId(item.category, 'product_category'),
        description:
          typeof item.description === 'string' ? item.description : '',
        baseAppeal: clampU32(item.baseAppeal, 'base_appeal', 1, 100),
        active: item.active ?? true,
      };
      const existing = ctx.db.productTemplate.productId.find(productId);
      if (existing) {
        ctx.db.productTemplate.productId.update({ ...existing, ...row });
      } else {
        ctx.db.productTemplate.insert(row);
      }

      for (const rawVariant of item.variants ?? []) {
        const variantId = requireId(rawVariant.variantId, 'variant_id');
        keepVariants.add(variantId);
        const variantRow = {
          variantId,
          productId,
          name: requireId(rawVariant.name, 'variant_name'),
          flavor: requireId(rawVariant.flavor, 'flavor'),
          contextTokens: clampU32(
            rawVariant.contextTokens,
            'context_tokens',
            1_000,
            2_000_000
          ),
          reasoning: clampU32(rawVariant.reasoning, 'reasoning', 1, 10),
          latency: clampU32(rawVariant.latency, 'latency', 1, 10),
          priceCents: clampU32(
            rawVariant.priceCents,
            'price_cents',
            0,
            250_000
          ),
          discountBps: clampU32(
            rawVariant.discountBps ?? 0,
            'discount_bps',
            0,
            9000
          ),
          active: rawVariant.active ?? true,
          featured: rawVariant.featured ?? false,
        };
        const existingVariant =
          ctx.db.variantTemplate.variantId.find(variantId);
        if (existingVariant) {
          ctx.db.variantTemplate.variantId.update({
            ...existingVariant,
            ...variantRow,
          });
        } else {
          ctx.db.variantTemplate.insert(variantRow);
        }
      }
    }

    for (const row of [...ctx.db.variantTemplate.iter()]) {
      if (!keepVariants.has(row.variantId)) ctx.db.variantTemplate.delete(row);
    }
    for (const row of [...ctx.db.productTemplate.iter()]) {
      if (!keepProducts.has(row.productId)) ctx.db.productTemplate.delete(row);
    }

    for (const rawScenario of scenarios) {
      const scenarioId = requireId(rawScenario.scenarioId, 'scenario_id');
      const row = {
        scenarioId,
        name: requireId(rawScenario.name, 'scenario_name'),
        description:
          typeof rawScenario.description === 'string'
            ? rawScenario.description
            : '',
        trafficPerTick: clampU32(
          rawScenario.trafficPerTick,
          'traffic_per_tick',
          1,
          30
        ),
        priceSensitivity: clampU32(
          rawScenario.priceSensitivity,
          'price_sensitivity',
          1,
          100
        ),
        rushBias: clampU32(rawScenario.rushBias, 'rush_bias', 1, 100),
        researchBias: clampU32(
          rawScenario.researchBias,
          'research_bias',
          1,
          100
        ),
        visualBias: clampU32(rawScenario.visualBias, 'visual_bias', 1, 100),
        memoryBias: clampU32(rawScenario.memoryBias, 'memory_bias', 1, 100),
        premiumBias: clampU32(rawScenario.premiumBias, 'premium_bias', 1, 100),
        volatility: clampU32(rawScenario.volatility, 'volatility', 0, 100),
      };
      const existing = ctx.db.scenario.scenarioId.find(scenarioId);
      if (existing) {
        ctx.db.scenario.scenarioId.update({ ...existing, ...row });
      } else {
        ctx.db.scenario.insert(row);
      }
    }
  }
);
