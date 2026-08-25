import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { adminVerdict, denyIfNotAdmin } from './auth';
import { throwSenderError } from './utils';

export type StripeConfig = {
  secretKey: string;
  stripeVersion: string | undefined;
  webhookSigningSecret: string | undefined;
};

export function loadConfigOrThrow(ctx: WriteCtx): StripeConfig {
  const row = ctx.db.stripeConfig.singleton.find(true);
  if (!row) {
    throwSenderError(
      'stripe.config_not_set: call set_stripe_config(...) first'
    );
  }
  return {
    secretKey: row.secretKey,
    stripeVersion: row.stripeVersion,
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
  if (ctx.db.stripeConfig.singleton.update) {
    ctx.db.stripeConfig.singleton.update(row);
  } else {
    ctx.db.stripeConfig.delete(existing);
    ctx.db.stripeConfig.insert(row);
  }
}

export const set_stripe_config = spacetimedb.procedure(
  {
    secretKey: t.string(),
    stripeVersion: t.option(t.string()),
    webhookSigningSecret: t.option(t.string()),
  },
  t.unit(),
  (ctx, args) => {
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
  }
);

export const set_stripe_webhook_signing_secret = spacetimedb.procedure(
  { webhookSigningSecret: t.string() },
  t.unit(),
  (ctx, { webhookSigningSecret }) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    const secret = webhookSigningSecret.trim();
    if (!secret) throwSenderError('stripe.invalid_webhook_signing_secret');
    ctx.withTx(tx => {
      const existing = tx.db.stripeConfig.singleton.find(true);
      if (!existing) {
        throwSenderError(
          'stripe.config_not_set: call set_stripe_config(...) first'
        );
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

export const get_stripe_config_status = spacetimedb.procedure(
  {},
  t.object('StripeConfigStatus', {
    isConfigured: t.bool(),
    hasWebhookSecret: t.bool(),
    stripeVersion: t.option(t.string()),
    secretKeyLength: t.u16(),
  }),
  ctx => {
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
  }
);
