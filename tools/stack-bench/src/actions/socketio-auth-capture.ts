import type { Page, Request } from 'playwright';
import { withBrowserRequest } from './browser-request.js';

interface ConnectCall {
  packet: string;
  finish(success: boolean | null): void;
}
interface Owner {
  connect(endpoint: string, body: Record<string, unknown> | null, packet: string): ConnectCall;
  fail(): void;
}
interface Session {
  endpoint: string;
  sid: string;
  websocket: boolean;
  connected: boolean;
  connecting: boolean;
  ping: boolean;
  probing: boolean;
  probed: boolean;
  pending?: { owner: Owner; call: ConnectCall };
}
interface Capture { activate(owner: Owner): { stop(): void; dispose(): void } }
const captures = new WeakMap<object, Capture>();
const object = (value: unknown): value is Record<string, unknown> => Boolean(value) && typeof value === 'object' && !Array.isArray(value);

export function socketIoTransport(address: string, transport: 'polling' | 'websocket'): boolean {
  const url = new URL(address);
  return url.searchParams.get('EIO') === '4' && url.searchParams.get('transport') === transport;
}
export const socketIoAuthCapture = (page: object) => captures.get(page);

// Engine.IO v4 controls have no application payload. Socket.IO CONNECT does:
// keep it in the write inventory and require its namespace receipt. Other client
// application packets remain unmeasured. https://github.com/socketio/socket.io-protocol
export async function installSocketIoAuthCapture(page: Page): Promise<Capture> {
  const sessions = new Map<string, Session>();
  const routed = new WeakSet<Request>();
  let owner: Owner | undefined;
  const endpoint = (address: string) => {
    const url = new URL(address);
    url.protocol = url.protocol === 'wss:' ? 'https:' : url.protocol === 'ws:' ? 'http:' : url.protocol;
    for (const name of ['sid', 't', 'transport']) url.searchParams.delete(name);
    url.searchParams.sort();
    return url.href;
  };
  const key = (address: string, sid: string) => JSON.stringify([endpoint(address), sid]);
  const sessionFor = (address: string) => sessions.get(key(address, new URL(address).searchParams.get('sid') ?? ''));
  const finish = (session: Session, success: boolean | null) => {
    const pending = session.pending; session.pending = undefined;
    pending?.call.finish(success);
  };
  const receive = (address: string, packet: string, session?: Session, observer: Owner | null = owner ?? null): Session | undefined => {
    try {
      if (packet.startsWith('0')) {
        const open: unknown = JSON.parse(packet.slice(1));
        if (!object(open) || typeof open.sid !== 'string' || !open.sid || !Array.isArray(open.upgrades)
          || open.upgrades.some(value => value !== 'websocket')
          || typeof open.pingInterval !== 'number' || open.pingInterval <= 0
          || typeof open.pingTimeout !== 'number' || open.pingTimeout <= 0) throw new Error('Invalid Engine.IO handshake');
        session = { endpoint: endpoint(address), sid: open.sid, websocket: open.upgrades.includes('websocket'),
          connected: false, connecting: false, ping: false, probing: false, probed: false };
        sessions.set(key(address, session.sid), session);
      } else if (session) {
        if (packet === '2') session.ping = true;
        else if (packet === '3probe' && session.probing) session.probed = true;
        else if (packet.startsWith('40') || packet.startsWith('44')) {
          const data: unknown = JSON.parse(packet.slice(2));
          const accepted = packet.startsWith('40');
          if (!object(data) || (accepted ? typeof data.sid !== 'string' || !data.sid : typeof data.message !== 'string')) {
            throw new Error('Invalid Socket.IO connection receipt');
          }
          session.connecting = false; session.connected = accepted; finish(session, accepted);
        } else if (packet === '41' || packet === '1') { session.connecting = false; session.connected = false; finish(session, null); }
      }
    } catch { observer?.fail(); if (session) finish(session, null); }
    return session;
  };
  const send = (session: Session | undefined, packet: string, websocket: boolean): string => {
    if (!session) { owner?.fail(); return packet; }
    try {
      if (packet === '3' && session.ping) session.ping = false;
      else if (packet === '2probe' && websocket && session.websocket && !session.probing) session.probing = true;
      else if (packet === '5' && websocket && session.probed) { session.probing = false; session.probed = false; }
      else if (packet === '41' && session.connected) session.connected = false;
      else if (packet === '1') { session.connecting = false; session.connected = false; finish(session, null); }
      else if (packet === '40' || packet.startsWith('40{')) {
        const body: unknown = packet === '40' ? null : JSON.parse(packet.slice(2));
        if (body !== null && !object(body) || session.connecting || session.connected) throw new Error('Unproved CONNECT');
        session.connecting = true;
        if (owner) {
          const call = owner.connect(session.endpoint, body as Record<string, unknown> | null, packet);
          session.pending = { owner, call };
          return call.packet;
        }
      } else owner?.fail();
    } catch { owner?.fail(); }
    return packet;
  };
  // Observe normal initial handshakes without changing requests outside a probe.
  page.on('response', response => {
    const request = response.request();
    if (routed.has(request) || !response.ok() || request.method() !== 'GET' || !socketIoTransport(request.url(), 'polling')) return;
    void response.text().then(body => {
      let session = sessionFor(request.url());
      for (const packet of body.split('\x1e')) session = receive(request.url(), packet, session, null);
    }).catch(() => {});
  });
  await page.route(url => socketIoTransport(url.href, 'polling'), async route => {
    const active = owner;
    const request = route.request();
    let session = sessionFor(request.url());
    const body = request.method() === 'POST'
      ? (request.postData() ?? '').split('\x1e').map(packet => send(session, packet, false)).join('\x1e') : undefined;
    if (!active) return route.fallback();
    routed.add(request);
    try {
      if (!['GET', 'POST'].includes(request.method())) active.fail();
      await withBrowserRequest(page.request, async api => {
        const response = await api.fetch(request, { ...(body === undefined ? {} : { data: body }),
          maxRedirects: 0, maxRetries: 0, timeout: 35000 });
        if (!response.ok()) { active.fail(); if (session) finish(session, null); }
        else if (request.method() === 'GET') {
          for (const packet of (await response.text()).split('\x1e')) session = receive(request.url(), packet, session, active);
        } else if (await response.text() !== 'ok') active.fail();
        await route.fulfill({ response });
      }, false);
    } catch { active.fail(); await route.abort().catch(() => {}); }
  });
  await page.routeWebSocket(url => socketIoTransport(url.href, 'websocket'), client => {
    const server = client.connectToServer();
    let session = sessionFor(client.url());
    client.onMessage(data => {
      session ??= sessionFor(client.url());
      if (typeof data !== 'string') { owner?.fail(); server.send(data); return; }
      server.send(send(session, data, true));
    });
    server.onMessage(data => {
      session ??= sessionFor(client.url());
      if (typeof data === 'string') session = receive(client.url(), data, session);
      client.send(data);
    });
    const closed = () => { if (session) finish(session, null); };
    client.onClose(async (code, reason) => { closed(); await server.close({ code, reason }).catch(closed); });
    server.onClose(async (code, reason) => { closed(); await client.close({ code, reason }).catch(closed); });
  });
  const capture = {
    activate(next: Owner) {
      if (owner) throw new Error('Socket.IO authentication capture is already active');
      owner = next;
      return {
        stop() { if (owner === next) owner = undefined; },
        dispose() {
          if (owner === next) owner = undefined;
          for (const session of sessions.values()) if (session.pending?.owner === next) finish(session, null);
        },
      };
    },
  };
  captures.set(page, capture);
  return capture;
}
