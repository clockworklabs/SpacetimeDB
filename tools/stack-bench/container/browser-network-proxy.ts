#!/usr/bin/env node

// Forwarding-only proxy for one interruptible browser context. It runs where the
// browser runs (inside the attempt's browser container in the appliance) and is
// controlled only over stdin/stdout: no control, status, or debug route listens.
// Commands, one JSON object per line: {id, cmd: 'config', user, pass}, then
// {id, cmd: 'cut' | 'restore' | 'dispose'}. EOF or a malformed command closes
// the listener and every owned socket.
import { createServer, get as httpGet, request as httpRequest } from 'node:http';
import type { IncomingHttpHeaders, Server } from 'node:http';
import { connect } from 'node:net';
import type { Socket } from 'node:net';
import { resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';

// Hop-by-hop and proxy headers never reach the application.
const HOP = new Set(['proxy-authorization', 'proxy-connection', 'connection', 'keep-alive',
  'transfer-encoding', 'te', 'trailer', 'upgrade']);
const endToEnd = (headers: IncomingHttpHeaders) =>
  Object.fromEntries(Object.entries(headers).filter(([key]) => !HOP.has(key)));

// A Vite reload socket carries the token its own client module embeds.
function viteToken(authority: string, token: string): Promise<boolean> {
  return new Promise(done => {
    const request = httpGet(`http://${authority}/@vite/client`, { timeout: 5000 }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { if (body.length < 2_000_000) body += chunk; });
      response.on('end', () => done(response.statusCode === 200 && body.includes(`const wsToken = ${JSON.stringify(token)}`)));
    });
    request.on('error', () => done(false));
    request.on('timeout', () => { request.destroy(); done(false); });
  });
}

export function startNetworkProxy(reply: (value: object) => void, exit: (code: number) => void) {
  let server: Server | undefined, expected = '', cut = false, port = 0;
  const owned = new Map<Socket, string | null>(); // socket -> the Vite token it carries, if tooling
  const own = (socket: Socket) => {
    owned.set(socket, null);
    socket.on('close', () => owned.delete(socket));
    socket.on('error', () => {});
  };
  const shutdown = (code: number) => {
    for (const socket of owned.keys()) socket.destroy();
    server?.close();
    exit(code);
  };
  // Requests aimed back at this listener would loop.
  const loops = (host: string, target: number) => target === port && /^(127\.|localhost$|\[?::1\]?$)/.test(host);

  const listen = () => new Promise<number>(ready => {
    server = createServer((req, res) => {
      if (req.headers['proxy-authorization'] !== expected) {
        res.writeHead(407, { 'Proxy-Authenticate': 'Basic realm="stack-bench"', Connection: 'close' }).end();
        return;
      }
      if (cut) { req.socket.destroy(); return; }
      let target: URL;
      try { target = new URL(req.url ?? ''); } catch { res.writeHead(400).end(); return; }
      if (target.protocol !== 'http:' || loops(target.hostname, Number(target.port || 80))) {
        res.writeHead(400).end();
        return;
      }
      own(req.socket);
      const upstream = httpRequest(target, { method: req.method, headers: endToEnd(req.headers) }, answer => {
        res.writeHead(answer.statusCode ?? 502, endToEnd(answer.headers));
        answer.pipe(res);
      });
      upstream.on('socket', socket => own(socket));
      upstream.on('error', () => res.destroy());
      req.pipe(upstream);
    });
    server.headersTimeout = 10_000;
    server.maxHeadersCount = 100;
    server.on('connect', (req, client: Socket, head: Buffer) => {
      client.on('error', () => {});
      if (req.headers['proxy-authorization'] !== expected) {
        client.end('HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm="stack-bench"\r\n'
          + 'Content-Length: 0\r\nConnection: close\r\n\r\n');
        return;
      }
      if (cut) { client.destroy(); return; }
      const match = /^([^:\s[\]]+|\[[0-9a-fA-F:.]+\]):(\d{1,5})$/.exec(req.url ?? '');
      const host = match?.[1]?.replace(/^\[|\]$/g, '') ?? '', targetPort = Number(match?.[2]);
      if (!match || targetPort < 1 || targetPort > 65535 || loops(host, targetPort)) {
        client.end('HTTP/1.1 400 Bad Request\r\n\r\n');
        return;
      }
      own(client);
      const upstream = connect(targetPort, host);
      own(upstream);
      upstream.on('error', () => client.destroy());
      upstream.on('connect', () => {
        client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        // A plaintext ws:// upgrade shows its path; only the dev server's own token is tooling.
        const first = async (chunk: Buffer) => {
          const line = chunk.toString('latin1', 0, Math.min(chunk.length, 4096)).split('\r\n')[0] ?? '';
          const path = /^GET (\S+) HTTP\/1\.1$/.exec(line)?.[1];
          const token = path?.startsWith('/') ? new URL(path, 'http://tunnel').searchParams.get('token') : null;
          if (token && await viteToken(req.url!, token)) { owned.set(client, token); owned.set(upstream, token); }
          if (client.destroyed || upstream.destroyed) return;
          upstream.write(chunk);
          client.pipe(upstream);
          upstream.pipe(client);
        };
        if (head.length) void first(head);
        else client.once('data', chunk => { void first(chunk); });
      });
    });
    server.listen(0, '127.0.0.1', () => { port = (server!.address() as { port: number }).port; ready(port); });
  });

  let queue = Promise.resolve();
  const handle = async (line: string) => {
    let command: { id?: unknown; cmd?: unknown; user?: unknown; pass?: unknown };
    try { command = JSON.parse(line); } catch { return shutdown(2); }
    const { id, cmd } = command ?? {};
    if (cmd === 'config' && !server && typeof command.user === 'string' && typeof command.pass === 'string') {
      expected = `Basic ${Buffer.from(`${command.user}:${command.pass}`).toString('base64')}`;
      return reply({ id, ok: true, port: await listen() });
    }
    if (!server) return shutdown(2);
    if (cmd === 'cut') {
      // Refuse first, then close, so nothing races through the cut.
      cut = true;
      let closed = 0;
      const tooling = new Set<string>();
      for (const [socket, token] of owned) {
        if (token) tooling.add(token);
        else { socket.destroy(); closed += 1; }
      }
      return reply({ id, ok: true, closed, tooling: [...tooling] });
    }
    if (cmd === 'restore') { cut = false; return reply({ id, ok: true }); }
    if (cmd === 'dispose') { reply({ id, ok: true }); return shutdown(0); }
    return shutdown(2);
  };
  return (line: string) => { queue = queue.then(() => handle(line)); };
}

function main(): void {
  const receive = startNetworkProxy(value => process.stdout.write(`${JSON.stringify(value)}\n`),
    code => process.exit(code));
  const input = createInterface({ input: process.stdin });
  input.on('line', receive);
  input.on('close', () => receive('dispose-on-eof'));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
