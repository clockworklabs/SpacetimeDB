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
  attach(context: BrowserContext): void;
  interrupt(): Promise<{ closed: number; open: number }>;
  restore(): Promise<void>;
  dispose(): Promise<void>;
}

const SETTLE_MS = 5000;
const COMMAND_TIMEOUT_MS = 10_000;

type Reply = { id?: number; ok?: boolean; port?: number; closed?: number; tooling?: string[] };

// Start before the actor's context exists; register dispose() with its cleanup at once.
export async function startNetworkInterruption(): Promise<NetworkInterruption> {
  const container = browserContainer();
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
    await command('dispose').catch(() => {});
    child.stdin.end();
    // Closing the pipe ends the remote helper; the local client must not outlive it.
    if (!stopped) child.kill();
  };
  const username = 'stack-bench', password = randomBytes(24).toString('hex');
  let port: number | undefined;
  try { ({ port } = await command('config', { user: username, pass: password })); }
  catch (error) { await dispose(); throw error; }

  const sockets = new Set<WebSocket>();
  const inFlight = new Set<Request>();
  return {
    // `<-loopback>` removes Chromium's implicit loopback bypass, so local apps go through it too.
    proxy: { server: `http://127.0.0.1:${port}`, bypass: '<-loopback>', username, password },
    attach(context) {
      const track = (page: Page) => page.on('websocket', socket => {
        sockets.add(socket);
        socket.on('close', () => sockets.delete(socket));
      });
      context.on('page', track);
      context.on('request', request => inFlight.add(request));
      context.on('requestfinished', request => inFlight.delete(request));
      context.on('requestfailed', request => inFlight.delete(request));
    },
    async interrupt() {
      const { closed = 0, tooling = [] } = await command('cut');
      // The browser itself must see every application connection end. A Vite dev
      // server's reload socket is tooling (cutting it reloads the page); the proxy
      // exempted only sockets whose token that server's own client module carries.
      const application = (socket: WebSocket) => !tooling.includes(new URL(socket.url()).searchParams.get('token') ?? '');
      const open = () => [...sockets].filter(application).length + inFlight.size;
      for (const end = Date.now() + SETTLE_MS; open() && Date.now() < end;) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      return { closed, open: open() };
    },
    async restore() { await command('restore'); },
    dispose,
  };
}
