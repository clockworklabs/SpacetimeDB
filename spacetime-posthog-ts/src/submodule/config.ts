import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { requireAdmin } from './auth';
import { normalizeHost, throwSenderError } from './validation';

export type PostHogConfig = {
  host: string;
  projectApiKey: string;
};

export function loadConfigOrThrow(ctx: WriteCtx): PostHogConfig {
  const row = ctx.db.posthogConfig.singleton.find(true);
  if (!row) {
    throwSenderError('posthog.config_missing');
  }
  return {
    host: row.host,
    projectApiKey: row.projectApiKey,
  };
}

export function loadConfigOrThrowFromProcedure(
  ctx: ProcedureModuleCtx
): PostHogConfig {
  return ctx.withTx(tx => loadConfigOrThrow(tx));
}

export const set_posthog_config = spacetimedb.procedure(
  {
    host: t.string(),
    projectApiKey: t.string(),
  },
  t.unit(),
  (ctx, args) => {
    const host = normalizeHost(args.host);
    const projectApiKey = args.projectApiKey.trim();
    if (!projectApiKey) throwSenderError('posthog.invalid_project_api_key');
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

export const get_posthog_config_status = spacetimedb.procedure(
  {},
  t.string(),
  ctx =>
    ctx.withTx(tx => {
      const row = tx.db.posthogConfig.singleton.find(true);
      if (!row) {
        return JSON.stringify({
          isConfigured: false,
          host: undefined,
          projectApiKeyLength: 0,
        });
      }
      return JSON.stringify({
        isConfigured: true,
        host: row.host,
        projectApiKeyLength: row.projectApiKey.length,
      });
    })
);
