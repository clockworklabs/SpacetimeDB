import type { BrowserContext, Page, WebSocketRoute } from 'playwright';

// Offline emulation holds an open WebSocket's messages and releases them later;
// the socket never disconnects. Route an actor's sockets through the harness so
// going offline closes them and refuses reconnects until the network returns.
export interface NetworkInterruption {
  interrupt(): Promise<{ closed: number; unrouted: number }>;
  restore(): { refused: number };
}

const interruptible = new WeakSet<BrowserContext>();

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
      offline = true;
      const pairs = [...routed];
      routed.clear();
      await Promise.all(pairs.flatMap(pair => [
        pair.page.close({ code: 1001, reason: 'network unavailable' }).catch(() => {}),
        pair.server.close({ code: 1001, reason: 'network unavailable' }).catch(() => {}),
      ]));
      return { closed: pairs.length, unrouted: Math.max(0, opened - routedCount) };
    },
    restore() {
      offline = false;
      return { refused };
    },
  };
}
