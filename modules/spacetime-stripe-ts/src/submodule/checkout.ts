import type Stripe from 'stripe';
import { errors } from './errors.js';

const protectedFields = [
  'customer',
  'customer_account',
  'customer_email',
  'customer_creation',
  'client_reference_id',
  'line_items',
  'mode',
  'ui_mode',
  'success_url',
  'cancel_url',
  'return_url',
  'metadata',
  'currency',
] as const;

export type CheckoutOptions = Omit<
  Stripe.Checkout.SessionCreateParams,
  (typeof protectedFields)[number]
>;

export type UserCheckoutArgs = {
  userId: string;
  email?: string;
  name?: string;
  metadata?: Record<string, string>;
  params?: CheckoutOptions;
  idempotencyKey?: string;
} & (
  | {
      mode: 'payment' | 'subscription';
      items: Array<{ priceId: string; quantity: bigint }>;
    }
  | { mode: 'setup'; currency: string; items?: never }
) &
  (
    | { uiMode?: 'hosted_page'; successUrl: string; cancelUrl: string }
    | { uiMode: 'embedded_page'; returnUrl: string }
  );

// Stripe's form protocol uses bracketed keys for nested objects and arrays.
function encodeForm(value: unknown, key: string, pairs: string[]): void {
  if (value === undefined) return;
  if (value !== null && typeof value === 'object') {
    for (const [child, item] of Object.entries(value)) {
      encodeForm(item, key ? `${key}[${child}]` : child, pairs);
    }
    if (Array.isArray(value) && value.length === 0) encodeForm('', key, pairs);
    return;
  }
  if (
    value !== null &&
    !['string', 'number', 'boolean', 'bigint'].includes(typeof value)
  ) {
    throw new Error(errors.checkoutInvalidParameter);
  }
  pairs.push(
    `${encodeURIComponent(key)}=${encodeURIComponent(value === null ? '' : String(value))}`
  );
}

export function checkoutForm(
  args: UserCheckoutArgs,
  customerId: string
): string {
  const params = args.params ?? {};
  for (const field of protectedFields) {
    if (Object.hasOwn(params, field))
      throw new Error(`${errors.checkoutProtectedParameter}:${field}`);
  }
  if (!args.userId) throw new Error(errors.checkoutUserRequired);
  if (args.mode !== 'setup') {
    if (!args.items?.length)
      throw new Error(errors.checkoutSessionRequiresItems);
    for (const item of args.items) {
      if (
        !item.priceId ||
        item.quantity <= 0n ||
        item.quantity > BigInt(Number.MAX_SAFE_INTEGER)
      ) {
        throw new Error(errors.checkoutInvalidQuantity);
      }
    }
  }
  if (
    (args.mode !== 'subscription' && params.subscription_data !== undefined) ||
    (args.mode !== 'payment' && params.payment_intent_data !== undefined) ||
    (args.mode !== 'setup' && params.setup_intent_data !== undefined)
  ) {
    throw new Error(errors.checkoutModeParametersMismatch);
  }
  const metadata = { ...args.metadata, userId: args.userId };
  const body: Stripe.Checkout.SessionCreateParams = {
    ...params,
    customer: customerId,
    mode: args.mode,
    ui_mode: args.uiMode ?? 'hosted_page',
    metadata,
    ...(args.mode === 'setup'
      ? {
          currency: args.currency,
          setup_intent_data: {
            ...params.setup_intent_data,
            metadata: { ...params.setup_intent_data?.metadata, ...metadata },
          },
        }
      : {
          line_items: args.items.map(item => ({
            price: item.priceId,
            quantity: Number(item.quantity),
          })),
        }),
    ...(args.uiMode === 'embedded_page'
      ? { return_url: args.returnUrl }
      : { success_url: args.successUrl, cancel_url: args.cancelUrl }),
    ...(args.mode === 'subscription'
      ? {
          subscription_data: {
            ...params.subscription_data,
            metadata: { ...params.subscription_data?.metadata, ...metadata },
          },
        }
      : {}),
    ...(args.mode === 'payment'
      ? {
          payment_intent_data: {
            ...params.payment_intent_data,
            metadata: { ...params.payment_intent_data?.metadata, ...metadata },
          },
        }
      : {}),
  };
  const pairs: string[] = [];
  encodeForm(body, '', pairs);
  return pairs.join('&');
}
