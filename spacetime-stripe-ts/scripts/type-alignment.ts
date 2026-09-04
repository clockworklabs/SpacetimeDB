import type Stripe from 'stripe';
import type * as v from 'valibot';
import type {
  ParsedStripeEvent,
  vStripeBillingPortalSessionResponse,
  vStripeCheckoutSessionResponse,
} from '../src/submodule/schema.ts';

type Assert<T extends true> = T;
type IsAssignable<From, To> = [From] extends [To] ? true : false;

type _StripeEvents = Assert<
  IsAssignable<
    | Stripe.CustomerCreatedEvent
    | Stripe.CustomerUpdatedEvent
    | Stripe.CustomerSubscriptionCreatedEvent
    | Stripe.CustomerSubscriptionUpdatedEvent
    | Stripe.CustomerSubscriptionDeletedEvent
    | Stripe.CheckoutSessionCompletedEvent
    | Stripe.InvoiceCreatedEvent
    | Stripe.InvoiceFinalizedEvent
    | Stripe.InvoicePaidEvent
    | Stripe.InvoicePaymentSucceededEvent
    | Stripe.InvoicePaymentFailedEvent
    | Stripe.PaymentIntentSucceededEvent,
    ParsedStripeEvent
  >
>;
type _CheckoutSessionResponse = Assert<
  IsAssignable<
    Stripe.Checkout.Session,
    v.InferOutput<typeof vStripeCheckoutSessionResponse>
  >
>;
type _BillingPortalSessionResponse = Assert<
  IsAssignable<
    Stripe.BillingPortal.Session,
    v.InferOutput<typeof vStripeBillingPortalSessionResponse>
  >
>;
