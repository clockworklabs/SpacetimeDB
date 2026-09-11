import { describe, expect, test } from 'vitest';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { DbConnectionImpl } from '../src/sdk/db_connection_impl';
import { EventEmitter } from '../src/sdk/event_emitter';
import type { EventContextInterface } from '../src/sdk/event_context';
import { TableCacheImpl, type Operation } from '../src/sdk/table_cache';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';

const schema = tablesToSchema(new ModuleContext(), {
  item: table({ name: 'item' }, { value: t.string() }),
});
const remoteModule = {
  ...schema,
  reducers: [],
  procedures: [],
  versionInfo: { cliVersion: '2.8.3' },
};

describe('reconciliation without primary keys', () => {
  test('preserves unchanged rows and adjusts overlapping subscription references', () => {
    const connection = new DbConnectionImpl({
      uri: new URL('ws://localhost'),
      nameOrAddress: 'test',
      emitter: new EventEmitter(),
      remoteModule,
      createWSFn: new WebsocketTestAdapterFactory().openWebSocket,
      compression: 'none',
      lightMode: false,
    });
    const cache = new TableCacheImpl<typeof remoteModule, 'item'>(
      schema.tables.item
    );
    const ctx: EventContextInterface<typeof remoteModule> = {
      db: connection.db,
      reducers: connection.reducers,
      isActive: true,
      subscriptionBuilder: () => connection.subscriptionBuilder(),
      disconnect: () => connection.disconnect(),
      event: { id: 'test', tag: 'SubscribeApplied' },
    };
    const insert: Operation<{ value: string }> = {
      type: 'insert',
      rowId: 'row-bytes',
      row: { value: 'same' },
    };
    cache.applyOperations([insert, insert], ctx);
    const callbacks = cache.applyOperations(
      [...cache.snapshotDeleteOperations(), insert],
      ctx,
      { skipIdenticalUpdates: true }
    );
    expect(callbacks).toEqual([]);
    expect(cache.count()).toBe(1n);
    expect(cache.snapshotDeleteOperations()).toHaveLength(1);
    const deletes = cache.applyOperations([{ ...insert, type: 'delete' }], ctx);
    expect(deletes.map(callback => callback.type)).toEqual(['delete']);
    expect(cache.count()).toBe(0n);
    connection.disconnect();
  });
});
