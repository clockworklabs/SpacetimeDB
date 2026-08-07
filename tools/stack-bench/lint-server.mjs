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
// Usage: node lint-server.mjs --port-file <path> --cmd <command>

import { createServer } from 'node:http';
import { execSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const arg = n => { const i = process.argv.indexOf(n); return i === -1 ? null : process.argv[i + 1]; };
const portFile = arg('--port-file');
const cmd = arg('--cmd');
if (!portFile || !cmd) { console.error('need --port-file and --cmd'); process.exit(2); }

const server = createServer((req, res) => {
  let out = '', ok = true;
  try {
    out = execSync(cmd, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, stdio: 'pipe' });
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

server.listen(0, '127.0.0.1', () => {
  // The port file is how the parent learns the port without an async wait: it
  // is written only once the socket is actually accepting.
  writeFileSync(portFile, String(server.address().port));
});
