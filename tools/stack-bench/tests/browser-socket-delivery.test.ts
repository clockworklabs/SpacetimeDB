import assert from 'node:assert/strict';
import test from 'node:test';
import type { BrowserContext, WebSocketRoute } from 'playwright';
import { connectBrowserSocket, installSocketDeliveryGate } from '../src/actions/browser-socket-delivery.js';

test('a socket opened before the delivery gate is installed is still gated', () => {
  const sent: unknown[] = [];
  const server = { send: (data: unknown) => { sent.push(data); } } as unknown as WebSocketRoute;
  const client = { connectToServer: () => server } as unknown as WebSocketRoute;
  const context = { once: () => {} } as unknown as BrowserContext;
  const delivery = connectBrowserSocket(context, client);
  // No gate yet: frames pass and nothing is recorded.
  assert.equal(delivery.receive('before'), true);
  const seen: string[] = [];
  let created = 0;
  installSocketDeliveryGate(context, () => {
    created++;
    return { sent: data => { seen.push(`sent ${String(data)}`); },
      received: data => { seen.push(`dropped ${String(data)}`); return false; },
      closed: () => { seen.push('closed'); } };
  });
  delivery.send('write');
  assert.equal(delivery.receive('reply'), false);
  delivery.closed();
  assert.deepEqual(sent, ['write']);
  assert.deepEqual(seen, ['sent write', 'dropped reply', 'closed']);
  assert.equal(created, 1, 'one gate per socket');
});
