import {
  Range,
  spacetimedb,
  t,
  type ModuleTimestamp,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import * as stripe from '@spacetimedb/stripe/submodule';
import { requireAdmin } from './auth';
import { stringArrayFromJson, throwSenderError } from './validation';

const DEFAULT_STORE_PRODUCTS: Array<{
  productId: string;
  name: string;
  description: string;
  mode: string;
  priceLabel: string;
  perks: string[];
  sortOrder: bigint;
}> = [
  {
    productId: 'orbital-starter-pack',
    name: 'Orbital Starter Pack',
    description: 'One-time booster crate for first-wave pilots.',
    mode: 'payment',
    priceLabel: '$19.00',
    perks: [
      'Orbital pilot badge',
      'Nebula ship skin',
      'Priority support queue',
    ],
    sortOrder: 10n,
  },
  {
    productId: 'warp-pass',
    name: 'Warp Pass',
    description: 'Monthly command-tier subscription.',
    mode: 'subscription',
    priceLabel: '$9.00 / month',
    perks: [
      'Expanded cargo slots',
      'Telemetry dashboard',
      'Beta sector access',
    ],
    sortOrder: 20n,
  },
  {
    productId: 'fleet-command-bundle',
    name: 'Fleet Command Bundle',
    description: 'Multi-seat bundle for your squad.',
    mode: 'payment',
    priceLabel: '$49.00',
    perks: [
      '5 pilot bundle',
      'Faction banner cosmetic',
      'Shared vault upgrade',
    ],
    sortOrder: 30n,
  },
];

const stripeHttpResponse = t.object('StoreStripeHttpResponse', {
  status: t.u16(),
  body: t.string(),
});

const storeValidateStripePriceResult = t.object(
  'StoreValidateStripePriceResult',
  {
    valid: t.bool(),
    status: t.u16(),
    active: t.option(t.bool()),
    currency: t.option(t.string()),
    unitAmount: t.option(t.i64()),
    livemode: t.option(t.bool()),
    type: t.option(t.string()),
    message: t.option(t.string()),
    code: t.option(t.string()),
    errorType: t.option(t.string()),
  }
);

const storeGetOrCreateCustomerResult = t.object(
  'StoreGetOrCreateCustomerResult',
  {
    customerId: t.string(),
    isNew: t.bool(),
  }
);

const storeCheckoutLineItem = t.object('StoreCheckoutLineItem', {
  priceId: t.string(),
  quantity: t.i64(),
});

const storeCheckoutSessionResult = t.object('StoreCheckoutSessionResult', {
  sessionId: t.string(),
  url: t.option(t.string()),
});

function parseAmountCents(priceLabel: string): bigint {
  const match = /(\d+)(?:\.(\d{1,2}))?/.exec(priceLabel);
  if (!match) {
    throwSenderError(`store.invalid_price_label:${priceLabel}`);
  }
  const whole = BigInt(Number.parseInt(match[1], 10));
  const fraction = BigInt(
    match[2] ? Number.parseInt(match[2].padEnd(2, '0'), 10) : 0
  );
  return whole * 100n + fraction;
}

function getLookupKey(databaseIdentity: string, productId: string): string {
  const safeDb = databaseIdentity.replace(/[^A-Za-z0-9_-]/g, '_');
  const safeProduct = productId.replace(/[^A-Za-z0-9_-]/g, '_');
  return `stdb_${safeDb}_${safeProduct}`;
}

function encodeFormField(value: string): string {
  return encodeURIComponent(value).replace(/%20/g, '+');
}

function formBody(pairs: Array<[string, string | undefined]>): string {
  return pairs
    .filter((pair): pair is [string, string] => pair[1] !== undefined)
    .map(([key, value]) => `${encodeFormField(key)}=${encodeFormField(value)}`)
    .join('&');
}

function callStripe(
  ctx: ProcedureModuleCtx,
  method: string,
  path: string,
  body?: string
) {
  return stripe.stripe_api_request(ctx.as.stripe, {
    method,
    path,
    formBody: body,
    idempotencyKey: undefined,
  }) as { status: number; body: string };
}

type StripeObject = Record<string, unknown>;

function isObject(value: unknown): value is StripeObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseStripeResponse(response: {
  status: number;
  body: string;
}): StripeObject {
  let parsed: unknown;
  try {
    parsed = JSON.parse(response.body || '{}');
  } catch {
    throwSenderError(`store.stripe_invalid_json:${response.status}`);
  }
  if (!isObject(parsed))
    throwSenderError(`store.stripe_invalid_response:${response.status}`);
  if (response.status < 200 || response.status >= 300) {
    const error = isObject(parsed.error) ? parsed.error : undefined;
    const message =
      typeof error?.message === 'string'
        ? error.message
        : `Stripe API returned ${response.status}`;
    throwSenderError(`store.stripe_api_error:${message}`);
  }
  return parsed;
}

function stripeObjectId(value: StripeObject, kind: string): string {
  if (typeof value.id !== 'string' || value.id.length === 0) {
    throwSenderError(`store.stripe_invalid_${kind}`);
  }
  return value.id;
}

function findStripePriceByLookupKey(
  ctx: ProcedureModuleCtx,
  lookupKey: string
): StripeObject | undefined {
  const query = formBody([
    ['lookup_keys[]', lookupKey],
    ['active', 'true'],
    ['limit', '1'],
  ]);
  const parsed = parseStripeResponse(
    callStripe(ctx, 'GET', `/v1/prices?${query}`)
  );
  if (!Array.isArray(parsed.data)) return undefined;
  const first = parsed.data[0];
  return isObject(first) ? first : undefined;
}

function createStripeProduct(
  ctx: ProcedureModuleCtx,
  args: {
    productId: string;
    name: string;
    description: string;
    databaseIdentity: string;
  }
) {
  return parseStripeResponse(
    callStripe(
      ctx,
      'POST',
      '/v1/products',
      formBody([
        ['name', args.name],
        ['description', args.description],
        ['metadata[stdb_product_id]', args.productId],
        ['metadata[stdb_db]', args.databaseIdentity],
      ])
    )
  );
}

function createStripePrice(
  ctx: ProcedureModuleCtx,
  args: {
    stripeProductId: string;
    productId: string;
    mode: string;
    priceLabel: string;
    lookupKey: string;
    databaseIdentity: string;
  }
) {
  const pairs: Array<[string, string | undefined]> = [
    ['product', args.stripeProductId],
    ['currency', 'usd'],
    ['unit_amount', String(parseAmountCents(args.priceLabel))],
    ['lookup_key', args.lookupKey],
    ['metadata[stdb_product_id]', args.productId],
    ['metadata[stdb_db]', args.databaseIdentity],
  ];
  if (args.mode === 'subscription') {
    pairs.push(['recurring[interval]', 'month']);
  }
  return parseStripeResponse(
    callStripe(ctx, 'POST', '/v1/prices', formBody(pairs))
  );
}

function upsertStoreProduct(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    productId: string;
    name: string;
    description: string;
    mode: string;
    priceLabel: string;
    stripePriceId: string | undefined;
    perksJson: string | undefined;
    active: boolean;
    sortOrder: bigint;
  }
) {
  const existing = ctx.db.storeProduct.productId.find(args.productId);
  const row = {
    productId: args.productId,
    name: args.name,
    description: args.description,
    mode: args.mode,
    priceLabel: args.priceLabel,
    stripePriceId: args.stripePriceId,
    perksJson: args.perksJson,
    active: args.active,
    sortOrder: args.sortOrder,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.storeProduct.insert(row);
    return;
  }
  if (ctx.db.storeProduct.productId.update) {
    ctx.db.storeProduct.productId.update(row);
  } else {
    ctx.db.storeProduct.delete(existing);
    ctx.db.storeProduct.insert(row);
  }
}

export const upsert_store_product = spacetimedb.reducer(
  {
    productId: t.string(),
    name: t.string(),
    description: t.string(),
    mode: t.string(),
    priceLabel: t.string(),
    stripePriceId: t.option(t.string()),
    perksJson: t.option(t.string()),
    active: t.option(t.bool()),
    sortOrder: t.option(t.i64()),
  },
  (ctx, args) => {
    const tx = ctx;
    requireAdmin(tx, ctx.sender);
    upsertStoreProduct(tx, ctx.timestamp, {
      productId: args.productId,
      name: args.name,
      description: args.description,
      mode: args.mode,
      priceLabel: args.priceLabel,
      stripePriceId: args.stripePriceId,
      perksJson: args.perksJson,
      active: args.active ?? true,
      sortOrder: args.sortOrder ?? 0n,
    });
  }
);

export const seed_default_store_products = spacetimedb.reducer(
  { force: t.option(t.bool()) },
  (ctx, args) => {
    const force = args.force ?? false;
    const tx = ctx;
    requireAdmin(tx, ctx.sender);
    const hasAnyProducts = tx.db.storeProduct.count() > 0n;
    if (hasAnyProducts && !force) return;

    for (const product of DEFAULT_STORE_PRODUCTS) {
      upsertStoreProduct(tx, ctx.timestamp, {
        productId: product.productId,
        name: product.name,
        description: product.description,
        mode: product.mode,
        priceLabel: product.priceLabel,
        stripePriceId: undefined,
        perksJson: JSON.stringify(product.perks),
        active: true,
        sortOrder: product.sortOrder,
      });
    }
  }
);

export const list_store_products_json = spacetimedb.procedure(
  {},
  t.string(),
  ctx =>
    ctx.withTx(tx => {
      const rows = [
        ...tx.db.storeProduct.byActiveSort.filter([true, new Range()]),
      ];
      const mapped = rows.map(row => ({
        id: row.productId,
        name: row.name,
        description: row.description,
        mode: row.mode,
        priceLabel: row.priceLabel,
        priceId: row.stripePriceId ?? '',
        perks: stringArrayFromJson(row.perksJson),
        sortOrder: Number(row.sortOrder),
      }));
      return JSON.stringify(mapped);
    })
);

export const configure_stripe = spacetimedb.procedure(
  {
    secretKey: t.string(),
    stripeVersion: t.option(t.string()),
    webhookSigningSecret: t.option(t.string()),
  },
  t.unit(),
  (ctx, args) => {
    const verdict = ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      return true;
    });
    void verdict;
    return stripe.set_stripe_config(ctx.as.stripe, {
      secretKey: args.secretKey,
      stripeVersion: args.stripeVersion,
      webhookSigningSecret: args.webhookSigningSecret,
    });
  }
);

export const store_stripe_api_request = spacetimedb.procedure(
  {
    method: t.string(),
    path: t.string(),
    formBody: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  stripeHttpResponse,
  (ctx, args) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    return stripe.stripe_api_request(ctx.as.stripe, {
      method: args.method,
      path: args.path,
      formBody: args.formBody,
      idempotencyKey: args.idempotencyKey,
    }) as { status: number; body: string };
  }
);

export const validate_store_stripe_price = spacetimedb.procedure(
  { priceId: t.string() },
  storeValidateStripePriceResult,
  (ctx, args) =>
    stripe.validate_stripe_price(ctx.as.stripe, {
      priceId: args.priceId,
    }) as {
      valid: boolean;
      status: number;
      active: boolean | undefined;
      currency: string | undefined;
      unitAmount: bigint | undefined;
      livemode: boolean | undefined;
      type: string | undefined;
      message: string | undefined;
      code: string | undefined;
      errorType: string | undefined;
    }
);

export const get_store_webhook_event_count = spacetimedb.procedure(
  {},
  t.i64(),
  ctx => stripe.get_webhook_event_count(ctx.as.stripe, {}) as bigint
);

export const get_or_create_store_customer = spacetimedb.procedure(
  {
    userId: t.string(),
    email: t.option(t.string()),
    name: t.option(t.string()),
  },
  storeGetOrCreateCustomerResult,
  (ctx, args) =>
    stripe.get_or_create_customer(ctx.as.stripe, {
      userId: args.userId,
      email: args.email,
      name: args.name,
    }) as { customerId: string; isNew: boolean }
);

export const create_store_checkout_session = spacetimedb.procedure(
  {
    items: t.array(storeCheckoutLineItem),
    customerId: t.option(t.string()),
    mode: t.string(),
    successUrl: t.string(),
    cancelUrl: t.string(),
    metadataJson: t.option(t.string()),
    subscriptionMetadataJson: t.option(t.string()),
    paymentIntentMetadataJson: t.option(t.string()),
  },
  storeCheckoutSessionResult,
  (ctx, args) =>
    stripe.create_checkout_session(ctx.as.stripe, {
      items: args.items,
      customerId: args.customerId,
      mode: args.mode,
      successUrl: args.successUrl,
      cancelUrl: args.cancelUrl,
      metadataJson: args.metadataJson,
      subscriptionMetadataJson: args.subscriptionMetadataJson,
      paymentIntentMetadataJson: args.paymentIntentMetadataJson,
    }) as { sessionId: string; url: string | undefined }
);

export const sync_store_products_with_stripe = spacetimedb.procedure(
  {},
  t.string(),
  ctx => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    const databaseIdentity = ctx.databaseIdentity.toHexString();
    const rows = ctx.withTx(tx => [
      ...tx.db.storeProduct.byActiveSort.filter([true, new Range()]),
    ]);
    const results: Array<{
      productId: string;
      priceId: string;
      action: string;
    }> = [];

    for (const row of rows) {
      if (row.stripePriceId) {
        results.push({
          productId: row.productId,
          priceId: row.stripePriceId,
          action: 'kept',
        });
        continue;
      }

      const lookupKey = getLookupKey(databaseIdentity, row.productId);
      const existing = findStripePriceByLookupKey(ctx, lookupKey);
      const price =
        existing ??
        (() => {
          const product = createStripeProduct(ctx, {
            productId: row.productId,
            name: row.name,
            description: row.description,
            databaseIdentity,
          });
          return createStripePrice(ctx, {
            stripeProductId: stripeObjectId(product, 'product'),
            productId: row.productId,
            mode: row.mode,
            priceLabel: row.priceLabel,
            lookupKey,
            databaseIdentity,
          });
        })();

      ctx.withTx(tx => {
        const current = tx.db.storeProduct.productId.find(row.productId);
        if (!current)
          throwSenderError(`store.product_not_found:${row.productId}`);
        upsertStoreProduct(tx, ctx.timestamp, {
          productId: current.productId,
          name: current.name,
          description: current.description,
          mode: current.mode,
          priceLabel: current.priceLabel,
          stripePriceId: stripeObjectId(price, 'price'),
          perksJson: current.perksJson,
          active: current.active,
          sortOrder: current.sortOrder,
        });
      });
      results.push({
        productId: row.productId,
        priceId: stripeObjectId(price, 'price'),
        action: existing ? 'linked' : 'created',
      });
    }

    return JSON.stringify(results);
  }
);

export const set_store_product_price = spacetimedb.reducer(
  { productId: t.string(), stripePriceId: t.string() },
  (ctx, args) => {
    const tx = ctx;
    requireAdmin(tx, ctx.sender);
    const existing = tx.db.storeProduct.productId.find(args.productId);
    if (!existing) {
      throwSenderError(`store.product_not_found:${args.productId}`);
    }
    upsertStoreProduct(tx, ctx.timestamp, {
      productId: existing.productId,
      name: existing.name,
      description: existing.description,
      mode: existing.mode,
      priceLabel: existing.priceLabel,
      stripePriceId: args.stripePriceId,
      perksJson: existing.perksJson,
      active: existing.active,
      sortOrder: existing.sortOrder,
    });
  }
);

export const clear_store_product_price = spacetimedb.reducer(
  { productId: t.string() },
  (ctx, args) => {
    const tx = ctx;
    requireAdmin(tx, ctx.sender);
    const existing = tx.db.storeProduct.productId.find(args.productId);
    if (!existing) {
      throwSenderError(`store.product_not_found:${args.productId}`);
    }
    upsertStoreProduct(tx, ctx.timestamp, {
      productId: existing.productId,
      name: existing.name,
      description: existing.description,
      mode: existing.mode,
      priceLabel: existing.priceLabel,
      stripePriceId: undefined,
      perksJson: existing.perksJson,
      active: existing.active,
      sortOrder: existing.sortOrder,
    });
  }
);
