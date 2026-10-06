import { describe, expect, test } from 'vitest';
import { BinaryWriter } from '../src';
import {
  RowSizeHint,
  ServerMessage,
  TableUpdateRows,
} from '../src/sdk/client_api/types';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import { DbConnection } from '../test-app/src/module_bindings';
import { bobIdentity, sallyIdentity } from './utils';

// A `user` row as a newer module sends it: the columns these bindings know,
// plus a column the module added to the end of the table.
function encodeUserWithAddedColumn(
  identity: typeof bobIdentity,
  username: string,
  added: number
): Uint8Array {
  const writer = new BinaryWriter(1024);
  writer.writeU256(identity.__identity__);
  writer.writeString(username);
  writer.writeU32(added);
  return writer.getBuffer();
}

async function connectedClient() {
  const wsAdapter = new WebsocketTestAdapter();
  const client = DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName('db')
    .withWSFn(wsAdapter.openWebSocket)
    .build();
  await client['wsPromise'];
  wsAdapter.acceptConnection();
  return { wsAdapter, client };
}

function insertUsers(rows: Uint8Array[], sizeHint: RowSizeHint) {
  return ServerMessage.TransactionUpdate({
    querySets: [
      {
        querySetId: { id: 0 },
        tables: [
          {
            tableName: 'user',
            rows: [
              TableUpdateRows.PersistentTable({
                inserts: {
                  sizeHint,
                  rowsData: new Uint8Array(rows.flatMap(row => [...row])),
                },
                deletes: {
                  sizeHint: RowSizeHint.RowOffsets([]),
                  rowsData: new Uint8Array(),
                },
              }),
            ],
          },
        ],
      },
    ],
  });
}

async function receiveUsers(rows: Uint8Array[], sizeHint: RowSizeHint) {
  const { wsAdapter, client } = await connectedClient();
  const inserted: { username: string }[] = [];
  const gotAll = new Promise<void>(resolve => {
    client.db.user.onInsert((_ctx, user) => {
      inserted.push(user);
      if (inserted.length === rows.length) resolve();
    });
  });
  wsAdapter.sendToClient(insertUsers(rows, sizeHint));
  await Promise.race([
    gotAll,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error('Timeout')), 1000)
    ),
  ]);
  return { client, inserted };
}

describe('rows from a module with an added column', () => {
  test('are read with variable-size rows (row offsets)', async () => {
    const bob = encodeUserWithAddedColumn(bobIdentity, 'bob', 7);
    const sally = encodeUserWithAddedColumn(sallyIdentity, 'sally', 8);

    const { client, inserted } = await receiveUsers(
      [bob, sally],
      RowSizeHint.RowOffsets([0n, BigInt(bob.length)])
    );

    expect(inserted.map(user => user.username)).toEqual(['bob', 'sally']);
    expect(client.db.user.identity.find(sallyIdentity)?.username).toEqual(
      'sally'
    );
    expect(client.db.user.count()).toEqual(2n);
  });

  test('are read with fixed-size rows', async () => {
    const bob = encodeUserWithAddedColumn(bobIdentity, 'bob', 7);
    const ann = encodeUserWithAddedColumn(sallyIdentity, 'ann', 8);
    expect(ann.length).toEqual(bob.length);

    const { inserted } = await receiveUsers(
      [bob, ann],
      RowSizeHint.FixedSize(bob.length)
    );

    expect(inserted.map(user => user.username)).toEqual(['bob', 'ann']);
  });
});
