import { Router } from 'spacetimedb/server';
import { spacetimedb } from './schema';
import { stripeWebhookHandler } from './operations/webhook';

export const stripeWebhookRouter = spacetimedb.httpRouter(
  new Router().post('/stripe/webhook', stripeWebhookHandler)
);
