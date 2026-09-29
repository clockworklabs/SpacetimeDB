import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { requireAdmin } from './auth';
import { errors, normalizeHost, throwSenderError } from './validation';

export type PostHogConfig = {
  host: string;
  projectApiKey: string;
};

export function loadConfig(ctx: WriteCtx): PostHogConfig | undefined {
  const row = ctx.db.posthogConfig.singleton.find(true);
  return row ? { host: row.host, projectApiKey: row.projectApiKey } : undefined;
}

export function loadConfigOrThrowFromProcedure(
  ctx: ProcedureModuleCtx
): PostHogConfig {
  return (
    ctx.withTx(tx => loadConfig(tx)) ?? throwSenderError(errors.configMissing)
  );
}

export const setPosthogConfig = spacetimedb.procedure(
  {
    host: t.string(),
    projectApiKey: t.string(),
  },
  t.unit(),
  (ctx, args) => {
    const host = normalizeHost(args.host);
    const projectApiKey = args.projectApiKey.trim();
    if (!projectApiKey) throwSenderError(errors.invalidProjectApiKey);
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      const existing = tx.db.posthogConfig.singleton.find(true);
      const row = {
        singleton: true,
        host,
        projectApiKey,
        updatedAt: ctx.timestamp,
      };
      if (!existing) {
        tx.db.posthogConfig.insert(row);
      } else {
        tx.db.posthogConfig.singleton.update(row);
      }
    });
    return {};
  }
);

export const getPosthogConfigStatus = spacetimedb.procedure(
  {},
  t.object('PostHogConfigStatus', {
    isConfigured: t.bool(),
    host: t.option(t.string()),
    projectApiKeyLength: t.u32(),
  }),
  ctx =>
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      const row = tx.db.posthogConfig.singleton.find(true);
      return {
        isConfigured: row != null,
        host: row?.host,
        projectApiKeyLength: row?.projectApiKey.length ?? 0,
      };
    })
);
