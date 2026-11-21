import { afterEach, expect, it, vi } from 'vitest';
import { openWebSocket } from '../src/sdk/ws';

afterEach(() => vi.unstubAllGlobals());

it.each([
  ['ws://host/stdb', 'http://host/stdb/', 'ws://host/stdb/'],
  ['ws://host/stdb/', 'http://host/stdb/', 'ws://host/stdb/'],
  ['wss://host', 'https://host/', 'wss://host/'],
])('connects under the path of %s', async (uri, http, ws) => {
  const fetched: string[] = [];
  const opened: string[] = [];
  vi.stubGlobal('fetch', async (url: URL) => {
    fetched.push(url.href);
    return new Response(JSON.stringify({ token: 'short' }), { status: 200 });
  });
  vi.stubGlobal(
    'WebSocket',
    class {
      binaryType = '';
      constructor(url: string) {
        opened.push(url);
      }
    }
  );
  await openWebSocket({
    url: new URL(uri),
    nameOrAddress: 'app',
    wsProtocol: [],
    authToken: 'stored',
    compression: 'none',
    lightMode: false,
  });
  expect(fetched).toEqual([`${http}v1/identity/websocket-token`]);
  expect(opened[0].startsWith(`${ws}v1/database/app/subscribe?`)).toBe(true);
});
