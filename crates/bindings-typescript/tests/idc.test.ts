import { describe, expect, it } from 'vitest';
import { remoteModule } from '../src/lib/remote_module';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { schema } from '../src/server/schema';
import { makeTableView } from '../src/server/runtime';

const gw = remoteModule({
  receivePing: {
    name: 'wire_ping',
    signatureHash: 123n,
    params: {
      note: t.string(),
      details: t.object('Details', { labels: t.array(t.string()) }),
    },
    paramNames: { note: 'note', details: 'details' },
  },
});

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
        tableSourceName: 'outbox',
        remoteReducer: 'wire_ping',
        signatureHash: 123n,
        targetColumn: 2,
        argColumns: [3, 1],
        onResultReducer: 'onResult',
      },
    ]);
  });

  it('builds the convenience outbox with receiver argument types', () => {
    const outbox = gw.outbox.receivePing({ name: 'pings' });
    expect(Object.keys(outbox.rowType.row)).toEqual([
      'msgId',
      'target',
      'note',
      'details',
    ]);
    expect(outbox.outbox?.argColumns).toEqual([2, 3]);
    expect(outbox.isOutbox).toBe(true);
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
    ) as any;
    expect(view.insert).toBeTypeOf('function');
    expect(view.iter).toBeTypeOf('function');
    expect(view.delete).toBeUndefined();
    expect(view.clear).toBeUndefined();
    for (const index of [view.id, view.byNote, view.byNoteHash]) {
      expect(index.delete).toBeUndefined();
      expect(index.update).toBeUndefined();
    }
    expect(view.id.find).toBeTypeOf('function');
    expect(view.byNote.filter).toBeTypeOf('function');
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
