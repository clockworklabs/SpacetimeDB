import { describe, expect, it } from 'vitest';
import { remoteModule } from '../src/lib/remote_module';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { schema } from '../src/server/schema';

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
      { row: t.rowOf(outbox), result: t.result(t.unit(), t.string()) },
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
      { row: t.rowOf(outbox), result: t.result(t.unit(), t.string()) },
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
      { row: t.rowOf(other), result: t.result(t.unit(), t.string()) },
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
      row: t.rowOf(outbox),
      result: t.result(t.unit(), t.string()),
    };
    const first = db.reducer(opts, params, () => {});
    const second = db.reducer(opts, params, () => {});
    expect(() => db.buildRawModuleDefV10({ first, second })).toThrow(
      'only have one'
    );
  });
});
