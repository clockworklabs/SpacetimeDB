#!/usr/bin/env node
// Stack Bench functional grader.
//
// Executes versioned scenario specs against a running app using multiple
// isolated browser contexts (one per actor), and scores each feature 0-3 from
// observed behavior only. Mechanically enforces the rules human graders applied:
//   - JS console errors during a feature cap it at 2/3
//   - a feature that only works after a reload caps at 1/3
//   - untestable (setup failed) scores 0
//
// The grader never reloads a page except in the refresh probe, so "realtime"
// means realtime.
//
// Usage: node grade.mjs --url http://localhost:6173 --level 1 [--out report.json]
//                      [--label spacetime-l1] [--headed] [--feature N]
// Exit codes: 0 = graded (any score), 2 = usage/infra error.

import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_WITHIN = 5000;
const MAX_PER_FEATURE = 3;

function parseArgs(argv) {
  const args = { level: 1, headed: false };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': args.url = argv[++i]; break;
      case '--level': args.level = parseInt(argv[++i], 10); break;
      case '--out': args.out = argv[++i]; break;
      case '--label': args.label = argv[++i]; break;
      case '--feature': args.feature = parseInt(argv[++i], 10); break;
      case '--spec': args.spec = argv[++i]; break;
      case '--restart-cmd': args.restartCmd = argv[++i]; break;
      case '--media': args.media = argv[++i]; break;
      case '--trace': args.trace = true; break;
      case '--headed': args.headed = true; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!args.url) {
    console.error('Usage: node grade.mjs --url <app-url> --level <N> [--out <file>] [--label <s>] [--feature <N>]');
    process.exit(2);
  }
  return args;
}

const tid = id => `[data-testid="${id}"]`;
const uniq = () => Math.random().toString(36).slice(2, 7);

// ─── Actor: an isolated browser context with its own identity ────────────────

const IDENTITY_FIELD = /^(user_?id|sender_?id|author_?id|from_?user|identity)$/i;
const CONTENT_FIELD = /^(content|text|message|body|msg)$/i;
const ROOM_FIELD = /^(room_?id|channel_?id|conversation_?id)$/i;

class Actor {
  constructor(name, page) {
    this.name = name;
    this.page = page;
    this.consoleErrors = [];
    // Record the app's own write requests so a test can replay one with a
    // tampered field. This adapts to whatever API the app happens to expose,
    // instead of the harness having to know its shape.
    this.lastWrite = null;
    this.lastWsWrite = null;
    // Apps write over WebSocket as often as over HTTP (socket.io emits a text
    // frame like 42["send_message",{...}]). We cannot replay those as easily,
    // but we can see whether the payload carries a client-supplied identity at
    // all — if it does not, the server must be deriving identity from the
    // connection, which is the property being tested.
    page.on('websocket', ws => {
      ws.on('framesent', f => {
        const p = typeof f.payload === 'string' ? f.payload : '';
        const m = p.match(/^\d+(\[.*\])$/s);
        if (!m) return;
        try {
          const [event, arg] = JSON.parse(m[1]);
          if (arg && typeof arg === 'object') this.lastWsWrite = { event, body: arg };
        } catch { /* not a socket.io event frame */ }
      });
    });
    page.on('request', req => {
      if (req.method() === 'GET' || req.method() === 'OPTIONS') return;
      const url = req.url();
      if (!/\/api\/|\/rooms|\/messages/.test(url)) return;
      let body = null;
      try { body = JSON.parse(req.postData() ?? ''); } catch { return; }
      if (body && typeof body === 'object') {
        this.lastWrite = { url, method: req.method(), headers: req.headers(), body };
      }
    });
    page.on('console', m => {
      if (m.type() === 'error') this.consoleErrors.push(m.text().slice(0, 200));
    });
    page.on('pageerror', e => this.consoleErrors.push(`pageerror: ${e.message.slice(0, 200)}`));
  }
  loc(testid, { contains, scope } = {}) {
    // `scope` narrows the search to inside a specific container (e.g. the badge
    // belonging to ONE room), so a stale element elsewhere can't satisfy it.
    const root = scope
      ? this.page.locator(tid(scope.testid), { hasText: scope.contains }).first()
      : this.page;
    return contains
      ? root.locator(tid(testid), { hasText: contains }).first()
      : root.locator(tid(testid)).first();
  }
}

// Scenario strings may reference a run-scoped room as "{room:base}".
const expand = (s, ctx) =>
  typeof s === 'string' ? s.replace(/\{room:([^}]+)\}/g, (_, b) => ctx.roomName(b)) : s;

// ─── Step execution ──────────────────────────────────────────────────────────

async function enterRoom(actor, roomName) {
  const item = actor.page.locator(tid('room-item'), { hasText: roomName }).first();
  await item.waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
  await item.click();
  const input = actor.loc('message-input');
  if (!(await input.isVisible().catch(() => false))) {
    await actor.page.waitForTimeout(750);
    // Apps may implement click-to-join then click-to-enter.
    if (!(await input.isVisible().catch(() => false))) await item.click();
  }
  await input.waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
}

async function runStep(step, actors, ctx) {
  // Cross-actor steps act on several actors at once, so they resolve first.
  if (step.do === 'expectOrderMatches') return expectOrderMatches(step, actors);
  if (step.do === 'sendConcurrently') {
    // Genuine concurrency: all senders fire without waiting for each other.
    // Apps legitimately rate-limit (the L1 spec requires spam prevention), so
    // each sender paces itself; concurrency comes from senders overlapping.
    await Promise.all(step.senders.map(s =>
      sendMany(actors.get(s.actor), s.prefix, s.count, s.delayMs ?? step.delayMs ?? 0)));
    return;
  }

  const actor = actors.get(step.actor);
  if (!actor) throw new Error(`unknown actor "${step.actor}"`);
  const page = actor.page;

  switch (step.do) {
    case 'setOffline': {
      await page.context().setOffline(step.offline !== false);
      await page.waitForTimeout(step.settleMs ?? 500);
      return;
    }
    case 'sendMany': return sendMany(actor, step.prefix, step.count, step.delayMs ?? 0);
    case 'expectAllPresent': {
      // Every message must be present exactly once: catches both loss and
      // duplication (optimistic insert plus broadcast echo renders twice).
      const within = step.within ?? 10000;
      const deadline = Date.now() + within;
      for (;;) {
        const counts = [];
        for (let i = 1; i <= step.count; i++) {
          counts.push(await actor.page.locator(tid('message-item'), { hasText: `${step.prefix}-${pad(i, step.count)}` }).count());
        }
        const missing = counts.filter(c => c === 0).length;
        const duplicated = counts.filter(c => c > 1).length;
        if (!missing && !duplicated) return;
        if (Date.now() > deadline) {
          throw new Error(`of ${step.count} "${step.prefix}" messages: ${missing} missing, ${duplicated} duplicated (after ${within}ms)`);
        }
        await actor.page.waitForTimeout(500);
      }
    }
    case 'register': {
      await page.locator(tid('name-input')).first().fill(`${step.name}-${ctx.scope}`);
      await page.locator(tid('name-submit')).first().click();
      await page.locator(tid('room-list')).first()
        .waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
      return;
    }
    case 'createRoom': {
      const roomName = ctx.roomName(step.room);
      const nameInput = page.locator(tid('room-name-input')).first();
      if (!(await nameInput.isVisible().catch(() => false))) {
        await page.locator(tid('room-create')).first().click();
      }
      await nameInput.fill(roomName);
      await page.locator(tid('room-name-submit')).first().click();
      // Must arrive via the app's live update path — no reload.
      await page.locator(tid('room-item'), { hasText: roomName }).first()
        .waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });
      return;
    }
    case 'enterRoom': return enterRoom(actor, ctx.roomName(step.room));
    case 'send': {
      const input = actor.loc('message-input');
      await input.fill(step.text);
      await input.press('Enter');
      return;
    }
    case 'typeInto': {
      const input = actor.loc('message-input');
      await input.click();
      await input.type(step.text, { delay: 40 });
      return;
    }
    case 'clearInput': {
      await actor.loc('message-input').fill('');
      return;
    }
    case 'click': {
      await actor.loc(step.testid).click({ timeout: step.within ?? DEFAULT_WITHIN });
      return;
    }
    case 'reload': {
      await page.reload({ waitUntil: 'domcontentloaded' });
      await page.waitForTimeout(step.settleMs ?? 2500);
      return;
    }
    case 'forgeWrite': {
      // Replay this actor's own write request with a field tampered. If the
      // app's write path carries no client-supplied identity at all (a
      // SpacetimeDB reducer call takes its identity from the connection), there
      // is nothing to forge and the step records `skipped`.
      const write = actor.lastWrite;
      if (!write) {
        // Nothing replayable over HTTP. If the app writes over WebSocket, judge
        // by whether that payload carries a client-supplied identity.
        const ws = actor.lastWsWrite;
        if (ws) {
          const idKey = Object.keys(ws.body).find(k => IDENTITY_FIELD.test(k));
          actor.forge = idKey
            ? { inconclusive: true, reason: `writes over WebSocket ("${ws.event}") carrying a client-supplied "${idKey}" — replay not attempted, treat as unverified` }
            : { structurallySafe: true, reason: `writes over WebSocket ("${ws.event}") with no client-supplied identity — the server must derive it from the connection` };
        } else {
          actor.forge = { inconclusive: true, reason: 'no write request observed at all — the test did not exercise anything' };
        }
        return;
      }

      const body = { ...write.body };
      const key = Object.keys(body).find(k => (step.field === 'room' ? ROOM_FIELD : IDENTITY_FIELD).test(k));
      if (!key) {
        actor.forge = { structurallySafe: true, reason: `write body carries no identity field (${Object.keys(body).join(',') || 'empty'}) — nothing to forge` };
        return;
      }

      let value = step.value;
      if (step.fromActor) {
        const victim = actors.get(step.fromActor);
        const vkey = Object.keys(victim.lastWrite?.body ?? {}).find(k => IDENTITY_FIELD.test(k));
        if (!vkey) {
          const vws = victim.lastWsWrite;
          const vwsKey = vws && Object.keys(vws.body).find(k => IDENTITY_FIELD.test(k));
          if (!vwsKey) { actor.forge = { structurallySafe: true, reason: `${step.fromActor} never exposes an identity to steal` }; return; }
          value = vws.body[vwsKey];
        } else value = victim.lastWrite.body[vkey];
      }
      body[key] = value;

      const contentKey = Object.keys(body).find(k => CONTENT_FIELD.test(k));
      if (contentKey && step.text) body[contentKey] = step.text;

      const res = await page.request.fetch(write.url, {
        method: write.method,
        headers: { 'content-type': 'application/json' },
        data: JSON.stringify(body),
      });
      actor.forge = { skipped: false, status: res.status(), accepted: res.ok(), tamperedField: key, value };
      await page.waitForTimeout(step.settleMs ?? 2000);
      return;
    }
    case 'expectForgeryRejected': {
      const f = actor.forge;
      if (!f) throw new Error('no forgeWrite ran before this assertion');
      // Only an ACCEPTED forgery is a failure here. "Could not replay" is not
      // evidence of safety, so it never scores a point on its own — the
      // universal DOM assertion that follows is what actually earns it.
      if (f.inconclusive || f.structurallySafe) return;
      if (f.accepted) {
        throw new Error(`server ACCEPTED a write with a tampered "${f.tamperedField}" (HTTP ${f.status}) — the client chooses who it is`);
      }
      return;
    }
    case 'scheduleMessage': {
      // The L2 contract names the toggle and the time input but not the submit
      // control, so fall back to a labelled button when no hook is present.
      const input = actor.loc('message-input');
      if (await input.count()) await input.fill(step.text);
      const toggle = actor.loc('schedule-toggle');
      if (await toggle.count()) await toggle.click({ timeout: DEFAULT_WITHIN }).catch(() => {});
      const when = actor.loc('schedule-time');
      await when.waitFor({ state: 'visible', timeout: DEFAULT_WITHIN });

      const type = (await when.getAttribute('type')) ?? 'text';
      const at = new Date(Date.now() + step.secondsAhead * 1000);
      const local = new Date(at.getTime() - at.getTimezoneOffset() * 60000).toISOString();
      if (type === 'datetime-local') await when.fill(local.slice(0, 16));
      else if (type === 'time') await when.fill(local.slice(11, 16));
      else await when.fill(String(step.secondsAhead));

      const submit = actor.loc('schedule-submit');
      if (await submit.count()) { await submit.click(); return; }
      const labelled = page.getByRole('button', { name: /schedule|send later|confirm/i }).first();
      if (await labelled.count()) { await labelled.click(); return; }
      await when.press('Enter');
      return;
    }
    case 'restartBackend': {
      // Restarts the app's backend process — the Express server, or the
      // SpacetimeDB host. Supplied by the caller because it is environment
      // specific; without it the step fails loudly rather than passing.
      if (!ctx.restartCmd) throw new Error('INCONCLUSIVE: no --restart-cmd supplied, backend was never restarted');
      const { execFileSync } = await import('node:child_process');
      try {
        execFileSync('bash', ['-c', ctx.restartCmd], { encoding: 'utf8', stdio: 'pipe', timeout: 120000 });
      } catch (err) {
        throw new Error(`backend restart command failed: ${(err.stdout || err.message || '').toString().trim().slice(-200)}`);
      }
      await page.waitForTimeout(step.settleMs ?? 10000);
      return;
    }
    case 'wait': return page.waitForTimeout(step.ms);
    case 'expect': return runExpect(actor, step, ctx);
    default: throw new Error(`unknown action "${step.do}"`);
  }
}

async function runExpect(actor, step, ctx = { roomName: x => x }) {
  const within = step.within ?? DEFAULT_WITHIN;
  const contains = expand(step.contains, ctx);
  const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
  const where = scope ? ` inside ${tid(scope.testid)} "${scope.contains}"` : '';
  const loc = actor.loc(step.testid, { contains, scope });

  if (step.absent) {
    // Poll until gone (or never present) — hidden OR detached both count.
    const deadline = Date.now() + within;
    for (;;) {
      const visible = await loc.isVisible().catch(() => false);
      if (!visible) return;
      if (Date.now() > deadline) {
        throw new Error(`${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}${where} still visible after ${within}ms`);
      }
      await actor.page.waitForTimeout(250);
    }
  }

  await loc.waitFor({ state: 'visible', timeout: within }).catch(() => {
    throw new Error(`${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}${where} not visible within ${within}ms`);
  });

  if (step.count !== undefined || step.maxCount !== undefined) {
    const all = scope
      ? actor.page.locator(tid(scope.testid), { hasText: scope.contains }).first().locator(tid(step.testid))
      : (contains ? actor.page.locator(tid(step.testid), { hasText: contains }) : actor.page.locator(tid(step.testid)));
    const n = await all.count();
    if (step.count !== undefined && n !== step.count) {
      throw new Error(`expected exactly ${step.count} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
    if (step.maxCount !== undefined && n > step.maxCount) {
      throw new Error(`expected at most ${step.maxCount} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
  }

  if (step.notContains) {
    const text = (await loc.innerText().catch(() => '')) || '';
    if (text.includes(step.notContains)) {
      throw new Error(`${tid(step.testid)} unexpectedly contains "${step.notContains}" (text: "${text.trim().slice(0, 80)}")`);
    }
  }
}

// Indices are zero-padded so "AA-1" cannot substring-match "AA-10".
const pad = (i, count) => String(i).padStart(String(count).length, '0');

async function sendMany(actor, prefix, count, delayMs) {
  const input = actor.loc('message-input');
  for (let i = 1; i <= count; i++) {
    await input.fill(`${prefix}-${pad(i, count)}`);
    await input.press('Enter');
    if (delayMs) await actor.page.waitForTimeout(delayMs);
  }
}

// Every client must observe the same messages in the same order. Express stacks
// broadcast with io.emit() AFTER awaiting the write, so the broadcast is not
// atomic with the commit and concurrent senders can interleave differently per
// client. A SpacetimeDB subscription update IS the commit, so it cannot diverge.
async function expectOrderMatches(step, actors) {
  const seqs = {};
  for (const name of step.actors) {
    const actor = actors.get(name);
    const texts = await actor.page.locator(tid('message-item')).allInnerTexts();
    seqs[name] = texts
      .map(t => (t.match(new RegExp(`${step.prefix}-\\d+`)) || [])[0])
      .filter(Boolean);
  }
  const [first, ...rest] = step.actors;
  for (const other of rest) {
    if (seqs[first].join('|') !== seqs[other].join('|')) {
      const diffAt = seqs[first].findIndex((v, i) => v !== seqs[other][i]);
      throw new Error(
        `message order differs between ${first} and ${other} at position ${diffAt}: ` +
        `${first} saw ${seqs[first].slice(Math.max(0, diffAt - 1), diffAt + 2).join(',')} / ` +
        `${other} saw ${seqs[other].slice(Math.max(0, diffAt - 1), diffAt + 2).join(',')}`
      );
    }
  }
}

// ─── Feature grading ─────────────────────────────────────────────────────────

async function gradeFeature(browser, feature, args, runCtx) {
  // Features share the app's DATABASE even though each gets fresh browser
  // contexts, so user and room names are scoped per feature — otherwise a
  // defect in one feature (e.g. a hijacked account) corrupts later setups.
  const scope = `${runCtx.runId}f${feature.id}`;
  const ctx = { ...runCtx, scope, roomName: base => `${base}-${scope}` };
  const actors = new Map();
  const contexts = [];
  const slug = `${args.label ?? 'run'}-f${feature.id}`;
  for (const name of feature.actors) {
    // Isolated storage per actor. Video is per-context, so each actor gets its
    // own recording — you can watch what every participant saw, side by side.
    const context = await browser.newContext(
      args.media ? { recordVideo: { dir: args.media, size: { width: 1280, height: 800 } } } : {}
    );
    if (args.trace) await context.tracing.start({ screenshots: true, snapshots: true });
    const page = await context.newPage();
    page.setDefaultTimeout(DEFAULT_WITHIN);
    await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
    actors.set(name, new Actor(name, page));
    contexts.push({ context, name, page });
  }

  const closeAll = async () => {
    for (const { context, name, page } of contexts) {
      if (args.trace) {
        await context.tracing.stop({ path: join(args.media ?? '.', `${slug}-${name}.trace.zip`) }).catch(() => {});
      }
      const video = args.media ? page.video() : null;
      await context.close();                     // video is only finalized on close
      if (video) {
        await video.saveAs(join(args.media, `${slug}-${name}.webm`)).catch(() => {});
        await video.delete().catch(() => {});     // drop Playwright's hashed original
      }
    }
  };

  const featureMax = feature.max ?? MAX_PER_FEATURE;
  const result = {
    id: feature.id, name: feature.name, score: 0, max: featureMax,
    criteria: [], caps: [], consoleErrors: [],
  };

  try {
    // Setup is not scored, but a failure makes the feature untestable (0).
    for (const step of feature.setup) {
      await runStep(step, actors, ctx);
    }
  } catch (err) {
    result.setupError = err.message;
    result.caps.push('setup-failed → 0');
    for (const c of feature.criteria) {
      result.criteria.push({ id: c.id, points: c.points, passed: false, detail: 'setup failed' });
    }
    await closeAll();
    return result;
  }

  let refreshDependent = false;
  for (const criterion of feature.criteria) {
    let passed = true, detail = null;
    try {
      for (const step of criterion.steps) await runStep(step, actors, ctx);
    } catch (err) {
      passed = false;
      detail = err.message;
      if (args.media) {
        const shotActor = actors.get(criterion.steps[criterion.steps.length - 1]?.actor) ?? actors.values().next().value;
        const shot = join(args.media, `${slug}-${criterion.id}.png`);
        await shotActor.page.screenshot({ path: shot, fullPage: true }).catch(() => {});
        result.screenshots = [...(result.screenshots ?? []), shot];
      }
      // Refresh probe: does the assertion pass once the page is reloaded?
      // If so the feature is refresh-dependent, not realtime.
      const failing = criterion.steps[criterion.steps.length - 1];
      if (failing?.do === 'expect' && !failing.absent) {
        try {
          const actor = actors.get(failing.actor);
          await actor.page.reload({ waitUntil: 'domcontentloaded' });
          await actor.page.waitForTimeout(2000);
          await runExpect(actor, { ...failing, within: 6000 });
          refreshDependent = true;
          detail += ' — PASSES AFTER RELOAD (refresh-dependent)';
        } catch { /* genuinely absent, not just refresh-dependent */ }
      }
    }
    result.criteria.push({ id: criterion.id, desc: criterion.desc, points: criterion.points, passed, detail });
    if (passed) result.score += criterion.points;
  }

  for (const actor of actors.values()) {
    for (const e of actor.consoleErrors) result.consoleErrors.push(`[${actor.name}] ${e}`);
  }

  // Caps, applied in severity order.
  if (feature.max === undefined && refreshDependent && result.score > 1) {
    result.caps.push('refresh-dependent → capped at 1');
    result.score = 1;
  }
  if (feature.max === undefined && result.consoleErrors.length && result.score > 2) {
    result.caps.push('console errors → capped at 2');
    result.score = 2;
  }

  await closeAll();
  if (args.media) result.videos = contexts.map(c => join(args.media, `${slug}-${c.name}.webm`));
  return result;
}

async function countExistingRooms(browser, args, runId) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    page.setDefaultTimeout(DEFAULT_WITHIN);
    await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.locator(tid('name-input')).first().fill(`preflight-${runId}`);
    await page.locator(tid('name-submit')).first().click();
    await page.locator(tid('room-list')).first().waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
    await page.waitForTimeout(1500);
    return await page.locator(tid('room-item')).count();
  } catch {
    return -1;                                     // couldn't determine
  } finally {
    await context.close();
  }
}

// ─── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const args = parseArgs(process.argv);
  const specPath = args.spec ? args.spec : join(ROOT, 'scenarios', `level-${String(args.level).padStart(2, '0')}.json`);
  let spec;
  try {
    spec = JSON.parse(readFileSync(specPath, 'utf8'));
  } catch {
    console.error(`No scenario spec at ${specPath}`);
    process.exit(2);
  }

  const features = args.feature ? spec.features.filter(f => f.id === args.feature) : spec.features;
  const runId = uniq();
  const ctx = { runId, roomName: base => `${base}-${runId}`, restartCmd: args.restartCmd };

  const browser = await chromium.launch({ headless: !args.headed });
  const report = {
    label: args.label ?? null, url: args.url, level: args.level, runId,
    total: 0, max: features.reduce((n, f) => n + (f.max ?? MAX_PER_FEATURE), 0), features: [],
  };

  // Preflight: grading a dirty database silently biases scores downward (a long
  // room/user list breaks assertions that pass on a clean app), so surface it
  // rather than letting it look like a real failure.
  report.environment = { preexistingRooms: await countExistingRooms(browser, args, runId) };
  if (report.environment.preexistingRooms > 0) {
    console.log(`WARNING: app already has ${report.environment.preexistingRooms} room(s) — ` +
      `scores are not comparable. Reset the database first ` +
      `(llm-sequential-upgrade/reset-app.sh <app-dir>).\n`);
  }

  for (const feature of features) {
    process.stdout.write(`Feature ${feature.id}: ${feature.name} ... `);
    const r = await gradeFeature(browser, feature, args, ctx);
    report.features.push(r);
    report.total += r.score;
    console.log(`${r.score}/${r.max}${r.caps.length ? ` (${r.caps.join('; ')})` : ''}`);
    for (const c of r.criteria.filter(c => !c.passed)) {
      console.log(`    FAIL ${c.id} — ${c.detail}`);
    }
  }

  await browser.close();

  console.log(`\nTOTAL ${report.total}/${report.max}`);
  if (args.out) {
    mkdirSync(dirname(args.out), { recursive: true });
    writeFileSync(args.out, JSON.stringify(report, null, 2));
    console.log(`Report written to ${args.out}`);
  }
}

main();
