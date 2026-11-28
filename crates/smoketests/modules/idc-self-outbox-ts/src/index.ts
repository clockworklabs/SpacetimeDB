// TypeScript port of idc-self-outbox: an outbox to a reducer in this module,
// declared without receiver bindings.
import { schema, table, t } from 'spacetimedb/server';

const State = t.object('State', {
  revision: t.u64(),
  displayName: t.option(t.string()),
});
const Operation = t.enum('Operation', {
  SetState: State,
  ClearState: t.unit(),
});
const Message = t.object('Message', {
  ItemID: t.u64(),
  operations: t.array(Operation),
});

const deliveredPing = table(
  { name: 'delivered_ping' },
  { id: t.u64().primaryKey().autoInc(), body: Message }
);
const outboundPing = table(
  { name: 'outbound_ping', outbox: 'local' },
  {
    id: t.u64().primaryKey().autoInc(),
    target: t.identity().target(),
    payload: Message.param({ name: 'messageBody' }),
  }
);
const pingResults = table(
  { name: 'ping_results' },
  {
    id: t.u64().primaryKey(),
    body: Message,
    error: t.option(t.string()),
  }
);

const spacetimedb = schema(
  { deliveredPing, outboundPing, pingResults },
  { CASE_CONVERSION_POLICY: { tag: 'None' } }
);
export default spacetimedb;

export const receivePing = spacetimedb.reducer(
  { receiverFor: outboundPing },
  { messageBody: Message },
  (ctx, { messageBody }) => {
    ctx.db.deliveredPing.insert({ id: 0n, body: messageBody });
  }
);

export const onPingResult = spacetimedb.reducer(
  { name: 'WirePingResult', onResult: outboundPing },
  { row: outboundPing.rowType, result: t.result(t.unit(), t.string()) },
  (ctx, { row, result }) => {
    ctx.db.pingResults.insert({
      id: row.id,
      body: row.payload,
      error: 'err' in result ? result.err : undefined,
    });
  }
);

export const enqueuePing = spacetimedb.reducer(
  { target: t.identity(), messageBody: Message },
  (ctx, { target, messageBody }) => {
    ctx.db.outboundPing.insert({ id: 0n, target, payload: messageBody });
  }
);
