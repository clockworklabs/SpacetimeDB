import { afterEach, describe, expect, it, vi } from 'vitest';
import { openWebSocket } from '../src/sdk/ws';

describe('openWebSocket', () => {
  afterEach(() => vi.unstubAllGlobals());

  async function urlsFor(uri: string) {
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
    return { fetched, opened };
  }

  it('keeps a path prefix with or without a trailing slash', async () => {
    for (const uri of ['ws://host/stdb', 'ws://host/stdb/']) {
      const { fetched, opened } = await urlsFor(uri);
      expect(fetched).toEqual(['http://host/stdb/v1/identity/websocket-token']);
      expect(opened[0]).toMatch(
        /^ws:\/\/host\/stdb\/v1\/database\/app\/subscribe\?/
      );
    }
  });

  it('is unchanged for an address without a path', async () => {
    const { fetched, opened } = await urlsFor('wss://host');
    expect(fetched).toEqual(['https://host/v1/identity/websocket-token']);
    expect(opened[0]).toMatch(/^wss:\/\/host\/v1\/database\/app\/subscribe\?/);
  });
});
