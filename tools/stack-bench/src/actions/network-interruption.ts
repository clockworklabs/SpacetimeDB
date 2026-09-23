import type { BrowserContext, Page, Request, WebSocketRoute } from 'playwright';

// Offline emulation holds an open WebSocket's messages and releases them later;
// the socket never disconnects, and an HTTP request already in flight (a long
// poll, an event stream) still completes. Route an actor's sockets through the
// harness so going offline closes them and refuses reconnects until the network
// returns, and let in-flight requests finish before the cut.
export interface NetworkInterruption {
  interrupt(): Promise<{ closed: number; unrouted: number; open: number }>;
  restore(): { refused: number };
}

const interruptible = new WeakSet<BrowserContext>();
const DRAIN_MS = 5000;

// A dev server's reload socket (Vite: the root path with a token) is tooling,
// not the app. Cutting it makes Vite reload the whole page, so it stays open.
const devServerSocket = (url: URL): boolean => url.pathname === '/' && url.searchParams.has('token');

export const hasNetworkInterruption = (context: BrowserContext): boolean => interruptible.has(context);

// Install before the context opens a page, so every socket passes through the route.
export async function installNetworkInterruption(context: BrowserContext): Promise<NetworkInterruption> {
  interruptible.add(context);
  const routed = new Set<{ page: WebSocketRoute; server: WebSocketRoute }>();
  let offline = false;
  let refused = 0;
  // Every socket a page opens must reach the route; one that did not could keep delivering.
  let opened = 0;
  let routedCount = 0;
  const track = (page: Page) => page.on('websocket', socket => {
    if (!devServerSocket(new URL(socket.url()))) opened += 1;
  });
  context.on('page', track);
  const inFlight = new Set<Request>();
  context.on('request', request => inFlight.add(request));
  context.on('requestfinished', request => inFlight.delete(request));
  context.on('requestfailed', request => inFlight.delete(request));
  await context.routeWebSocket(url => !devServerSocket(url), page => {
    routedCount += 1;
    if (offline) {
      refused += 1;
      void page.close({ code: 1001, reason: 'network unavailable' }).catch(() => {});
      return;
    }
    const pair = { page, server: page.connectToServer() };
    routed.add(pair);
    page.onMessage(data => pair.server.send(data));
    pair.server.onMessage(data => page.send(data));
    page.onClose((code, reason) => { routed.delete(pair); void pair.server.close({ code, reason }).catch(() => {}); });
    pair.server.onClose((code, reason) => { routed.delete(pair); void page.close({ code, reason }).catch(() => {}); });
  });
  return {
    async interrupt() {
      // The caller already blocks new requests. A socket cut mid-handshake (an
      // HTTP-to-WebSocket upgrade) is not an outage the app can observe cleanly.
      for (const end = Date.now() + DRAIN_MS; inFlight.size && Date.now() < end;) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      offline = true;
      const pairs = [...routed];
      routed.clear();
      await Promise.all(pairs.flatMap(pair => [
        pair.page.close({ code: 1001, reason: 'network unavailable' }).catch(() => {}),
        pair.server.close({ code: 1001, reason: 'network unavailable' }).catch(() => {}),
      ]));
      return { closed: pairs.length, unrouted: Math.max(0, opened - routedCount), open: inFlight.size };
    },
    restore() {
      offline = false;
      return { refused };
    },
  };
}
