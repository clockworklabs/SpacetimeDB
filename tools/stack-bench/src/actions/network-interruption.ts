import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { createInterface } from 'node:readline';
import type { BrowserContext, BrowserContextOptions, Page, Request, WebSocket } from 'playwright';
import { browserContainer } from '../../container/browser-pipe.js';
import { compiledEntrypoint } from '../package-root.js';

// Offline emulation holds an open WebSocket's messages and releases them later,
// and lets an HTTP request already in flight (a long poll, an event stream) keep
// delivering. An interruptible actor's context therefore sends all its traffic
// through a harness proxy that runs where the browser runs. Going offline refuses
// new connections and closes the existing ones, as a network outage would.
export interface NetworkInterruption {
  // Context options that route the actor's traffic, loopback included, through the proxy.
  readonly proxy: NonNullable<BrowserContextOptions['proxy']>;
  attach(context: BrowserContext, page: Page): Promise<void>;
  interrupt(): Promise<{ closed: number; open: string[] }>;
  restore(): Promise<void>;
  dispose(): Promise<void>;
}

const SETTLE_MS = 5000;
const COMMAND_TIMEOUT_MS = 10_000;

type Reply = { id?: number; ok?: boolean; port?: number; closed?: string[]; tooling?: string[] };

// Start before the actor's context exists; register dispose() with its cleanup at once.
export async function startNetworkInterruption(): Promise<NetworkInterruption> {
  // The proxy runs where the browser runs: the leased browser container, or this
  // host for an unleased browser such as the null control's own browser server.
  const container = process.env.STACK_BENCH_LEASE ? browserContainer() : null;
  const helper = compiledEntrypoint('container', 'browser-network-proxy.js');
  const child = container
    ? spawn('docker', ['exec', '-i', container, 'node', helper], { stdio: ['pipe', 'pipe', 'ignore'] })
    : spawn(process.execPath, [helper], { stdio: ['pipe', 'pipe', 'ignore'] });
  const waiting = new Map<number, (reply: Reply | null) => void>();
  let next = 0, stopped = false;
  createInterface({ input: child.stdout }).on('line', line => {
    let reply: Reply;
    try { reply = JSON.parse(line); } catch { return; }
    waiting.get(reply.id ?? -1)?.(reply);
    waiting.delete(reply.id ?? -1);
  });
  const stop = () => { stopped = true; for (const settle of waiting.values()) settle(null); waiting.clear(); };
  child.on('exit', stop);
  child.on('error', stop);
  child.stdin.on('error', () => {});
  const command = (cmd: string, extra: object = {}) => new Promise<Reply>((done, fail) => {
    if (stopped) { fail(new Error('the network interruption proxy has stopped')); return; }
    const id = ++next;
    const timer = setTimeout(() => {
      waiting.delete(id);
      fail(new Error(`the network interruption proxy did not acknowledge ${cmd}`));
    }, COMMAND_TIMEOUT_MS);
    waiting.set(id, reply => {
      clearTimeout(timer);
      if (reply?.ok) done(reply);
      else fail(new Error(`the network interruption proxy failed ${cmd}`));
    });
    child.stdin.write(`${JSON.stringify({ id, cmd, ...extra })}\n`);
  });
  const dispose = async () => {
    let acknowledgement: unknown;
    try { await command('dispose'); }
    catch (error) { acknowledgement = error; }
    stopped = true;
    child.stdin.end();
    const exited = () => child.exitCode !== null || child.signalCode !== null;
    const waitForExit = () => new Promise<boolean>(resolve => {
      const timer = setTimeout(() => { child.off('exit', onExit); resolve(false); }, COMMAND_TIMEOUT_MS);
      const onExit = () => { clearTimeout(timer); resolve(true); };
      child.once('exit', onExit);
      if (exited()) { child.off('exit', onExit); clearTimeout(timer); resolve(true); }
    });
    if (!exited()) {
      const confirmed = await waitForExit();
      if (!confirmed) {
        child.kill();
        const localExit = await waitForExit();
        throw new Error(`network interruption proxy exit was not confirmed; remote cleanup is unknown${localExit ? '' : ' and local client exit is unknown'}`,
          { cause: acknowledgement });
      }
    }
    if (child.exitCode !== 0) {
      throw new Error(`network interruption proxy cleanup was not confirmed (exit ${child.exitCode ?? child.signalCode})`,
        { cause: acknowledgement });
    }
  };
  const username = 'stack-bench', password = randomBytes(24).toString('hex');
  let port: number | undefined;
  try { ({ port } = await command('config', { user: username, pass: password })); }
  catch (error) {
    try { await dispose(); }
    catch (cleanupError) { throw new AggregateError([error, cleanupError], 'network interruption setup and cleanup failed'); }
    throw error;
  }

  const sockets = new Set<WebSocket>();
  const openedDuringCut = new WeakSet<WebSocket>();
  let cutting = false;
  let trackingFailure: unknown;
  const inFlight = new Set<Request>();
  const attachedContexts = new WeakSet<BrowserContext>();
  const trackedPages = new WeakMap<Page, Promise<void>>();
  const track = (context: BrowserContext, page: Page): Promise<void> => {
    const existing = trackedPages.get(page);
    if (existing) return existing;
    const pageSockets = new Set<WebSocket>();
    // A replaced document's sockets end without a close event from Playwright.
    const forget = () => { for (const socket of pageSockets) sockets.delete(socket); pageSockets.clear(); };
    page.on('websocket', socket => {
      sockets.add(socket);
      if (cutting) openedDuringCut.add(socket);
      pageSockets.add(socket);
      socket.on('close', () => { sockets.delete(socket); pageSockets.delete(socket); });
    });
    const ready = context.newCDPSession(page).then(async session => {
      session.on('Page.frameNavigated', (event: { frame: { parentId?: string } }) => {
        if (!event.frame.parentId) forget();
      });
      page.on('close', () => { forget(); void session.detach().catch(() => {}); });
      try { await session.send('Page.enable'); }
      catch (error) { await session.detach().catch(() => {}); throw error; }
    });
    trackedPages.set(page, ready);
    return ready;
  };
  return {
    // `<-loopback>` removes Chromium's implicit loopback bypass, so local apps go through it too.
    proxy: { server: `http://127.0.0.1:${port}`, bypass: '<-loopback>', username, password },
    async attach(context, page) {
      if (!attachedContexts.has(context)) {
        attachedContexts.add(context);
        context.on('page', opened => { void track(context, opened).catch(error => { trackingFailure = error; }); });
        context.on('request', request => inFlight.add(request));
        context.on('requestfinished', request => inFlight.delete(request));
        context.on('requestfailed', request => inFlight.delete(request));
      }
      await track(context, page);
    },
    async interrupt() {
      if (trackingFailure) throw trackingFailure;
      cutting = true;
      const { closed = [], tooling = [] } = await command('cut');
      // Every application connection must be seen to end by the browser or by the proxy.
      // A Vite dev server's reload socket is tooling (cutting it reloads the page); the
      // proxy exempted only sockets whose token that server's own client module carries.
      const application = (socket: WebSocket) => !tooling.includes(new URL(socket.url()).searchParams.get('token') ?? '');
      // Name what stayed open by origin and path only; queries can carry credentials.
      const where = (url: string) => { const parsed = new URL(url); return `${parsed.origin}${parsed.pathname}`; };
      // Emulated offline can hold a close event until the network returns, so a connection
      // also counts as cut when the proxy closed one to the same host, port and path (an
      // encrypted tunnel shows only its host and port).
      const cutByProxy = (tally: Map<string, number>, url: string) => {
        const parsed = new URL(url);
        const authority = `${parsed.hostname}:${parsed.port || (/^(https|wss):$/.test(parsed.protocol) ? 443 : 80)}`;
        for (const target of [`${authority}${parsed.pathname}`, authority]) {
          const count = tally.get(target) ?? 0;
          if (count) { tally.set(target, count - 1); return true; }
        }
        return false;
      };
      const open = () => {
        const tally = new Map<string, number>();
        for (const target of closed) tally.set(target, (tally.get(target) ?? 0) + 1);
        return [...[...sockets].filter(application).filter(socket => !cutByProxy(tally, socket.url()))
          .map(socket => `websocket ${where(socket.url())}${openedDuringCut.has(socket) ? ' (opened during the cut)' : ''}`),
        ...[...inFlight].filter(request => !cutByProxy(tally, request.url()))
          .map(request => `${request.resourceType()} ${request.method()} ${where(request.url())}`)];
      };
      for (const end = Date.now() + SETTLE_MS; open().length && Date.now() < end;) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      return { closed: closed.length, open: open() };
    },
    async restore() { await command('restore'); cutting = false; },
    dispose,
  };
}
