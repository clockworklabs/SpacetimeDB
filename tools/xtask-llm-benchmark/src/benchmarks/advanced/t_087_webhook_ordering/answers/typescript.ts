import { schema, table, t } from 'spacetimedb/server';
import { Router, SyncResponse } from 'spacetimedb/server';
const webhookReceipt = table(
  { name: 'webhook_receipt', public: true },
  {
    eventId: t.string().primaryKey(),
    account: t.string(),
    sequence: t.u64(),
    value: t.string(),
  }
);
const webhookAccount = table(
  { name: 'webhook_account', public: true },
  {
    account: t.string().primaryKey(),
    sequence: t.u64(),
    value: t.string(),
  }
);
const spacetimedb = schema({ webhookReceipt, webhookAccount });
export default spacetimedb;

export const webhook = spacetimedb.httpHandler((ctx, request) => {
  const p = request.text().split('|');
  if (p.length !== 4 || p.some(s => !s) || !/^[0-9]+$/.test(p[2]))
    return new SyncResponse('invalid', { status: 400 });
  const sequence = BigInt(p[2]);
  if (sequence <= 0n || sequence > 18446744073709551615n)
    return new SyncResponse('invalid', { status: 400 });
  const [account, eventId, , value] = p;
  const result = ctx.withTx(tx => {
    const old = tx.db.webhookReceipt.eventId.find(eventId);
    if (old)
      return old.account === account &&
        old.sequence === sequence &&
        old.value === value
        ? 'duplicate'
        : 'conflict';
    tx.db.webhookReceipt.insert({ eventId, account, sequence, value });
    const state = tx.db.webhookAccount.account.find(account);
    if (state) {
      if (sequence <= state.sequence) return 'stale';
      tx.db.webhookAccount.account.update({ account, sequence, value });
    } else tx.db.webhookAccount.insert({ account, sequence, value });
    return 'applied';
  });
  return new SyncResponse(result);
});
export const routes = spacetimedb.httpRouter(
  new Router().post('/webhook', webhook)
);
