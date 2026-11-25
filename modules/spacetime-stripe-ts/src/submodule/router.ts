import { Router } from 'spacetimedb/server';
import { spacetimedb } from './schema.js';
import { stripeWebhookHandler } from './operations/webhook.js';

export const stripeWebhookRouter = spacetimedb.httpRouter(
  new Router().post('/stripe/webhook', stripeWebhookHandler)
);
