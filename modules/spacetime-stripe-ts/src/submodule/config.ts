import type { ProcedureExport } from 'spacetimedb/server';
import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { adminVerdict, denyIfNotAdmin } from './auth.js';
import { errors } from './errors.js';
import { throwSenderError } from './validation.js';

export type StripeConfig = {
  secretKey: string;
  stripeVersion: string | undefined;
  webhookSigningSecret: string | undefined;
};

export function loadConfigOrThrow(ctx: WriteCtx): StripeConfig {
  const row = ctx.db.stripeConfig.singleton.find(true);
  if (!row) {
    throwSenderError(errors.configNotSet);
  }
  return {
    secretKey: row.secretKey,
    stripeVersion: row.stripeVersion ?? '2026-08-26.dahlia',
    webhookSigningSecret: row.webhookSigningSecret,
  };
}

export function loadConfigOrThrowFromProcedure(
  ctx: ProcedureModuleCtx
): StripeConfig {
  return ctx.withTx(tx => loadConfigOrThrow(tx));
}

function upsertConfig(
  ctx: WriteCtx,
  args: {
    secretKey: string;
    stripeVersion: string | undefined;
    webhookSigningSecret: string | undefined;
  }
) {
  const existing = ctx.db.stripeConfig.singleton.find(true);
  const row = {
    singleton: true,
    secretKey: args.secretKey,
    stripeVersion: args.stripeVersion ?? existing?.stripeVersion,
    webhookSigningSecret:
      args.webhookSigningSecret ?? existing?.webhookSigningSecret,
    updatedAt: ctx.timestamp,
  };
  if (!existing) {
    ctx.db.stripeConfig.insert(row);
    return;
  }
  ctx.db.stripeConfig.singleton.update(row);
}

const unitResult: ReturnType<typeof t.unit> = t.unit();

const setStripeConfigArgs = {
  secretKey: t.string(),
  stripeVersion: t.option(t.string()),
  webhookSigningSecret: t.option(t.string()),
};
export const setStripeConfig: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof setStripeConfigArgs,
  typeof unitResult
> = spacetimedb.procedure(setStripeConfigArgs, unitResult, (ctx, args) => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  ctx.withTx(tx => {
    upsertConfig(tx, {
      secretKey: args.secretKey,
      stripeVersion: args.stripeVersion,
      webhookSigningSecret: args.webhookSigningSecret,
    });
  });
  return {};
});

const setStripeWebhookSigningSecretArgs = { webhookSigningSecret: t.string() };
export const setStripeWebhookSigningSecret: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof setStripeWebhookSigningSecretArgs,
  typeof unitResult
> = spacetimedb.procedure(
  setStripeWebhookSigningSecretArgs,
  unitResult,
  (ctx, { webhookSigningSecret }) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    const secret = webhookSigningSecret.trim();
    if (!secret) throwSenderError(errors.invalidWebhookSigningSecret);
    ctx.withTx(tx => {
      const existing = tx.db.stripeConfig.singleton.find(true);
      if (!existing) {
        throwSenderError(errors.configNotSet);
      }
      tx.db.stripeConfig.singleton.update({
        ...existing,
        webhookSigningSecret: secret,
        updatedAt: ctx.timestamp,
      });
    });
    return {};
  }
);

const getStripeConfigStatusResult = t.object('StripeConfigStatus', {
  isConfigured: t.bool(),
  hasWebhookSecret: t.bool(),
  stripeVersion: t.option(t.string()),
  secretKeyLength: t.u16(),
});
export const getStripeConfigStatus: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof getStripeConfigStatusResult
> = spacetimedb.procedure({}, getStripeConfigStatusResult, ctx => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  return ctx.withTx(tx => {
    const row = tx.db.stripeConfig.singleton.find(true);
    if (!row) {
      return {
        isConfigured: false,
        hasWebhookSecret: false,
        stripeVersion: undefined,
        secretKeyLength: 0,
      };
    }
    return {
      isConfigured: true,
      hasWebhookSecret: row.webhookSigningSecret !== undefined,
      stripeVersion: row.stripeVersion,
      secretKeyLength: row.secretKey.length,
    };
  });
});
