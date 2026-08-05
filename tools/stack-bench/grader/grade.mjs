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
  constructor(name, page, context) {
    this.name = name;
    this.context = context;
    this.consoleErrors = [];
    this.attach(page);
  }
  attach(page) {
    this.page = page;
    // Record the app's own write requests so a test can replay one with a
    // tampered field. This adapts to whatever API the app happens to expose,
    // instead of the harness having to know its shape.
    this.lastWrite = null;
    this.lastWrites = {};   // by method: a toggle adds with POST and removes with DELETE,
                            // so replaying "the last write" can undo instead of redo
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
        const write = { url, method: req.method(), headers: req.headers(), body };
        this.lastWrite = write;
        this.lastWrites[req.method()] = write;
      }
    });
    page.on('console', m => {
      if (m.type() !== 'error') return;
      const text = m.text();
      // The browser logs a failed fetch as a console error, so an app that
      // correctly refuses something — a duplicate username, a wrong password —
      // looks like it has a bug. A 4xx is the server deliberately saying no;
      // that is the behaviour under test, not a defect. 5xx still counts, and so
      // does every genuine JavaScript error.
      if (/Failed to load resource.*status of 4\d\d/.test(text)) return;
      this.consoleErrors.push(text.slice(0, 200));
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


// ─── On-screen annotation ────────────────────────────────────────────────────
// Recording a run is only half useful if you cannot tell what it was doing. This
// paints the current feature, criterion and step onto each actor's page, so the
// video explains itself. The banner carries no test id and lives outside the app
// root, so scoped assertions cannot see it.

const OVERLAY_ID = '__stackbench_overlay';

async function annotate(actor, { feature, criterion, step, status } = {}) {
  if (!actor?.annotate) return;
  await actor.page.evaluate(({ id, feature, criterion, step, status, who }) => {
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement('div');
      el.id = id;
      el.style.cssText = [
        'position:fixed', 'inset:0 0 auto 0', 'z-index:2147483647',
        'font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace',
        'padding:6px 10px', 'pointer-events:none', 'white-space:pre',
        'background:rgba(12,12,16,.92)', 'color:#e8e8ef',
        'border-bottom:2px solid #4c8dff',
      ].join(';');
      document.documentElement.appendChild(el);
    }
    const colour = status === 'fail' ? '#ff5c5c' : status === 'pass' ? '#3ddc84' : '#4c8dff';
    el.style.borderBottomColor = colour;
    el.textContent = [
      `${who}   ${feature ?? ''}`,
      criterion ? `  ${status === 'fail' ? 'FAILED' : 'checking'}: ${criterion}` : '',
      step ? `  > ${step}` : '',
    ].filter(Boolean).join(String.fromCharCode(10));
  }, { id: OVERLAY_ID, feature, criterion, step, status, who: actor.name }).catch(() => {});
}

// A one-line description of a step, for the banner and the timeline.
function describeStep(step) {
  switch (step.do) {
    case 'signUp': return `sign up as ${step.name}${step.expectFailure ? ' (expected to fail)' : ''}`;
    case 'signIn': return `sign in as ${step.name}${step.expectFailure ? ' (expected to fail)' : ''}`;
    case 'register': return `register as ${step.name}`;
    case 'createRoom': return `create room "${step.room}"`;
    case 'enterRoom': return `enter room "${step.room}"`;
    case 'send': return `send "${step.text}"`;
    case 'sendMany': return `send ${step.count} messages`;
    case 'sendConcurrently': return `${step.senders.length} clients send at once`;
    case 'typeInto': return 'start typing';
    case 'clearInput': return 'stop typing';
    case 'click': return `click ${step.testid}`;
    case 'reload': return 'reload the page';
    case 'closeClient': return 'close the browser';
    case 'openClient': return 'reopen the browser';
    case 'setOffline': return step.offline === false ? 'reconnect' : 'go offline';
    case 'restartBackend': return 'restart the backend';
    case 'ensureRegistered': return 'sign back in if needed';
    case 'scheduleMessage': return `schedule "${step.text}" for ${step.secondsAhead}s ahead`;
    case 'wait': return `wait ${step.ms}ms`;
    case 'expect': return `expect ${step.absent ? 'no ' : ''}${step.testid}${step.contains ? ` containing "${step.contains}"` : ''}`;
    case 'expectAllPresent': return `expect all ${step.count} "${step.prefix}" messages exactly once`;
    case 'expectOrderMatches': return 'expect both clients agree on order';
    case 'expectForgeryRejected': return 'expect the forged write to be rejected';
    default: return step.do;
  }
}


// Replay a captured request as the app itself sent it. Rebuilding the headers
// from scratch drops whatever carries the session, so the server answers 401 and
// the test measures nothing. Hop-by-hop and length headers are dropped because
// they describe the original transfer.
function replayHeaders(write, overrides = {}) {
  const out = { ...(write.headers ?? {}), 'content-type': 'application/json', ...overrides };
  for (const k of Object.keys(out)) {
    if (/^(content-length|host|connection|transfer-encoding|accept-encoding)$/i.test(k)) delete out[k];
  }
  return out;
}

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
  if (step.do === 'expectAgreement') return expectAgreement(step, actors, ctx);
  if (step.do === 'replayConcurrently') {
    // Browser clicks arrive milliseconds apart and each request finishes in
    // less, so a race never happens. Replaying each actor's own captured write
    // through Promise.all issues them together and actually overlaps them.
    const method = step.method ?? 'POST';
    const pending = step.actors
      .map(name => {
        const a = actors.get(name);
        return { actor: a, write: a?.lastWrites?.[method] ?? a?.lastWrite };
      })
      .filter(x => x.write);
    if (pending.length < 2) {
      throw new Error('INCONCLUSIVE: fewer than two write requests were captured, so nothing was contended. ' +
        'This backend may not write over HTTP.');
    }
    await Promise.all(pending.map(({ actor, write }) =>
      actor.page.request.fetch(write.url, {
        method: write.method,
        headers: replayHeaders(write),
        data: JSON.stringify(write.body),
      }).catch(() => {})));
    await pending[0].actor.page.waitForTimeout(step.settleMs ?? 3000);
    return;
  }
  if (step.do === 'clickConcurrently') {
    // Genuine simultaneity: every actor clicks without waiting for the others.
    // `targets` gives each actor its own element, so they compete for a shared
    // limit instead of all pressing the same button.
    const targets = step.targets ?? step.actors.map(actor => ({ actor }));
    await Promise.all(targets.map(t => {
      const a = actors.get(t.actor);
      const where = t.in ?? step.in;
      const scope = where ? { testid: where.testid, contains: expand(where.contains, ctx) } : undefined;
      return a.loc(step.testid, { scope }).click({ timeout: step.within ?? DEFAULT_WITHIN }).catch(() => {});
    }));
    await actors.values().next().value.page.waitForTimeout(step.settleMs ?? 3000);
    return;
  }
  if (step.do === 'restartBackend') {
    if (!ctx.restartCmd) throw new Error('INCONCLUSIVE: no --restart-cmd supplied, backend was never restarted');
    const { execFileSync } = await import('node:child_process');
    try {
      // stdio 'ignore': the restarted server is a backgrounded descendant that
      // keeps an inherited pipe open, so 'pipe' never returns even though the
      // script exits cleanly. The script does its own readiness wait.
      execFileSync('bash', ['-c', ctx.restartCmd], { stdio: 'ignore', timeout: 300000 });
    } catch (err) {
      // Exit 3 means the restart was refused as unsafe (a shared host), not that
      // the app failed. Report it as untestable rather than as a defect.
      if (err.status === 3) throw new Error('INCONCLUSIVE: backend restart refused — no benchmark-owned instance available');
      throw new Error(`backend restart failed: ${(err.stdout || err.message || '').toString().trim().slice(-200)}`);
    }
    // Not page-bound: clients may be closed across the restart.
    await new Promise(r => setTimeout(r, step.settleMs ?? 10000));
    return;
  }
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
    case 'signUp': {
      // Passwords are derived from the scoped name so a later signIn can
      // reproduce them without the scenario restating the credential.
      const user = `${step.name}-${ctx.scope}`;
      const pass = step.password ?? `pw-${user}`;
      await page.locator(tid('signup-username')).first().fill(user);
      await page.locator(tid('signup-password')).first().fill(pass);
      await page.locator(tid('signup-submit')).first().click();
      if (step.expectFailure) {
        await page.waitForTimeout(step.settleMs ?? 2000);
        return;
      }
      await page.locator(tid('current-user')).first()
        .waitFor({ state: 'visible', timeout: DEFAULT_WITHIN * 2 });
      return;
    }
    case 'signIn': {
      const user = `${step.name}-${ctx.scope}`;
      const pass = step.password ?? `pw-${user}`;
      const toggle = actor.loc('signin-toggle');
      if (await toggle.count()) await toggle.click({ timeout: DEFAULT_WITHIN }).catch(() => {});
      await page.locator(tid('signin-username')).first().fill(user);
      await page.locator(tid('signin-password')).first().fill(pass);
      await page.locator(tid('signin-submit')).first().click();
      if (step.expectFailure) {
        await page.waitForTimeout(step.settleMs ?? 2000);
        return;
      }
      await page.locator(tid('current-user')).first()
        .waitFor({ state: 'visible', timeout: DEFAULT_WITHIN * 2 });
      return;
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
      const scope = step.in
        ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) }
        : undefined;
      await actor.loc(step.testid, { scope }).click({ timeout: step.within ?? DEFAULT_WITHIN });
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
        headers: replayHeaders(write),
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
      // A dedicated compose field inside the scheduling UI takes precedence.
      const schedInput = actor.page.locator('[data-testid="schedule-message-input"], [data-testid="schedule-text"]').first();
      if (await schedInput.count()) await schedInput.fill(step.text);

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
    case 'ensureSignedIn':
    case 'ensureRegistered': {
      // Some apps drop the session on reload (scored separately as an
      // invariant). Re-register so THIS test measures scheduling durability
      // rather than re-measuring session persistence.
      const nameInput = page.locator(tid('signup-username')).first();
      if (await nameInput.isVisible().catch(() => false)) {
        const user = `${step.name}-${ctx.scope}`;
        await nameInput.fill(user);
        await page.locator(tid('signup-password')).first().fill(`pw-${user}`);
        await page.locator(tid('signup-submit')).first().click();
        await page.locator(tid('room-list')).first().waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
        await page.waitForTimeout(step.settleMs ?? 1500);
      }
      return;
    }
    case 'closeClient': {
      // A client that stays connected through a backend restart auto-reconnects
      // before anything else can happen. Closing it first models the ordinary
      // case — the user is not sitting on the page while you deploy — and keeps
      // this test about scheduling rather than reconnect behaviour, which
      // invariant 103 scores separately.
      await page.close();
      return;
    }
    case 'openClient': {
      const fresh = await actor.context.newPage();
      fresh.setDefaultTimeout(DEFAULT_WITHIN);
      actor.attach(fresh);
      await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
      await fresh.waitForTimeout(step.settleMs ?? 4000);
      return;
    }
    case 'expectStable': {
      // Read the same element several times: its text must not change while
      // nothing is happening. Catches unstable sorts and re-render churn that a
      // single assertion sails straight past.
      const loc = actor.loc(step.testid, { contains: expand(step.contains, ctx) });
      await loc.waitFor({ state: 'visible', timeout: step.within ?? DEFAULT_WITHIN });
      const seen = [];
      for (let i = 0; i < (step.samples ?? 4); i++) {
        seen.push(((await loc.innerText().catch(() => '')) || '').trim());
        await page.waitForTimeout(step.intervalMs ?? 700);
      }
      const distinct = [...new Set(seen)];
      if (distinct.length > 1) {
        throw new Error(`${tid(step.testid)} changed while idle: ${distinct.map(t => JSON.stringify(t.slice(0, 50))).join(' then ')}`);
      }
      return;
    }
    case 'freshClient': {
      // Open a brand-new browser and look at the same thing. Anything the acting
      // client shows but a fresh one cannot see never reached the server —
      // optimistic inserts that were never confirmed, counts incremented only
      // locally, state that lives in one browser's memory.
      const context = await actor.page.context().browser().newContext();
      const fresh = await context.newPage();
      fresh.setDefaultTimeout(DEFAULT_WITHIN);
      await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
      const observer = new Actor(`${actor.name}-fresh`, fresh, context);
      observer.annotate = actor.annotate;
      actors.set(`${step.actor}-fresh`, observer);
      // Registered for teardown; it is not one of the feature's declared actors.
      ctx.extraContexts?.push({ context, name: `${step.actor}-fresh`, page: fresh });
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

// Every client must end up seeing the same thing. A single-actor assertion says
// "Alice sees 20" and misses the actual defect, which is Bob seeing 19 — exactly
// what non-atomic broadcast produces.
async function expectAgreement(step, actors, ctx) {
  const contains = expand(step.contains, ctx);
  const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
  const deadline = Date.now() + (step.within ?? 10000);
  let seen = {};
  for (;;) {
    seen = {};
    for (const name of step.actors) {
      const loc = actors.get(name).loc(step.testid, { contains, scope });
      seen[name] = ((await loc.innerText().catch(() => '<missing>')) || '<missing>').trim();
    }
    if (new Set(Object.values(seen)).size === 1) return;
    if (Date.now() > deadline) {
      throw new Error(`clients disagree on ${tid(step.testid)}: ` +
        Object.entries(seen).map(([k, v]) => `${k} sees ${JSON.stringify(v.slice(0, 40))}`).join(', '));
    }
    await actors.get(step.actors[0]).page.waitForTimeout(500);
  }
}

// ─── Feature grading ─────────────────────────────────────────────────────────

async function gradeFeature(browser, feature, args, runCtx) {
  // Features share the app's DATABASE even though each gets fresh browser
  // contexts, so user and room names are scoped per feature — otherwise a
  // defect in one feature (e.g. a hijacked account) corrupts later setups.
  const scope = `${runCtx.runId}f${feature.id}`;
  const extraContexts = [];
  const ctx = { ...runCtx, scope, roomName: base => `${base}-${scope}`, extraContexts };
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
    const actor = new Actor(name, page, context);
    actor.annotate = Boolean(args.media);
    actors.set(name, actor);
    contexts.push({ context, name, page });
  }

  const closeAll = async () => {
    for (const { context, name, page } of [...contexts, ...extraContexts]) {
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
      await annotate(actors.get(step.actor), { feature: feature.name, criterion: 'setup', step: describeStep(step) });
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
      for (const step of criterion.steps) {
        await annotate(actors.get(step.actor) ?? actors.values().next().value,
          { feature: feature.name, criterion: criterion.id, step: describeStep(step) });
        await runStep(step, actors, ctx);
      }
      for (const a of actors.values()) {
        await annotate(a, { feature: feature.name, criterion: criterion.id, step: 'passed', status: 'pass' });
      }
    } catch (err) {
      passed = false;
      detail = err.message;
      if (args.media) {
        for (const a of actors.values()) {
          await annotate(a, { feature: feature.name, criterion: criterion.id, step: err.message.slice(0, 120), status: 'fail' });
        }
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
  const ctx = { runId, roomName: base => `${base}-${runId}`, restartCmd: args.restartCmd, url: args.url };

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
      `(stack-bench/reset-db.sh <backend> <app-dir>).\n`);
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
