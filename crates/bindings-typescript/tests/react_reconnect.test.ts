// @vitest-environment jsdom
import { afterEach, expect, test } from 'vitest';
import { act, createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { ConnectionId } from '../src';
import { SpacetimeDBProvider } from '../src/react/SpacetimeDBProvider';
import { useSpacetimeDB } from '../src/react/useSpacetimeDB';
import type { ConnectionState } from '../src/react/connection_state';
import { ConnectionManager } from '../src/sdk/connection_manager';
import { ServerMessage } from '../src/sdk/client_api/types';
import { WebsocketTestAdapterFactory } from '../src/sdk/websocket_test_adapter';
import { DbConnection } from '../test-app/src/module_bindings';
import { anIdentity } from './utils';

(
  globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

const factory = new WebsocketTestAdapterFactory();
const builder = (token: string) =>
  DbConnection.builder()
    .withUri('ws://127.0.0.1:1234')
    .withDatabaseName('react-reconnect')
    .withWSFn(factory.openWebSocket)
    .withToken(token);
const key = ConnectionManager.getKey(
  builder('').getUri(),
  builder('').getModuleName()
);
const root = createRoot(document.createElement('div'));

afterEach(async () => {
  await act(async () => root.unmount());
});

async function settle() {
  await ConnectionManager.getConnection<DbConnection>(key)?.['wsPromise'];
}

test('reconnect(builder) connects again with the new builder token', async () => {
  let context!: ConnectionState;
  function Consumer() {
    context = useSpacetimeDB();
    return null;
  }
  await act(async () =>
    root.render(
      createElement(
        SpacetimeDBProvider,
        { connectionBuilder: builder('signed-out-token') },
        createElement(Consumer)
      )
    )
  );
  await settle();
  expect(factory.current.connectArgs?.authToken).toBe('signed-out-token');

  // The server issues its own token. A rebuild from the retained builder would
  // resume it; `reconnect` is for changing identity, so the new token wins.
  factory.current.acceptConnection();
  factory.current.sendToClient(
    ServerMessage.InitialConnection({
      identity: anIdentity,
      connectionId: ConnectionId.random(),
      token: 'server-issued-token',
    })
  );

  await act(async () => context.reconnect(builder('signed-in-token')));
  await settle();
  expect(factory.sockets).toHaveLength(2);
  expect(factory.current.connectArgs?.authToken).toBe('signed-in-token');
});
