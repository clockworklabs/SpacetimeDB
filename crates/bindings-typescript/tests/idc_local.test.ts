import { describe, expect, it } from 'vitest';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { schema } from '../src/server/schema';

function localOutbox() {
  return table(
    { outbox: 'local' },
    {
      msgId: t.u64().primaryKey().autoInc(),
      payload: t.string().param({ name: 'body' }),
      target: t.identity().target(),
      count: t.u32(),
    }
  );
}

describe('local IDC outboxes', () => {
  it('sends the receiver source name and maps parameters in reducer order', () => {
    const outbox = localOutbox();
    const db = schema({ outbox });
    // The explicit canonical name does not change the source name the host resolves.
    const receive = db.reducer(
      { name: 'WireReceive', receiverFor: outbox },
      { count: t.u32(), body: t.string() },
      () => {}
    );
    const onResult = db.reducer(
      { onResult: outbox },
      { row: outbox.rowType, result: t.result(t.unit(), t.string()) },
      () => {}
    );
    const raw = db.buildRawModuleDefV10({ receive, onResult });
    expect(raw.sections.find(s => s.tag === 'LocalOutboxes')?.value).toEqual([
      {
        tableName: 'outbox',
        reducerSourceName: 'receive',
        targetColumn: 2,
        argColumns: [3, 1],
        onResultReducer: 'onResult',
      },
    ]);
    expect(raw.sections.find(s => s.tag === 'Outboxes')?.value).toEqual([]);
  });

  it('omits the local outbox section from modules without one', () => {
    const raw = schema({
      ordinary: table({}, { id: t.u64().primaryKey() }),
    }).buildRawModuleDefV10({});
    expect(raw.sections.some(s => s.tag === 'LocalOutboxes')).toBe(false);
  });

  it('supports receivers with no parameters', () => {
    const outbox = table(
      { outbox: 'local' },
      {
        msgId: t.u64().primaryKey().autoInc(),
        target: t.identity().target(),
      }
    );
    const db = schema({ outbox });
    const noop = db.reducer({ name: 'noop', receiverFor: outbox }, () => {});
    const raw = db.buildRawModuleDefV10({ noop });
    expect(
      raw.sections.find(s => s.tag === 'LocalOutboxes')?.value[0].argColumns
    ).toEqual([]);
  });

  it('requires exactly one receiver for each local outbox', () => {
    const outbox = localOutbox();
    expect(() => schema({ outbox }).buildRawModuleDefV10({})).toThrow(
      'Local outbox outbox needs a reducer declared with receiverFor'
    );

    const db = schema({ outbox });
    const params = { body: t.string(), count: t.u32() };
    const first = db.reducer({ receiverFor: outbox }, params, () => {});
    const second = db.reducer({ receiverFor: outbox }, params, () => {});
    expect(() => db.buildRawModuleDefV10({ first, second })).toThrow(
      'more than one receiverFor reducer'
    );
  });

  it('rejects receiver parameters without a matching column', () => {
    const outbox = localOutbox();
    const missing = schema({ outbox });
    const receive = missing.reducer(
      { receiverFor: outbox },
      { body: t.string(), count: t.u32(), extra: t.bool() },
      () => {}
    );
    expect(() => missing.buildRawModuleDefV10({ receive })).toThrow(
      "exactly one column for receiver parameter 'extra'"
    );

    const wrongType = schema({ outbox });
    const typed = wrongType.reducer(
      { receiverFor: outbox },
      { body: t.string(), count: t.u64() },
      () => {}
    );
    expect(() => wrongType.buildRawModuleDefV10({ typed })).toThrow(
      "Outbox column for 'count' has the wrong type"
    );
  });

  it('rejects a receiverFor table that is not a local outbox', () => {
    const ordinary = table({}, { id: t.u64().primaryKey() });
    const db = schema({ ordinary });
    const receive = db.reducer(
      // @ts-expect-error receiverFor requires a local outbox table.
      { receiverFor: ordinary },
      { id: t.u64() },
      () => {}
    );
    expect(() => db.buildRawModuleDefV10({ receive })).toThrow(
      "receiverFor requires a table declared with outbox: 'local'"
    );
  });

  it('rejects a lifecycle receiver', () => {
    const outbox = localOutbox();
    const db = schema({ outbox });
    const init = db.init({ name: 'init', receiverFor: outbox }, () => {});
    expect(() => db.buildRawModuleDefV10({ init })).toThrow(
      'A lifecycle reducer cannot receive from an outbox'
    );
  });

  it('checks the outbox shape when the table is declared', () => {
    expect(() =>
      table(
        { outbox: 'local' },
        { id: t.u64().primaryKey(), target: t.identity().target() }
      )
    ).toThrow('u64 primary key with autoInc()');
    expect(() =>
      table(
        { outbox: 'local' },
        { msgId: t.u64().primaryKey().autoInc(), target: t.identity() }
      )
    ).toThrow('exactly one identity().target() column');
  });
});
