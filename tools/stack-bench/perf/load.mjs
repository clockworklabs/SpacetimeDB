#!/usr/bin/env node
// Stack Bench load harness: measure the SYSTEMS, not the authoring.
//
// The functional suites stopped discriminating once the model got good enough
// to pass them on every backend, and what remained moved 20 points between runs
// of the same prompt. This measures three apps that have already been verified
// to do the same thing, so model luck is not part of the result: hold the
// application constant and vary the load.
//
// It drives each app through its own real client — a browser, the same way the
// grader does — because that is the only path all three share. A raw protocol
// driver would have to speak socket.io for two of them and BSATN for the third,
// which measures three different clients rather than three databases.
//
// Reported per backend:
//   delivery latency p50/p95/p99  — send until the message is on other clients
//   loss                          — messages that never arrived anywhere
//   CPU seconds and peak memory   — what the server side spent to do it
//   cost per 1000 delivered       — CPU seconds normalised by work done
//
// Usage:
//   node load.mjs --url http://localhost:6273 --backend postgres --label pg
//                 [--clients 8] [--rounds 20] [--out report.json]

import { existsSync } from 'node:fs';
import { sampleProcesses as hostSample } from '../platform.mjs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { emptyArtifactIdentities, writeArtifact } from '../artifacts.mjs';

// Playwright is already installed for the grader and the linter; resolving it
// from there keeps this tool install-free rather than adding a third copy of a
// browser automation package to the repository.
const HERE = dirname(fileURLToPath(import.meta.url));
function loadPlaywright() {
  for (const dir of ['grader', 'linter']) {
    const pkg = join(HERE, '..', dir, 'node_modules', 'playwright');
    if (existsSync(pkg)) return createRequire(import.meta.url)(pkg);
  }
  console.error('playwright not found in grader/ or linter/ node_modules — run npm install in one of them');
  process.exit(2);
}
const { chromium } = loadPlaywright();

// The level-1 spec requires at most one message per second per ACCOUNT, so a
// send inside that window is correctly refused; pacing below it would measure
// the rate limiter and report the refusals as dropped messages.
const RATE_LIMIT_MS = 1100;

const tid = id => `[data-testid="${id}"]`;
const uniq = () => Math.random().toString(36).slice(2, 7);

function parseArgs(argv) {
  const a = { clients: 8, rounds: 20, warmup: 3, seed: 0, headless: true };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': a.url = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--label': a.label = argv[++i]; break;
      case '--clients': a.clients = parseInt(argv[++i], 10); break;
      case '--rounds': a.rounds = parseInt(argv[++i], 10); break;
      case '--warmup': a.warmup = parseInt(argv[++i], 10); break;
      case '--seed': a.seed = parseInt(argv[++i], 10); break;
      case '--out': a.out = argv[++i]; break;
      case '--headed': a.headless = false; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.url || !a.backend) {
    console.error('Usage: node load.mjs --url <url> --backend <b> [--clients N] [--rounds N] [--out f]');
    process.exit(2);
  }
  a.label ??= a.backend;
  return a;
}

// ─── Server-side resource accounting ─────────────────────────────────────────

// Which processes and containers constitute "the server" for each backend. The
// SpacetimeDB host is shared with whatever else is published on it, so its
// numbers are a delta across the measurement window and are labelled as such.
const SERVER_PROCESSES = {
  postgres: { match: 'results/postgres-run0/app/server', container: 'stack-bench-postgres' },
  mongodb: { match: 'results/mongodb-run0/app/server', container: 'stack-bench-mongodb' },
  spacetime: { match: '127.0.0.1:3210', container: null, shared: false },
};

function sampleProcesses(backend) {
  const spec = SERVER_PROCESSES[backend];
  if (!spec) return null;
  // Process sampling differs per OS; platform.mjs owns that difference so this
  // tool runs on a Linux runner as well as a developer machine.
  const { byPid, rss } = hostSample(spec.match);
  if (!byPid.size) return null;
  return { byPid, peakRssBytes: rss, processes: byPid.size };
}

// CPU actually spent during the window. A process present in both samples
// contributes its increase; one that appeared mid-run contributes all of its
// time. A process that exited is not counted, so this is a lower bound.
function cpuSpent(before, after) {
  if (!before?.byPid || !after?.byPid) return null;
  let total = 0;
  for (const [pid, cpu] of after.byPid) total += cpu - (before.byPid.get(pid) ?? 0);
  return Number(total.toFixed(2));
}

function sampleContainer(backend) {
  const spec = SERVER_PROCESSES[backend];
  if (!spec?.container) return null;
  try {
    const out = execFileSync('docker',
      ['stats', '--no-stream', '--format', '{{.CPUPerc}}|{{.MemUsage}}', spec.container],
      { encoding: 'utf8' }).trim();
    const [cpuPerc, mem] = out.split('|');
    return { cpuPercent: cpuPerc, memory: mem };
  } catch { return null; }
}

// ─── Client setup ────────────────────────────────────────────────────────────

async function signUp(page, url, name) {
  await page.goto(url, { waitUntil: 'domcontentloaded' });
  await page.locator(tid('signup-username')).first().fill(name);
  await page.locator(tid('signup-password')).first().fill(`pw-${name}`);
  await page.locator(tid('signup-submit')).first().click();
  await page.locator(tid('current-user')).first().waitFor({ state: 'visible', timeout: 20000 });
}

async function createRoom(page, roomName) {
  const nameInput = page.locator(tid('room-name-input')).first();
  if (!(await nameInput.isVisible().catch(() => false))) {
    await page.locator(tid('room-create')).first().click();
  }
  await nameInput.fill(roomName);
  await page.locator(tid('room-name-submit')).first().click();
  await page.locator(tid('room-item'), { hasText: roomName }).first()
    .waitFor({ state: 'visible', timeout: 20000 });
}

async function enterRoom(page, roomName) {
  const item = page.locator(tid('room-item'), { hasText: roomName }).first();
  await item.waitFor({ state: 'visible', timeout: 20000 });
  await item.click();
  const input = page.locator(tid('message-input')).first();
  if (!(await input.isVisible().catch(() => false))) {
    await page.waitForTimeout(750);
    if (!(await input.isVisible().catch(() => false))) await item.click();  // join-then-enter
  }
  await input.waitFor({ state: 'visible', timeout: 20000 });
}

const pct = (sorted, p) => sorted.length
  ? sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))]
  : null;

async function main() {
  const args = parseArgs(process.argv);
  const run = uniq();
  const room = `load-${run}`;
  console.log(`\n=== load: ${args.label} (${args.backend}) ===`);
  console.log(`  ${args.clients} clients, ${args.rounds} rounds, ${args.url}`);

  const browser = await chromium.launch({ headless: args.headless });
  const clients = [];
  try {
    for (let i = 0; i < args.clients; i++) {
      const context = await browser.newContext();
      const page = await context.newPage();
      await signUp(page, args.url, `load${i}-${run}`);
      clients.push({ name: `load${i}`, page, context });
    }
    console.log(`  signed up ${clients.length} clients`);

    await createRoom(clients[0].page, room);
    for (const c of clients) await enterRoom(c.page, room);
    console.log(`  all clients in "${room}"`);

    if (args.seed > 0) {
      // Accumulate history before measuring, so the data-volume factor is not
      // tied to how many rounds we time. Deliveries are not awaited here.
      const startedSeed = Date.now();
      for (let i = 0; i < args.seed; i++) {
        const c = clients[i % clients.length];
        const since = Date.now() - (c.lastSentAt ?? 0);
        if (since < RATE_LIMIT_MS) await c.page.waitForTimeout(RATE_LIMIT_MS - since);
        const input = c.page.locator(tid('message-input')).first();
        await input.fill(`SEED-${run}-${String(i).padStart(5, '0')}`);
        await input.press('Enter');
        c.lastSentAt = Date.now();
        if ((i + 1) % 250 === 0) console.log(`  seeded ${i + 1}/${args.seed}`);
      }
      console.log(`  seeded ${args.seed} messages in ${Math.round((Date.now() - startedSeed) / 1000)}s`);
      // Let the backlog land on every client before timing anything.
      await clients[0].page.waitForTimeout(5000);
    }

    // Let subscriptions settle so the first round is not measuring connect time.
    await clients[0].page.waitForTimeout(2000);

    const latencies = [];
    let sent = 0, lost = 0;
    let before = null, started = null;

    // Warm-up rounds are sent and awaited like any other, then discarded. The
    // first requests against a freshly published module or a cold pool are
    // dominated by one-time cost, and leaving them in made whichever
    // configuration ran first look slower than the same work measured later.
    for (let r = -args.warmup; r < args.rounds; r++) {
      const measuring = r >= 0;
      if (measuring && before === null) {
        before = { proc: sampleProcesses(args.backend), container: sampleContainer(args.backend) };
        started = Date.now();
      }
      // Rotate the sender: the level-1 spec requires one message per second per
      // ACCOUNT, so a single sender would measure the rate limiter instead.
      const sender = clients[((r % clients.length) + clients.length) % clients.length];
      const text = `LOAD-${run}-${measuring ? '' : 'W'}${String(Math.abs(r)).padStart(4, '0')}`;
      const receivers = clients.filter(c => c !== sender);

      const waits = receivers.map(c =>
        c.page.locator(tid('message-item'), { hasText: text }).first()
          .waitFor({ state: 'visible', timeout: 15000 })
          .then(() => Date.now())
          .catch(() => null));

      // The limit is per account, so wait out this sender's own budget rather
      // than assuming the rotation is slow enough. A rejected send would be
      // recorded as a loss and read as the backend dropping messages.
      const since = Date.now() - (sender.lastSentAt ?? 0);
      if (since < RATE_LIMIT_MS) await sender.page.waitForTimeout(RATE_LIMIT_MS - since);

      const t0 = Date.now();
      await sender.page.locator(tid('message-input')).first().fill(text);
      await sender.page.locator(tid('message-input')).first().press('Enter');
      sender.lastSentAt = Date.now();
      if (measuring) sent++;

      const arrivals = await Promise.all(waits);
      if (!measuring) continue;                     // warm-up: sent, awaited, discarded
      for (const at of arrivals) {
        if (at === null) lost++;
        else latencies.push(at - t0);
      }
    }

    const elapsedMs = Date.now() - started;
    const after = { proc: sampleProcesses(args.backend), container: sampleContainer(args.backend) };

    const sorted = [...latencies].sort((a, b) => a - b);
    const delivered = latencies.length;
    const cpuSeconds = cpuSpent(before.proc, after.proc);

    const report = {
      label: args.label, backend: args.backend, url: args.url,
      clients: args.clients, rounds: args.rounds, warmupDiscarded: args.warmup,
      seededBefore: args.seed,
      sent, delivered, lost,
      elapsedMs,
      deliveryLatencyMs: {
        p50: pct(sorted, 50), p95: pct(sorted, 95), p99: pct(sorted, 99),
        min: sorted[0] ?? null, max: sorted[sorted.length - 1] ?? null,
      },
      server: {
        cpuSeconds,
        // The SpacetimeDB host serves other databases, so its CPU is a delta
        // over the window and still includes anything else published on it.
        cpuShared: Boolean(SERVER_PROCESSES[args.backend]?.shared),
        peakRssBytes: after.proc?.peakRssBytes ?? null,
        processes: after.proc?.processes ?? null,
        container: after.container ?? null,
      },
      // The comparable cost figure: server CPU spent per thousand messages
      // actually delivered to a client, which normalises away fan-out size.
      cpuSecondsPer1kDelivered: cpuSeconds !== null && delivered
        ? Number((cpuSeconds / delivered * 1000).toFixed(3)) : null,
    };

    console.log(`  delivered ${delivered}/${sent * (clients.length - 1)} (lost ${lost})`);
    console.log(`  latency p50 ${report.deliveryLatencyMs.p50}ms  p95 ${report.deliveryLatencyMs.p95}ms  p99 ${report.deliveryLatencyMs.p99}ms`);
    console.log(`  server CPU ${cpuSeconds ?? '?'}s${report.server.cpuShared ? ' (shared host — delta)' : ''}, peak RSS ${
      report.server.peakRssBytes ? (report.server.peakRssBytes / 1e6).toFixed(0) + 'MB' : '?'}`);
    console.log(`  cost ${report.cpuSecondsPer1kDelivered ?? '?'} CPU-seconds per 1000 delivered`);

    if (args.out) {
      const id = `performance-${args.label}-${new Date(started).toISOString().replace(/[:.]/g, '-')}`;
      writeArtifact(args.out, {
        kind: 'performance_run', id,
        timestamps: { startedAt: new Date(started).toISOString(), completedAt: new Date().toISOString() },
        identities: emptyArtifactIdentities({ stackAdapter: { id: args.backend } }),
        payload: report,
      });
      console.log(`  wrote ${args.out}`);
    }
  } finally {
    await browser.close();
  }
}

main();
