#!/usr/bin/env node

// Forwarding-only proxy for one interruptible browser context. It runs where the
// browser runs (inside the attempt's browser container in the appliance) and is
// controlled only over stdin/stdout: no control, status, or debug route listens.
// Commands, one JSON object per line: {id, cmd: 'config', user, pass}, then
// {id, cmd: 'cut' | 'restore' | 'dispose'}. EOF or a malformed command closes
// the listener and every owned socket.
import { createServer, get as httpGet, request as httpRequest } from 'node:http';
import type { IncomingHttpHeaders, Server, ServerResponse } from 'node:http';
import { connect } from 'node:net';
import type { Socket } from 'node:net';
import { resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';
import { randomUUID } from 'node:crypto';
import { StringDecoder } from 'node:string_decoder';
import { brotliDecompressSync, gunzipSync, inflateSync, constants } from 'node:zlib';

// Hop-by-hop and proxy headers never reach the application.
const HOP = new Set(['proxy-authorization', 'proxy-connection', 'connection', 'keep-alive',
  'transfer-encoding', 'te', 'trailer', 'upgrade']);
const endToEnd = (headers: IncomingHttpHeaders) =>
  Object.fromEntries(Object.entries(headers).filter(([key]) => !HOP.has(key)));

// A Vite reload socket carries the token its own client module embeds.
export function viteToken(authority: string, token: string): Promise<boolean> {
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
  const limit = 8 * 1024 * 1024;
  let observeHttp = false, incomplete = false, wireBytes = 0, wireChunks = 0, retainedBytes = 0, metadataBytes = 0;
  const bodies = new Set<string>();
  const pendingHttp = new Set<ServerResponse>();
  const records: { id: string; url: string; method: string; status: number; contentType: string;
    body: string; complete: boolean; encoding: string; chunks: Buffer[] | null; bytes: number }[] = [];
  const decoded = (record: typeof records[number]): string => {
    if (!record.chunks) return record.body;
    const wire = Buffer.concat(record.chunks, record.bytes);
    if (!wire.length) return '';
    try {
      const options = { maxOutputLength: limit,
        finishFlush: record.complete ? constants.Z_FINISH : constants.Z_SYNC_FLUSH };
      const bytes = record.encoding === '' || record.encoding === 'identity' ? wire
        : record.encoding === 'gzip' ? gunzipSync(wire, options)
        : record.encoding === 'deflate' ? inflateSync(wire, options)
        : record.encoding === 'br' ? brotliDecompressSync(wire, { maxOutputLength: limit,
          finishFlush: record.complete ? constants.BROTLI_OPERATION_FINISH : constants.BROTLI_OPERATION_FLUSH })
        : null;
      if (!bytes) { incomplete = true; return ''; }
      const decoder = new StringDecoder('utf8');
      return decoder.write(bytes) + (record.complete ? decoder.end() : '');
    } catch { incomplete = true; return ''; }
  };
  const retain = (record: typeof records[number]) => {
    if (!record.chunks) return;
    const body = decoded(record), size = Buffer.byteLength(body);
    if (!bodies.has(body)) {
      if (retainedBytes + size > limit) incomplete = true;
      else { bodies.add(body); retainedBytes += size; record.body = body; }
    }
    wireBytes -= record.bytes;
    wireChunks -= record.chunks.length;
    record.chunks = null;
  };
  const httpSnapshot = () => {
    let size = retainedBytes;
    const seen = new Set(bodies);
    const snapshot = records.map(record => {
      let body = record.body;
      if (record.chunks) {
        body = decoded(record);
        if (seen.has(body)) body = '';
        else if (size + Buffer.byteLength(body) > limit) { incomplete = true; body = ''; }
        else { seen.add(body); size += Buffer.byteLength(body); }
      }
      return { id: record.id, url: record.url, method: record.method, status: record.status,
        contentType: record.contentType, body, complete: record.complete };
    });
    return { records: snapshot, incomplete, pending: pendingHttp.size };
  };
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
      if (observeHttp) {
        pendingHttp.add(res);
        const settled = () => pendingHttp.delete(res);
        res.once('finish', settled);
        res.once('close', settled);
        res.once('error', settled);
      }
      own(req.socket, `${target.hostname}:${target.port || 80}${target.pathname}`);
      const upstream = httpRequest(target, { method: req.method, headers: endToEnd(req.headers) }, answer => {
        const headers = endToEnd(answer.headers);
        if (observeHttp) {
          const record = { id: randomUUID(), url: target.href, method: req.method ?? 'GET',
            status: answer.statusCode ?? 502, contentType: answer.headers['content-type'] ?? '',
            body: '', complete: false, encoding: (answer.headers['content-encoding'] ?? '').trim().toLowerCase(),
            chunks: [] as Buffer[] | null, bytes: 0 };
          // Native EventSource is observed without waiting for stream closure.
          // The browser observer separately rejects fetch-based SSE capture.
          if (/text\/event-stream/i.test(record.contentType)) pendingHttp.delete(res);
          metadataBytes += Buffer.byteLength(JSON.stringify({ ...record, chunks: undefined }));
          // Overwrite any application-supplied receipt. An unretained ID cannot
          // be matched to a successful snapshot by the observer.
          headers['x-stack-bench-capture'] = record.id;
          if (metadataBytes > limit) incomplete = true;
          else {
            records.push(record);
            // Match the existing body observer: binary images remain uninspected,
            // while scripts, styles and even mislabeled HTML fonts are retained.
            const textual = /(application\/(json|[^;]+\+json|x-ndjson|(?:x-)?(?:java|ecma)script)|text\/(plain|html|css|(?:java|ecma)script))/i.test(record.contentType);
            const write = res.write.bind(res);
            res.write = (chunk, encodingOrCallback?, callback?) => {
              const live = !res.destroyed && !res.writableEnded;
              const forwarded = Reflect.apply(write, res, [chunk, encodingOrCallback, callback]) as boolean;
              if (live && textual && record.chunks) {
                const bytes = typeof chunk === 'string' ? Buffer.from(chunk,
                  typeof encodingOrCallback === 'string' ? encodingOrCallback : 'utf8') : Buffer.from(chunk);
                const available = Math.max(0, limit - wireBytes);
                // Bound Buffer/array overhead as well as payload bytes when a
                // response deliberately writes many one-byte chunks.
                if (bytes.length > available || wireChunks >= limit / 64) incomplete = true;
                if (bytes.length && available && wireChunks < limit / 64) {
                  const copy = Buffer.from(bytes.subarray(0, available));
                  record.chunks.push(copy); record.bytes += copy.length; wireBytes += copy.length; wireChunks++;
                }
              }
              return forwarded;
            };
            res.on('finish', () => { record.complete = answer.complete; retain(record); });
            res.on('close', () => retain(record));
          }
        }
        res.writeHead(answer.statusCode ?? 502, headers);
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
    let command: { id?: unknown; cmd?: unknown; user?: unknown; pass?: unknown; observeHttp?: unknown };
    try { command = JSON.parse(line); } catch { return shutdown(2); }
    const { id, cmd } = command ?? {};
    if (cmd === 'config' && !server && typeof command.user === 'string' && typeof command.pass === 'string') {
      observeHttp = command.observeHttp === true;
      expected = `Basic ${Buffer.from(`${command.user}:${command.pass}`).toString('base64')}`;
      return reply({ id, ok: true, port: await listen() });
    }
    if (!server) return shutdown(2);
    if (cmd === 'httpSnapshot' && observeHttp) return reply({ id, ok: true, ...httpSnapshot() });
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
