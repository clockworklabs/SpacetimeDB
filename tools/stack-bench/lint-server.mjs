#!/usr/bin/env node
// Answers the build's hook check over loopback, so nothing in its directory
// names the harness.
//
// This runs as its OWN process for a reason that cost a two-hour run to learn.
// The first version listened inside agent.mjs, which spends the whole session
// blocked in execFileSync waiting for the coding session to finish. A blocked
// event loop accepts no connections, so `curl` hung until the build's own
// 120-second tool timeout and the build never got a single lint result: it
// shipped with 14 missing hooks after three fix rounds. A server in a process
// that is not free to serve is not a server.
//
// Usage: node lint-server.mjs --port-file <path> --cmd <command> [--host <addr>]

import { createServer } from 'node:http';
import { execSync } from 'node:child_process';
import { timingSafeEqual } from 'node:crypto';
import { writeFileSync } from 'node:fs';

const arg = n => { const i = process.argv.indexOf(n); return i === -1 ? null : process.argv[i + 1]; };
const portFile = arg('--port-file');
const cmd = arg('--cmd');
const token = arg('--token');
if (!portFile || !cmd || !token || !/^[a-f0-9]{64}$/.test(token)) {
  console.error('need --port-file, --cmd, and a 256-bit hex --token');
  process.exit(2);
}

function authorized(req) {
  const supplied = req.headers['x-stack-bench-lint-token'];
  if (typeof supplied !== 'string' || supplied.length !== token.length) return false;
  return timingSafeEqual(Buffer.from(supplied), Buffer.from(token));
}

const server = createServer((req, res) => {
  if (req.method !== 'GET' || req.url !== '/lint') {
    res.writeHead(404, { 'content-type': 'text/plain' });
    res.end('not found\n');
    return;
  }
  if (!authorized(req)) {
    res.writeHead(403, { 'content-type': 'text/plain' });
    res.end('forbidden\n');
    return;
  }
  let out = '', ok = true;
  try {
    out = execSync(cmd, {
      encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: 'pipe', timeout: 110_000,
    });
  } catch (e) {
    ok = false;
    out = `${e.stdout ?? ''}${e.stderr ?? ''}` || String(e.message);
  }
  // A failing lint must fail the shim, or a build reads silence as a pass.
  res.writeHead(ok ? 200 : 500, { 'content-type': 'text/plain' });
  res.end(out);
});

// The lint drives a real browser, so a request is slow by nature. Node would
// otherwise close the socket out from under a check that was going to pass.
server.headersTimeout = 0;
server.requestTimeout = 0;
server.timeout = 0;

// Loopback, including for containerised builds. Docker Desktop proxies
// host.docker.internal through to the host's 127.0.0.1 — measured, because the
// obvious guess is the opposite and widening the bind to 0.0.0.0 would put the
// hook server on the LAN for nothing. --host exists for a Docker that does not
// do that (plain Linux with --add-host=host.docker.internal:host-gateway
// reaches the host's gateway address, where a loopback listener is invisible).
server.listen(0, arg('--host') ?? '127.0.0.1', () => {
  // The port file is how the parent learns the port without an async wait: it
  // is written only once the socket is actually accepting.
  writeFileSync(portFile, String(server.address().port));
});
