import type { ProcedureExport } from 'spacetimedb/server';
import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { requireAdmin } from './auth.js';
import { errors, normalizeHost, throwSenderError } from './validation.js';

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

const unitResult: ReturnType<typeof t.unit> = t.unit();

const setPosthogConfigArgs = {
  host: t.string(),
  projectApiKey: t.string(),
};
export const setPosthogConfig: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof setPosthogConfigArgs,
  typeof unitResult
> = spacetimedb.procedure(setPosthogConfigArgs, unitResult, (ctx, args) => {
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
});

const getPosthogConfigStatusResult = t.object('PostHogConfigStatus', {
  isConfigured: t.bool(),
  host: t.option(t.string()),
  projectApiKeyLength: t.u32(),
});
export const getPosthogConfigStatus: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof getPosthogConfigStatusResult
> = spacetimedb.procedure({}, getPosthogConfigStatusResult, ctx =>
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
