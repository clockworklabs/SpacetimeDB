import { createHash } from 'node:crypto';
import type { BrowserContext, WebSocketRoute } from 'playwright';
import { evidenceNowMs } from '../src/evidence/evidence-timing.js';

// Install before the actor opens its connection. This gate changes delivery,
// never request contents, database state, or the application's retry policy.
export async function installResponseLoss(context: BrowserContext) {
  if (context.serviceWorkers().length) throw new Error('response loss cannot observe an existing service worker');
  const sockets = new Set<{ page: WebSocketRoute; server: WebSocketRoute }>();
  const pending = new Set<Promise<void>>();
  const events: { kind: 'http-request' | 'http-response' | 'ws-send' | 'ws-drop';
    atMs: number; bytes?: number; sha256?: string; status?: number; path: string }[] = [];
  const errors = new Set<string>();
  context.on('serviceworker', () => { errors.add('service worker bypasses response interception'); });
  let state: 'ready' | 'armed' | 'finished' = 'ready', armedAtMs: number | null = null;
  let truncated = false, release!: () => void;
  const released = new Promise<void>(resolve => { release = resolve; });
  const record = (kind: typeof events[number]['kind'], path: string, data?: string | Buffer, status?: number) => {
    // Bounded evidence: an overflow is unmeasured, never a successful fault.
    if (events.length === 256) { truncated = true; return; }
    events.push({ kind, path, atMs: evidenceNowMs(), ...(data === undefined ? {} : {
      bytes: Buffer.byteLength(data), sha256: createHash('sha256').update(data).digest('hex'),
    }), ...(status === undefined ? {} : { status }) });
  };
  // The isolated actor loses write replies on all its HTTP paths and inbound
  // WebSocket messages. This also covers SDK retries and proxy URLs unchanged.
  await context.route('**/*', route => {
    const task = (async () => {
      if (state !== 'armed' || ['GET', 'HEAD', 'OPTIONS'].includes(route.request().method())) return route.continue();
      const path = new URL(route.request().url()).pathname;
      record('http-request', path, route.request().postDataBuffer() ?? Buffer.alloc(0));
      // No redirects or transport retries: one intercepted write remains one write.
      const response = await route.fetch({ maxRedirects: 0, maxRetries: 0, timeout: 10_000 });
      try {
        record('http-response', path, undefined, response.status());
        await released;
        await route.abort('connectionreset');
      } finally { await response.dispose(); }
    })().catch(() => { errors.add('HTTP response interception failed'); });
    pending.add(task);
    void task.finally(() => pending.delete(task));
    return task;
  });
  await context.routeWebSocket('**/*', page => {
    const server = page.connectToServer(), pair = { page, server };
    sockets.add(pair);
    const path = new URL(page.url()).pathname;
    page.onMessage(data => {
      try {
        if (state === 'armed') record('ws-send', path, data);
        server.send(data);
      } catch { errors.add('WebSocket request forwarding failed'); }
    });
    server.onMessage(data => {
      try {
        if (state === 'armed') record('ws-drop', path, data);
        else page.send(data);
      } catch { errors.add('WebSocket response forwarding failed'); }
    });
    page.onClose(async (code, reason) => {
      sockets.delete(pair);
      try { await server.close({ code, reason }); }
      catch { errors.add('WebSocket server close failed'); }
    });
    server.onClose(async (code, reason) => {
      sockets.delete(pair);
      try { await page.close({ code, reason }); }
      catch { errors.add('WebSocket client close failed'); }
    });
  });
  return {
    arm() {
      if (state !== 'ready') throw new Error('response-loss gate can be armed only once');
      state = 'armed'; armedAtMs = evidenceNowMs();
    },
    evidence() { return { state, armedAtMs, truncated, errors: [...errors], events: [...events] }; },
    async finish() {
      if (state === 'finished') return;
      state = 'finished';
      release();
      // Discard the lost reply permanently. New SDK connections can recover normally.
      const closed = await Promise.allSettled([...sockets].flatMap(({ page, server }) => [
        page.close({ code: 1011, reason: 'connection interrupted' }),
        server.close({ code: 1011, reason: 'connection interrupted' }),
      ]));
      if (closed.some(result => result.status === 'rejected')) errors.add('WebSocket fault cleanup failed');
      await Promise.all([...pending]);
    },
  };
}
