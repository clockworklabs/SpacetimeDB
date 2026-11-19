import { describe, expect, test, vi } from 'vitest';
import BinaryReader from '../src/lib/binary_reader';
import BinaryWriter from '../src/lib/binary_writer';
import { Timestamp } from '../src/lib/timestamp';
import { ServerMessage } from '../src/sdk/client_api/types';
import { makeQuerySetUpdate } from './utils';
import { schema, table, t } from '../src/schema/module';
import { createClient } from '../src/sdk/create_client';
import { INTERNAL_REMOTE_MODULE } from '../src/sdk/internal';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';

const Person = t.row({
  id: t.u32().primaryKey().autoInc(),
  name: t.string().name('full_name'),
  age: t.u8().default(18),
});
const people = table(
  {
    name: 'people',
    public: true,
    indexes: [{ accessor: 'byAge', algorithm: 'btree', columns: ['age'] }],
  },
  Person
);
const childSchema = schema({ people });
const child = {
  default: childSchema,
  add: childSchema.reducer({ name: 'insert_person' }, { name: t.string() }),
};
const root = schema({ people, auth: { name: 'auth_wire', module: child } });
const moduleSchema = {
  default: root,
  add: root.reducer({ name: 'add_person' }, { name: t.string() }),
  sayHello: root.reducer({}),
  greet: root.procedure({ name: t.string() }, t.string()),
  names: root.anonymousView(
    { public: true },
    t.array(t.row({ name: t.string() }))
  ),
  named: root.view({ public: true }, { name: t.string() }, t.array(Person)),
  me: root.view({ public: true }, t.option(Person)),
  Person,
  actions: root.exportGroup({ default: root.reducer({}) }),
};

const client = createClient(moduleSchema, {
  tableAliases: { people_legacy: 'people' },
});

describe('module schemas', () => {
  test('retains table and function metadata and produces the typed client API', async () => {
    expect(moduleSchema.add.params.name.algebraicType.tag).toBe('String');
    expect(moduleSchema.greet.returnType.algebraicType.tag).toBe('String');
    expect(people.rowType.row.id.columnMetadata.isAutoIncrement).toBe(true);
    expect(people.rowType.row.age.columnMetadata.defaultValue).toBe(18);
    expect(client.tables.people_legacy).toBe(client.tables.people);
    expect(client.tables.people.toSql()).toBe('SELECT * FROM "people"');
    expect(client.tables.auth.people.toSql()).toBe(
      'SELECT * FROM "auth_wire"."people"'
    );
    expect(client.reducers.add.name).toBe('add_person');
    expect(client.reducers.auth.add.name).toBe('auth_wire.insert_person');

    const ws = new WebsocketTestAdapter();
    const conn = client.DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withDatabaseName('example')
      .withWSFn(ws.openWebSocket)
      .build();
    expect(conn).toBeInstanceOf(client.DbConnection);
    expect(conn.db.people_legacy).toBe(conn.db.people);
    const module = conn[INTERNAL_REMOTE_MODULE]();
    expect(module.viewsWithArgs).toEqual(['named']);
    expect(Object.keys(module.tables)).toEqual([
      'people',
      'auth.people',
      'names',
      'named',
      'me',
    ]);
    expect(module.tables.people.sourceName).toBe('people');
    expect(module.tables.people.columns.name.columnMetadata.name).toBe(
      'full_name'
    );
    await conn['wsPromise'];
    ws.acceptConnection();
    const inserted: string[] = [];
    conn.db.people.onInsert((ctx, row) => {
      expect(ctx.db.people).toBe(conn.db.people);
      expect(ctx.db.people_legacy).toBe(conn.db.people);
      inserted.push(row.name);
    });
    const writer = new BinaryWriter(64);
    Person.serialize(writer, { id: 1, name: 'Alice', age: 30 });
    ws.sendToClient(
      ServerMessage.TransactionUpdate({
        querySets: [makeQuerySetUpdate(0, 'people', writer.getBuffer())],
      })
    );
    await vi.waitFor(() => expect(inserted).toEqual(['Alice']));
    expect(conn.db.people.id.find(1)?.name).toBe('Alice');

    const call = conn.reducers.auth.add({ name: 'Bob' });
    await Promise.resolve();
    const request = ws.outgoingMessages.find(m => m.tag === 'CallReducer');
    if (request?.tag !== 'CallReducer')
      throw new Error('Expected a reducer request');
    expect(request.value.reducer).toBe('auth_wire.insert_person');
    expect(new BinaryReader(request.value.args).readString()).toBe('Bob');
    ws.sendToClient(
      ServerMessage.ReducerResult({
        requestId: request.value.requestId,
        timestamp: new Timestamp(0n),
        result: {
          tag: 'Ok',
          value: {
            retValue: new Uint8Array(),
            transactionUpdate: { querySets: [] },
          },
        },
      })
    );
    await call;
    conn.disconnect();
  });

  test('rejects functions exported from a different schema', () => {
    expect(() => {
      createClient({ default: schema({}), add: moduleSchema.add });
    }).toThrow('another schema');
  });
});
