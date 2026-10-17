import { remoteModule, schema, table, t } from './index';

const gw = remoteModule({
  receivePing: {
    name: 'receive_ping',
    signatureHash: 1n,
    params: { note: t.string() },
    paramNames: { note: 'note' },
  },
});
const outbox = gw.outbox.receivePing({ name: 'pings' });
const explicit = table(
  { outbox: { receiver: gw.reducers.receivePing } },
  {
    id: t.u64().primaryKey().autoInc(),
    target: gw.identity().target(),
    note: t.string().index('btree'),
  }
);
const ordinary = table({}, { id: t.u64().primaryKey(), note: t.string() });
const db = schema({ outbox, explicit, ordinary });
db.reducer({ to: gw.identity(), note: t.string() }, (ctx, { to, note }) => {
  ctx.db.outbox.insert({ msgId: 0n, target: to, note });
  ctx.db.outbox.msgId.find(1n);
  ctx.db.explicit.note.filter('hello');
  // @ts-expect-error Outboxes cannot be updated.
  ctx.db.outbox.msgId.update({ msgId: 1n, target: to, note });
  // @ts-expect-error Outboxes cannot be deleted by index.
  ctx.db.outbox.msgId.delete(1n);
  // @ts-expect-error Outboxes cannot be deleted by range.
  ctx.db.explicit.note.delete('hello');
  // @ts-expect-error Outbox rows cannot be deleted.
  ctx.db.outbox.delete({ msgId: 1n, target: to, note });
  // @ts-expect-error Outboxes cannot be cleared.
  ctx.db.explicit.clear();
  // @ts-expect-error Generated arguments preserve their types.
  ctx.db.outbox.insert({ msgId: 0n, target: to, note: 123 });
  ctx.db.ordinary.id.update({ id: 1n, note });
  ctx.db.ordinary.id.delete(1n);
  ctx.db.ordinary.delete({ id: 1n, note });
  ctx.db.ordinary.clear();
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
