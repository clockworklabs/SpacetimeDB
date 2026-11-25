import { schema, table, t, SenderError } from 'spacetimedb/server';

const details = t.object('PingDetails', {
  labels: t.array(t.string()),
  checkpoints: t.array(
    t.object('PingCheckpoint', {
      sequence: t.u32(),
      state: t.enum('PingState', {
        Ready: t.unit(),
        Deferred: t.option(t.string()),
      }),
    })
  ),
  note: t.option(t.string()),
});
const priority = t.enum('PingPriority', { Low: t.unit(), High: t.unit() });
const spacetimedb = schema({
  pingLog: table(
    {},
    {
      id: t.u64().primaryKey().autoInc(),
      sender: t.identity(),
      moduleVersion: t.u32(),
      body: t.string(),
    }
  ),
  detailedPingLog: table(
    {},
    {
      id: t.u64().primaryKey().autoInc(),
      body: t.string(),
      details,
    }
  ),
  priorityLog: table(
    {},
    {
      id: t.u64().primaryKey().autoInc(),
      priority,
    }
  ),
});
export default spacetimedb;
export const receivePing = spacetimedb.reducer(
  { name: 'Receive_Ping' },
  { body: t.string() },
  (ctx, { body }) => {
    if (body === 'reject') throw new SenderError('rejected');
    ctx.db.pingLog.insert({
      id: 0n,
      sender: ctx.sender,
      moduleVersion: 1,
      body,
    });
  }
);
// Same signature, different canonical-name casing. IDC must not dispatch here
// when the sender's bindings specify Receive_Ping.
export const receiveLowercasePing = spacetimedb.reducer(
  { name: 'receive_ping' },
  { body: t.string() },
  (ctx, { body }) => {
    ctx.db.pingLog.insert({
      id: 0n,
      sender: ctx.sender,
      moduleVersion: 1,
      body: `wrong-case:${body}`,
    });
  }
);
export const receiveDetailedPing = spacetimedb.reducer(
  { name: 'Receive_DetailedPing' },
  { body: t.string(), details },
  (ctx, args) => {
    ctx.db.detailedPingLog.insert({ id: 0n, ...args });
  }
);
export const receivePriority = spacetimedb.reducer(
  { priority },
  (ctx, args) => {
    ctx.db.priorityLog.insert({ id: 0n, ...args });
  }
);
