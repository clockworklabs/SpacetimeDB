import { remoteModule, schema, table, t } from './index';
import type { OutboxTableOpts } from '../lib/table';

const gwModule: unique symbol = Symbol();
const authModule: unique symbol = Symbol();
const gw = remoteModule(
  {
    receivePing: {
      name: 'receive_ping',
      signatureHash: 1n,
      params: { note: t.string() },
    },
  },
  gwModule
);
const auth = remoteModule(
  {
    receivePing: {
      name: 'receive_ping',
      signatureHash: 1n,
      params: { note: t.string() },
    },
  },
  authModule
);
const outbox = gw.outbox.receivePing({ name: 'pings' });
// @ts-expect-error As with regular tables, the schema key defines the accessor.
gw.outbox.receivePing({ accessor: 'pings' });
const explicit = table(
  { outbox: { receiver: gw.reducers.receivePing } },
  {
    id: t.u64().primaryKey().autoInc(),
    target: gw.identity().target(),
    note: t.string().index('btree'),
  }
);
table(
  {
    outbox: {
      // @ts-expect-error Local receiver functions are not supported; use generated bindings.
      receiver: () => gw.reducers.receivePing,
    },
  },
  explicit.rowType.row
);
const ordinary = table({}, { id: t.u64().primaryKey(), note: t.string() });
const options: OutboxTableOpts<typeof explicit.rowType.row> = {
  outbox: { receiver: gw.reducers.receivePing },
};
const widened = table(options, explicit.rowType.row);
const absent = table({ outbox: undefined }, explicit.rowType.row);
const db = schema({ outbox, explicit, ordinary, widened, absent });
db.reducer({ to: gw.identity(), note: t.string() }, (ctx, { to, note }) => {
  ctx.db.outbox.insert({ msgId: 0n, target: to, note });
  ctx.db.outbox.msgId.find(1n);
  ctx.db.explicit.note.filter('hello');
  // @ts-expect-error Generated arguments preserve their types.
  ctx.db.outbox.insert({ msgId: 0n, target: to, note: 123 });
  ctx.db.ordinary.id.update({ id: 1n, note });
  ctx.db.ordinary.id.delete(1n);
  ctx.db.ordinary.delete({ id: 1n, note });
  ctx.db.ordinary.clear();
  ctx.db.absent.id.update({ id: 1n, target: to, note });
  ctx.db.absent.clear();
  ctx.db.widened.insert({ id: 0n, target: to, note });
  ctx.db.widened.id.find(1n);
  ctx.db.widened.note.filter(note);
});
db.reducer(
  { reactive: { onResult: outbox } },
  { row: t.rowOf(outbox), result: t.result(t.unit(), t.string()) },
  (_ctx, { row, result }) => {
    const note: string = row.note;
    if ('err' in result) {
      const error: string = result.err;
      console.log(note, error);
    }
  }
);

db.reducer(
  { to: gw.identity(), other: auth.identity(), raw: t.identity() },
  (ctx, { to, other, raw }) => {
    ctx.db.outbox.insert({ msgId: 0n, target: to, note: 'valid' });
    // @ts-expect-error A different dependency's identity is incompatible.
    ctx.db.outbox.insert({ msgId: 0n, target: other, note: 'wrong module' });
    // @ts-expect-error A raw identity must not bypass receiver typing.
    ctx.db.outbox.insert({ msgId: 0n, target: raw, note: 'untyped' });
  }
);
