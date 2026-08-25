import { Range, t } from 'spacetimedb/server';
import * as posthog from '@spacetimedb/posthog/submodule';

import {
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
  cafeDeliveryLogViewRow,
  cafeAnalyticsSummaryRow,
  spacetimedb,
} from './schema';
export { default } from './schema';

import { newestFirst } from './recent';

export const flush_analytics = spacetimedb.procedure(
  { limit: t.u32() },
  t.string(),
  (ctx, args) =>
    JSON.stringify(posthog.flushOutbox(ctx.as.posthog, { limit: args.limit }))
);

export const cafeProducts = spacetimedb.view(
  { name: 'cafe_products', public: true },
  t.array(product.rowType),
  ctx => [...ctx.db.product.byOwner.filter(ctx.sender.toHexString())]
);

export const cafeVariants = spacetimedb.view(
  { name: 'cafe_variants', public: true },
  t.array(variant.rowType),
  ctx => [...ctx.db.variant.byOwner.filter(ctx.sender.toHexString())]
);

export const cafeScenarios = spacetimedb.view(
  { name: 'cafe_scenarios', public: true },
  t.array(scenario.rowType),
  ctx => [...ctx.db.scenario.iter()]
);

export const cafeConfig = spacetimedb.view(
  { name: 'cafe_config', public: true },
  t.array(simConfig.rowType),
  ctx => {
    const row = ctx.db.simConfig.owner.find(ctx.sender.toHexString());
    return row ? [row] : [];
  }
);

export const cafeMetrics = spacetimedb.view(
  { name: 'cafe_metrics', public: true },
  t.array(metrics.rowType),
  ctx => {
    const row = ctx.db.metrics.owner.find(ctx.sender.toHexString());
    return row ? [row] : [];
  }
);

export const cafeEcon = spacetimedb.view(
  { name: 'cafe_econ', public: true },
  t.array(econ.rowType),
  ctx => {
    const row = ctx.db.econ.owner.find(ctx.sender.toHexString());
    return row ? [row] : [];
  }
);

export const cafeQueue = spacetimedb.view(
  { name: 'cafe_queue', public: true },
  t.array(waitingBot.rowType),
  ctx => {
    const rows = [...ctx.db.waitingBot.owner.filter(ctx.sender.toHexString())];
    rows.sort((a, b) =>
      a.queueId < b.queueId ? -1 : a.queueId > b.queueId ? 1 : 0
    );
    return rows;
  }
);

export const cafeRecentSessions = spacetimedb.view(
  { name: 'cafe_recent_sessions', public: true },
  t.array(botSession.rowType),
  ctx =>
    newestFirst([
      ...ctx.db.botSession.owner.filter(ctx.sender.toHexString()),
    ]).slice(0, 80)
);

export const cafeRecentPurchases = spacetimedb.view(
  { name: 'cafe_recent_purchases', public: true },
  t.array(purchase.rowType),
  ctx =>
    newestFirst([
      ...ctx.db.purchase.owner.filter(ctx.sender.toHexString()),
    ]).slice(0, 50)
);

export const cafeRecentActivity = spacetimedb.view(
  { name: 'cafe_recent_activity', public: true },
  t.array(activity.rowType),
  ctx =>
    newestFirst([
      ...ctx.db.activity.owner.filter(ctx.sender.toHexString()),
    ]).slice(0, 80)
);

export const posthogOutboxAdmin = spacetimedb.view(
  { name: 'posthog_outbox_admin', public: true },
  posthog.t.array(posthog.posthogOutbox.rowType),
  ctx => {
    const admin = ctx.db.posthog.posthogAdminIdentity.identity.find(ctx.sender);
    return admin
      ? [
          ...ctx.db.posthog.posthogOutbox.byStatus.filter(
            posthog.OutboxStatus.Queued
          ),
        ]
      : [];
  }
);

export const posthogDeliveryLogAdmin = spacetimedb.view(
  { name: 'posthog_delivery_log_admin', public: true },
  posthog.t.array(cafeDeliveryLogViewRow),
  ctx => {
    const admin = ctx.db.posthog.posthogAdminIdentity.identity.find(ctx.sender);
    if (!admin) return [];
    const rows = [
      ...ctx.db.posthog.posthogDeliveryLog.byAttemptedAt.filter(new Range()),
    ];
    rows.sort((a, b) => {
      const av = a.attemptedAt.microsSinceUnixEpoch;
      const bv = b.attemptedAt.microsSinceUnixEpoch;
      return av < bv ? 1 : av > bv ? -1 : 0;
    });
    return rows.slice(0, 50).map(row => ({
      deliveryId: row.deliveryId.toString(),
      source: row.source.tag,
      distinctId: row.distinctId,
      event: row.event,
      ok: row.ok,
      statusCode: row.statusCode,
      responseBody: row.responseBody,
      attemptedAt: row.attemptedAt,
    }));
  }
);

export const cafeAnalyticsSummary = spacetimedb.anonymousView(
  { name: 'cafe_analytics_summary', public: true },
  posthog.t.array(cafeAnalyticsSummaryRow),
  ctx => {
    const stats = ctx.db.posthog.posthogDeliveryStats.singleton.find(true);
    return [
      {
        queued: stats?.pending ?? 0n,
        delivered: stats?.delivered ?? 0n,
        failed: stats?.failed ?? 0n,
      },
    ];
  }
);
