import { describe, expect, it } from 'vitest';
import { remoteModule } from '../src/lib/remote_module';
import { table, type OutboxTable, type Table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { schema } from '../src/server/schema';
import { makeTableView } from '../src/server/runtime';

const gw = remoteModule(
  {
    receivePing: {
      name: 'wire_ping',
      signatureHash: 123n,
      params: {
        note: t.string(),
        details: t.object('Details', { labels: t.array(t.string()) }),
      },
    },
  },
  Symbol()
);

function explicitOutbox() {
  return table(
    {
      outbox: { receiver: gw.reducers.receivePing },
      indexes: [
        { accessor: 'byNote', algorithm: 'btree', columns: ['note'] },
        { accessor: 'byNoteHash', algorithm: 'hash', columns: ['note'] },
      ],
    },
    {
      id: t.u64().primaryKey().autoInc(),
      details: gw.reducers.receivePing.params.details,
      target: gw.identity().target(),
      note: t.string(),
    }
  );
}

describe('IDC outboxes', () => {
  it('allows parameter overrides on metadata columns without sending bookkeeping columns', () => {
    const receiver = remoteModule(
      {
        receive: {
          name: 'receive',
          signatureHash: 1n,
          params: { messageId: t.u64(), destination: t.identity() },
        },
      },
      Symbol()
    );
    const outbox = table(
      { outbox: { receiver: receiver.reducers.receive } },
      {
        trace: t.string(),
        id: t.u64().primaryKey().autoInc().param({ name: 'messageId' }),
        target: receiver.identity().target().param({ name: 'destination' }),
      }
    );
    expect(outbox.outbox?.argColumns).toEqual([1, 2]);
    expect(Object.keys(outbox.rowType.row)).toEqual(['trace', 'id', 'target']);
  });

  it('rejects an override colliding with another column matching the same parameter', () => {
    const row = explicitOutbox().rowType.row;
    expect(() =>
      table(
        { outbox: { receiver: gw.reducers.receivePing } },
        {
          ...row,
          secondNote: t.string().param({ name: 'note' }),
        }
      )
    ).toThrow("exactly one column for receiver parameter 'note'");
  });

  it('supports reducers with no parameters', () => {
    const receiver = remoteModule(
      {
        noop: { name: 'noop', params: {}, signatureHash: 2n },
      },
      Symbol()
    );
    expect(receiver.outbox.noop().outbox?.argColumns).toEqual([]);
  });

  it('rejects targets from another dependency and untyped identities', () => {
    const auth = remoteModule(
      { receivePing: gw.reducers.receivePing },
      Symbol()
    );
    const row = explicitOutbox().rowType.row;
    for (const target of [auth.identity().target(), t.identity().target()]) {
      expect(() =>
        table(
          { outbox: { receiver: gw.reducers.receivePing } },
          { ...row, target }
        )
      ).toThrow('selected receiver module identity');
    }
    expect(() => auth.outbox.receivePing()).not.toThrow();
    // Column metadata operations preserve the receiver identity builder.
    expect(() =>
      table(
        { outbox: { receiver: gw.reducers.receivePing } },
        { ...row, target: gw.identity().name('destination').target() }
      )
    ).not.toThrow();
  });

  it('preserves a remote zero signature hash', () => {
    const receiver = remoteModule(
      {
        receivePing: { ...gw.reducers.receivePing, signatureHash: 0n },
      },
      Symbol()
    );
    const outbox = receiver.outbox.receivePing();
    const raw = schema({ outbox }).buildRawModuleDefV10({});
    const section = raw.sections.find(s => s.tag === 'Outboxes');
    expect(section?.value[0].signatureHash).toBe(0n);
    expect(section?.value[0].remoteReducer).toBe('wire_ping');
  });

  it('maps receiver parameters by name and records the result callback', () => {
    const outbox = explicitOutbox();
    const db = schema({ outbox });
    const onResult = db.reducer(
      { name: 'wire_callback', reactive: { onResult: outbox } },
      { row: outbox.rowType, result: t.result(t.unit(), t.string()) },
      () => {}
    );
    const raw = db.buildRawModuleDefV10({ onResult });
    expect(raw.sections.find(s => s.tag === 'Outboxes')?.value).toEqual([
      {
        tableName: 'outbox',
        remoteReducer: 'wire_ping',
        signatureHash: 123n,
        targetColumn: 2,
        argColumns: [3, 1],
        onResultReducer: 'onResult',
      },
    ]);
  });

  it('keeps convenience outbox metadata separate from msgId and target parameters', () => {
    const receiver = remoteModule(
      {
        receive: {
          name: 'receive',
          signatureHash: 1n,
          params: { msgId: t.u64(), target: t.string() },
        },
      },
      Symbol()
    );
    const outbox = receiver.outbox.receive();
    expect(Object.keys(outbox.rowType.row)).toEqual([
      '__msgId',
      '__target',
      'msgId',
      'target',
    ]);
    expect(outbox.outbox?.targetColumn).toBe(1);
    expect(outbox.outbox?.argColumns).toEqual([2, 3]);
  });

  it('builds the convenience outbox with receiver argument types', () => {
    const outbox = gw.outbox.receivePing({ name: 'pings' });
    expect(Object.keys(outbox.rowType.row)).toEqual([
      '__msgId',
      '__target',
      'note',
      'details',
    ]);
    expect(outbox.outbox?.argColumns).toEqual([2, 3]);
    expect(outbox.isOutbox).toBe(true);
  });

  it('matches a sender property when its database column has another name', () => {
    const receiver = remoteModule(
      {
        receiveMessage: {
          name: 'receive_message',
          signatureHash: 456n,
          params: { messageText: t.string() },
        },
      },
      Symbol()
    );
    const outbox = table(
      { outbox: { receiver: receiver.reducers.receiveMessage } },
      {
        id: t.u64().primaryKey().autoInc(),
        target: receiver.identity().target(),
        messageText: t.string().name('legacy_message'),
      }
    );
    expect(outbox.outbox?.argColumns).toEqual([2]);
    expect(outbox.rowType.row.messageText.columnMetadata.name).toBe(
      'legacy_message'
    );

    const aliased = table(
      { outbox: { receiver: receiver.reducers.receiveMessage } },
      {
        id: t.u64().primaryKey().autoInc(),
        target: receiver.identity().target(),
        body: t.string().name('legacy_message').param({ name: 'messageText' }),
      }
    );
    expect(aliased.outbox?.argColumns).toEqual([2]);
    expect(aliased.rowType.row.body.columnMetadata.name).toBe('legacy_message');
    expect(aliased.rowType.row.body.columnMetadata.paramName).toBe(
      'messageText'
    );

    const directAlias = table(
      { outbox: { receiver: receiver.reducers.receiveMessage } },
      {
        id: t.u64().primaryKey().autoInc(),
        target: receiver.identity().target(),
        body: t.string().param({ name: 'messageText' }),
      }
    );
    expect(directAlias.outbox?.argColumns).toEqual([2]);

    expect(() =>
      table(
        { outbox: { receiver: receiver.reducers.receiveMessage } },
        {
          id: t.u64().primaryKey().autoInc(),
          target: receiver.identity().target(),
          body: t.string().name('message_text'),
        }
      )
    ).toThrow("parameter 'messageText'");
  });

  it('does not expose mutation methods at runtime, including indexed deletes', () => {
    const outbox = explicitOutbox();
    const db = schema({ outbox });
    const view = makeTableView(
      db.typespace,
      db.schemaType.tables.outbox.tableDef,
      '',
      '',
      true
    ) as OutboxTable<typeof db.schemaType.tables.outbox>;
    expect(view.insert).toBeTypeOf('function');
    expect(view.iter).toBeTypeOf('function');
    expect('delete' in view).toBe(false);
    expect('clear' in view).toBe(false);
    for (const index of [view.id, view.byNote, view.byNoteHash]) {
      expect('delete' in index).toBe(false);
      expect('update' in index).toBe(false);
    }
    expect(view.id.find).toBeTypeOf('function');
    expect(view.byNote.filter).toBeTypeOf('function');
  });

  it('preserves ordinary table mutation methods without outbox options', () => {
    const ordinary = table(
      {
        indexes: [
          { accessor: 'byNote', algorithm: 'btree', columns: ['note'] },
          { accessor: 'byNoteHash', algorithm: 'hash', columns: ['note'] },
        ],
      },
      { id: t.u64().primaryKey(), note: t.string() }
    );
    const db = schema({ ordinary });
    const view = makeTableView(
      db.typespace,
      db.schemaType.tables.ordinary.tableDef
    ) as Table<typeof db.schemaType.tables.ordinary>;
    expect(view.delete).toBeTypeOf('function');
    expect(view.clear).toBeTypeOf('function');
    expect(view.id.update).toBeTypeOf('function');
    for (const index of [view.id, view.byNote, view.byNoteHash]) {
      expect(index.delete).toBeTypeOf('function');
    }
  });

  it('rejects malformed table shapes and receiver arguments', () => {
    const row = explicitOutbox().rowType.row;
    const opts = { outbox: { receiver: gw.reducers.receivePing } };
    expect(() =>
      table(opts, { ...row, id: t.u32().primaryKey().autoInc() })
    ).toThrow('u64 primary key');
    expect(() => table(opts, { ...row, target: t.identity() })).toThrow(
      'exactly one'
    );
    expect(() =>
      table(opts, { ...row, secondTarget: t.identity().target() })
    ).toThrow('exactly one');
    expect(() => table(opts, { ...row, note: t.u32() })).toThrow('wrong type');
    const { note: _, ...missing } = row;
    expect(() => table(opts, missing)).toThrow("parameter 'note'");
    expect(() => table({ ...opts, event: true }, row)).toThrow('event tables');
  });

  it('rejects wrong callback parameters and callbacks for another schema', () => {
    const outbox = explicitOutbox();
    const db = schema({ outbox });
    const bad = db.reducer(
      { reactive: { onResult: outbox } },
      { row: t.string() },
      () => {}
    );
    expect(() => db.buildRawModuleDefV10({ bad })).toThrow(
      'outbox row followed by'
    );
    const other = schema({});
    const callback = other.reducer(
      { reactive: { onResult: outbox } },
      { row: outbox.rowType, result: t.result(t.unit(), t.string()) },
      () => {}
    );
    expect(() => other.buildRawModuleDefV10({ callback })).toThrow(
      'registered exactly once'
    );
  });

  it('rejects a callback row from a different table with the same shape', () => {
    const outbox = explicitOutbox();
    const other = explicitOutbox();
    const db = schema({ outbox, other });
    const bad = db.reducer(
      { reactive: { onResult: outbox } },
      { row: other.rowType, result: t.result(t.unit(), t.string()) },
      () => {}
    );
    expect(() => db.buildRawModuleDefV10({ bad })).toThrow(
      'outbox row followed by'
    );
  });

  it('rejects multiple callbacks for one outbox', () => {
    const outbox = explicitOutbox();
    const db = schema({ outbox });
    const opts = { reactive: { onResult: outbox } };
    const params = {
      row: outbox.rowType,
      result: t.result(t.unit(), t.string()),
    };
    const first = db.reducer(opts, params, () => {});
    const second = db.reducer(opts, params, () => {});
    expect(() => db.buildRawModuleDefV10({ first, second })).toThrow(
      'only have one'
    );
  });
});
