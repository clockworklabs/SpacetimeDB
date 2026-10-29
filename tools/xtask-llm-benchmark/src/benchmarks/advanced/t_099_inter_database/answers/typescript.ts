import { schema, table, t, SenderError } from 'spacetimedb/server';
import receiver from './receiver';
const receipt = table(
  { public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    body: t.string(),
    sender: t.identity(),
  }
);
const outgoing = table(
  { public: true, outbox: { receiver: receiver.reducers.receive } },
  {
    id: t.u64().primaryKey().autoInc(),
    target: receiver.identity().target(),
    body: t.string(),
    reject: t.bool(),
  }
);
const deliveryResult = table(
  { name: 'delivery_result', public: true },
  {
    id: t.u64().primaryKey(),
    body: t.string(),
    succeeded: t.bool(),
  }
);
const db = schema({ receipt, outgoing, deliveryResult });
export default db;
export const receive = db.reducer(
  { body: t.string(), reject: t.bool() },
  (ctx, { body, reject }) => {
    ctx.db.receipt.insert({ id: 0n, body, sender: ctx.sender });
    if (reject) throw new SenderError('receiver rejected');
  }
);
export const send_message = db.reducer(
  {
    target: receiver.identity(),
    body: t.string(),
    reject: t.bool(),
    abort: t.bool(),
  },
  (ctx, { target, body, reject, abort }) => {
    ctx.db.outgoing.insert({ id: 0n, target, body, reject });
    if (abort) throw new SenderError('sender rejected');
  }
);
export const on_delivery = db.reducer(
  { onResult: outgoing },
  { row: outgoing.rowType, result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) =>
    ctx.db.deliveryResult.insert({
      id: row.id,
      body: row.body,
      succeeded: 'ok' in result,
    })
);
