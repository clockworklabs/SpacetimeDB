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
    receiveRevision: {
      name: 'receive_revision',
      signatureHash: 2n,
      params: { revision: t.u64() },
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
  {
    name: 'WirePings',
    outbox: { receiver: gw.reducers.receivePing },
    indexes: [
      { accessor: 'byNote', algorithm: 'btree', columns: ['note'] },
      { accessor: 'byTarget', algorithm: 'hash', columns: ['target'] },
    ],
  },
  t.row({
    id: t.u64().primaryKey().autoInc(),
    target: gw.identity().target(),
    note: t.string().index('btree'),
  })
);
const revisions = gw.outbox.receiveRevision();
table(
  {
    outbox: {
      // @ts-expect-error Each outbox targets one reducer, not a list of reducers.
      receiver: [gw.reducers.receivePing, gw.reducers.receiveRevision],
    },
  },
  explicit.rowType.row
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
const trusted = table(
  {},
  { world: gw.identity().primaryKey(), admin: auth.identity().index('btree') }
);
const db = schema({
  outbox,
  explicit,
  revisions,
  ordinary,
  widened,
  absent,
  trusted,
});
db.reducer({ to: gw.identity(), note: t.string() }, (ctx, { to, note }) => {
  ctx.db.outbox.insert({ msgId: 0n, target: to, note });
  ctx.db.outbox.msgId.find(1n);
  ctx.db.explicit.insert({ id: 0n, target: to, note });
  ctx.db.explicit.note.filter('hello');
  ctx.db.explicit.byNote.filter(note);
  ctx.db.explicit.byTarget.filter(to);
  ctx.db.revisions.insert({ msgId: 0n, target: to, revision: 1n });
  // @ts-expect-error Each reducer's outbox retains its own argument types.
  ctx.db.revisions.insert({ msgId: 0n, target: to, revision: note });
  // @ts-expect-error Explicit B-tree indexes remain read-only on outboxes.
  ctx.db.explicit.byNote.delete(note);
  // @ts-expect-error Explicit hash indexes remain read-only on outboxes.
  ctx.db.explicit.byTarget.delete(to);
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
  ctx.db.absent.id.update({ id: 1n, target: to, note });
  ctx.db.absent.clear();
  ctx.db.widened.insert({ id: 0n, target: to, note });
  ctx.db.widened.id.find(1n);
  ctx.db.widened.note.filter(note);
  // @ts-expect-error Widening options must not expose updates on an outbox.
  ctx.db.widened.id.update({ id: 1n, target: to, note });
  // @ts-expect-error Widening options must not expose index deletes.
  ctx.db.widened.id.delete(1n);
  // @ts-expect-error Widening options must not expose range deletes.
  ctx.db.widened.note.delete(note);
  // @ts-expect-error Widening options must not expose row deletes.
  ctx.db.widened.delete({ id: 1n, target: to, note });
  // @ts-expect-error Widening options must not expose clear.
  ctx.db.widened.clear();
});
db.reducer(
  { onResult: outbox },
  { row: outbox.rowType, result: t.result(t.unit(), t.string()) },
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
    ctx.db.outbox.insert({
      msgId: 0n,
      // @ts-expect-error A different dependency's identity is incompatible.
      target: other,
      note: 'wrong module',
    });
    // @ts-expect-error A raw identity must not bypass receiver typing.
    ctx.db.outbox.insert({ msgId: 0n, target: raw, note: 'untyped' });
  }
);

// Index lookups take a raw identity, so a reducer can check its caller against
// trusted receivers and reply to the stored, typed identity.
db.reducer({ note: t.string() }, (ctx, { note }) => {
  const caller = ctx.db.trusted.world.find(ctx.sender);
  if (!caller) throw new Error('unknown caller');
  ctx.db.outbox.insert({ msgId: 0n, target: caller.world, note });
  ctx.db.trusted.admin.filter(ctx.sender);
  ctx.db.trusted.admin.delete(ctx.sender);
  ctx.db.trusted.world.delete(ctx.sender);
  ctx.db.explicit.byTarget.filter(ctx.sender);
  // @ts-expect-error An unchecked caller identity is not a receiver identity.
  ctx.db.outbox.insert({ msgId: 0n, target: ctx.sender, note });
  // @ts-expect-error Rows keep the receiver's identity type.
  ctx.db.trusted.insert({ world: ctx.sender, admin: caller.admin });
  // @ts-expect-error A row's identity keeps its own receiver's type.
  ctx.db.outbox.insert({ msgId: 0n, target: caller.admin, note });
});

// Query comparisons take a raw identity too, so a view can filter by its caller.
db.view(
  { name: 'trustedCaller', public: true },
  t.array(trusted.rowType),
  ctx =>
    ctx.from.trusted
      .where(row => row.world.eq(ctx.sender).or(row.admin.ne(ctx.sender)))
      .build()
);
