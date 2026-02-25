/**
 * Regression test for #4433: wire protocol uses canonical table names,
 * but the SDK lookup map was keyed by accessor names. When they differ
 * (e.g., accessor "reports" vs. wire name "report"), the lookup fails.
 */
import { describe, expect, test } from 'vitest';
import { BinaryWriter, ConnectionId, Identity, type Infer } from '../src';
import { ServerMessage } from '../src/sdk/client_api/types';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';
import { DbConnection } from '../test-wire-name-app/src/module_bindings';
import ReportsRow from '../test-wire-name-app/src/module_bindings/reports_table';
import CategoriesRow from '../test-wire-name-app/src/module_bindings/categories_table';
import { makeQuerySetUpdate } from './utils';

const testIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);

function encodeReport(value: Infer<typeof ReportsRow>): Uint8Array {
  const writer = new BinaryWriter(1024);
  ReportsRow.serialize(writer, value);
  return writer.getBuffer();
}

function encodeCategory(value: Infer<typeof CategoriesRow>): Uint8Array {
  const writer = new BinaryWriter(1024);
  CategoriesRow.serialize(writer, value);
  return writer.getBuffer();
}

class Deferred<T> {
  #resolve: (value: T) => void = () => {};
  #reject: (reason?: any) => void = () => {};
  promise: Promise<T>;
  constructor() {
    this.promise = new Promise<T>((resolve, reject) => {
      this.#resolve = resolve;
      this.#reject = reject;
    });
  }
  resolve(value: T) {
    this.#resolve(value);
  }
  reject(reason?: any) {
    this.#reject(reason);
  }
}

async function setupConnection() {
  const wsAdapter = new WebsocketTestAdapter();
  const onConnectDeferred = new Deferred<DbConnection>();
  DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName('db')
    .withWSFn(wsAdapter.createWebSocketFn.bind(wsAdapter) as any)
    .onConnect(ctx => onConnectDeferred.resolve(ctx))
    .build();

  // Wait for ws to be ready
  await new Promise<void>(resolve => setTimeout(resolve, 50));
  wsAdapter.acceptConnection();

  wsAdapter.sendToClient(
    ServerMessage.InitialConnection({
      identity: testIdentity,
      token: 'test-token',
      connectionId: ConnectionId.random(),
    })
  );

  const client = await Promise.race([
    onConnectDeferred.promise,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('Connection timeout')), 2000)
    ),
  ]);
  return { client, wsAdapter };
}

describe('Wire name lookup (#4433)', () => {
  test('table with accessor "reports" receives rows sent under canonical wire name "report"', async () => {
    const { client, wsAdapter } = await setupConnection();

    const insertDeferred = new Deferred<Infer<typeof ReportsRow>>();
    client.db.reports.onInsert((_ctx, row) => {
      insertDeferred.resolve(row);
    });

    // The wire protocol sends the CANONICAL table name "report",
    // not the accessor name "reports". Before the fix, this would crash
    // with: Cannot read properties of undefined (reading 'columns')
    const update = makeQuerySetUpdate(
      0,
      'report', // canonical wire name — differs from accessor "reports"
      encodeReport({ id: 1, title: 'Bug Report', body: 'Something broke' })
    );

    wsAdapter.sendToClient(
      ServerMessage.TransactionUpdate({
        querySets: [update],
      })
    );

    const row = await Promise.race([
      insertDeferred.promise,
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error('Timeout — onInsert not called')),
          2000
        )
      ),
    ]);

    expect(row.id).toBe(1);
    expect(row.title).toBe('Bug Report');
    expect(row.body).toBe('Something broke');
  });

  test('table with accessor "categories" receives rows sent under canonical wire name "report_category"', async () => {
    const { client, wsAdapter } = await setupConnection();

    const insertDeferred = new Deferred<Infer<typeof CategoriesRow>>();
    client.db.categories.onInsert((_ctx, row) => {
      insertDeferred.resolve(row);
    });

    const update = makeQuerySetUpdate(
      0,
      'report_category', // canonical wire name — differs from accessor "categories"
      encodeCategory({ id: 1, name: 'Critical', reportId: 42 })
    );

    wsAdapter.sendToClient(
      ServerMessage.TransactionUpdate({
        querySets: [update],
      })
    );

    const row = await Promise.race([
      insertDeferred.promise,
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error('Timeout — onInsert not called')),
          2000
        )
      ),
    ]);

    expect(row.id).toBe(1);
    expect(row.name).toBe('Critical');
    expect(row.reportId).toBe(42);
  });
});
