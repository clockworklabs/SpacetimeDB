import { Router } from 'spacetimedb/server';
import { spacetimedb } from './schema';
import { stripe_webhook_handler } from './operations/webhook';

export const stripeWebhookRouter = spacetimedb.httpRouter(
  new Router().post('/stripe/webhook', stripe_webhook_handler)
);
