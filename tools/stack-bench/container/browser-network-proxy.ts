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
    let settled = false;
    const finish = (valid: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      request.destroy();
      done(valid);
    };
    const request = httpGet(`http://${authority}/@vite/client`, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => {
        body += chunk;
        if (body.length > 2_000_000) finish(false);
      });
      response.on('end', () => finish(response.statusCode === 200 && body.includes(`const wsToken = ${JSON.stringify(token)}`)));
    });
    const timer = setTimeout(() => finish(false), 5000);
    request.on('error', () => finish(false));
  });
}

export function startNetworkProxy(reply: (value: object) => void, exit: (code: number) => void) {
  let server: Server | undefined, expected = '', cut = false, port = 0;
  const owned = new Map<Socket, string | null>(); // socket -> the Vite token it carries, if tooling
  const pending = new Map<Socket, Promise<void>>();
  // Where each browser-side connection goes, as host:port plus path (never the query).
  const targets = new Map<Socket, string>();
  const own = (socket: Socket, target?: string) => {
    if (target) targets.set(socket, target);
    if (!owned.has(socket)) {
      owned.set(socket, null);
      socket.on('close', () => { owned.delete(socket); targets.delete(socket); pending.delete(socket); });
      socket.on('error', () => {});
    }
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
      own(req.socket, `${target.hostname}:${target.port || 80}${target.pathname}`);
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
      own(client, req.url);
      const upstream = connect(targetPort, host);
      own(upstream);
      upstream.on('error', () => client.destroy());
      upstream.on('connect', () => {
        client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        // Inspect a copy. Forwarding cannot wait for the asynchronous Vite check.
        let prefix = '', inspecting = true;
        const inspect = (chunk: Buffer) => {
          if (!inspecting) return;
          prefix += chunk.toString('latin1', 0, Math.min(chunk.length, 4096 - prefix.length));
          const end = prefix.indexOf('\r\n');
          if (end < 0 && prefix.length < 4096) return;
          inspecting = false;
          const line = end < 0 ? '' : prefix.slice(0, end);
          const path = /^GET (\S+) HTTP\/1\.1$/.exec(line)?.[1];
          if (path?.startsWith('/')) targets.set(client, `${req.url}${path.split('?')[0]}`);
          const token = path?.startsWith('/') ? new URL(path, 'http://tunnel').searchParams.get('token') : null;
          if (token) {
            const lookup = viteToken(req.url!, token).then(tooling => {
              if (tooling && !client.destroyed && !upstream.destroyed) {
                owned.set(client, token); owned.set(upstream, token);
              }
            }).finally(() => { pending.delete(client); pending.delete(upstream); });
            pending.set(client, lookup); pending.set(upstream, lookup);
          }
        };
        client.on('data', inspect);
        if (head.length) { inspect(head); upstream.write(head); }
        client.pipe(upstream);
        upstream.pipe(client);
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
      const tooling = new Set<string>(), closed: string[] = [];
      const awaiting = [...pending.keys()];
      for (const socket of awaiting) socket.pause();
      for (const [socket, token] of owned) {
        if (pending.has(socket)) continue;
        if (token) { tooling.add(token); continue; }
        socket.destroy();
        const target = targets.get(socket);
        if (target) closed.push(target);
      }
      await Promise.all(new Set(awaiting.map(socket => pending.get(socket)).filter(p => p !== undefined)));
      for (const socket of awaiting) {
        if (socket.destroyed) continue;
        const token = owned.get(socket);
        if (token) { tooling.add(token); socket.resume(); continue; }
        socket.destroy();
        const target = targets.get(socket);
        if (target) closed.push(target);
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
