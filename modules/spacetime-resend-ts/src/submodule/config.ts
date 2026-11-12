import type { ProcedureExport } from 'spacetimedb/server';
import {
  spacetimedb,
  t,
  unitResult,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { adminVerdict, denyIfNotAdmin } from './auth.js';
import { throwSenderError } from './validation.js';
import { errors } from './errors.js';

export type ResendConfig = {
  apiKey: string;
  webhookSigningSecret: string | undefined;
  defaultFrom: string | undefined;
};

export function loadConfigOrThrow(ctx: WriteCtx): ResendConfig {
  const row = ctx.db.resendConfig.singleton.find(true);
  if (!row) {
    throwSenderError(
      `${errors.configNotSet}: call set_resend_config(...) first`
    );
  }
  return {
    apiKey: row.apiKey,
    webhookSigningSecret: row.webhookSigningSecret,
    defaultFrom: row.defaultFrom,
  };
}

export function loadConfigOrThrowFromProcedure(
  ctx: ProcedureModuleCtx
): ResendConfig {
  return ctx.withTx(tx => loadConfigOrThrow(tx));
}

function upsertConfig(
  ctx: WriteCtx,
  args: {
    apiKey: string;
    webhookSigningSecret?: string | undefined;
    defaultFrom?: string | undefined;
  }
) {
  const existing = ctx.db.resendConfig.singleton.find(true);
  const row = {
    singleton: true,
    apiKey: args.apiKey,
    webhookSigningSecret:
      args.webhookSigningSecret ?? existing?.webhookSigningSecret,
    defaultFrom: args.defaultFrom ?? existing?.defaultFrom,
    updatedAt: ctx.timestamp,
  };
  if (!existing) {
    ctx.db.resendConfig.insert(row);
    return;
  }
  ctx.db.resendConfig.singleton.update(row);
}

// Requires an admin seeded by the database owner; no public first-call bootstrap.
const setResendConfigArgs = {
  apiKey: t.string(),
  webhookSigningSecret: t.option(t.string()),
  defaultFrom: t.option(t.string()),
};
export const setResendConfig: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof setResendConfigArgs,
  typeof unitResult
> = spacetimedb.procedure(setResendConfigArgs, unitResult, (ctx, args) => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  ctx.withTx(tx => {
    upsertConfig(tx, args);
  });
  return {};
});

const getResendConfigStatusResult = t.object('ResendConfigStatus', {
  isConfigured: t.bool(),
  hasWebhookSecret: t.bool(),
  defaultFrom: t.option(t.string()),
  apiKeyLength: t.u16(),
});
export const getResendConfigStatus: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof getResendConfigStatusResult
> = spacetimedb.procedure({}, getResendConfigStatusResult, ctx => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  return ctx.withTx(tx => {
    const row = tx.db.resendConfig.singleton.find(true);
    if (!row) {
      return {
        isConfigured: false,
        hasWebhookSecret: false,
        defaultFrom: undefined,
        apiKeyLength: 0,
      };
    }
    return {
      isConfigured: true,
      hasWebhookSecret: row.webhookSigningSecret !== undefined,
      defaultFrom: row.defaultFrom,
      apiKeyLength: row.apiKey.length,
    };
  });
});
