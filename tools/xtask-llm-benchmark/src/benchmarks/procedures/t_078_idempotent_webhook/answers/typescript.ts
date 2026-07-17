import { Router, SyncResponse, schema, table, t } from 'spacetimedb/server';

const processedEvent = table({ name: 'processed_event' }, { eventId: t.string().primaryKey() });
const webhookState = table({ name: 'webhook_state', public: true }, {
  key: t.string().primaryKey(), lastSequence: t.u64(), value: t.string(),
});
const spacetimedb = schema({ processedEvent, webhookState });
export default spacetimedb;

export const webhook = spacetimedb.httpHandler((ctx, request) => {
  const parts = request.text().split('|', 3);
  if (parts.length !== 3) return new SyncResponse('invalid', { status: 400 });
  const [eventId, sequenceText, value] = parts;
  const sequence = BigInt(sequenceText);
  const outcome = ctx.withTx(tx => {
    if (tx.db.processedEvent.eventId.find(eventId)) return 'duplicate';
    tx.db.processedEvent.insert({ eventId });
    const state = tx.db.webhookState.key.find('account');
    if (state) {
      if (sequence <= state.lastSequence) return 'stale';
      tx.db.webhookState.key.update({ ...state, lastSequence: sequence, value });
    } else tx.db.webhookState.insert({ key: 'account', lastSequence: sequence, value });
    return 'applied';
  });
  return new SyncResponse(outcome);
});
export const routes = spacetimedb.httpRouter(new Router().post('/webhook', webhook));
