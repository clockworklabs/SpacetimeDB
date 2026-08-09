#!/usr/bin/env node
// Stack Bench functional grader.
//
// Executes versioned scenario specs against a running app using multiple
// isolated browser contexts (one per actor), and scores each feature from
// observed behavior only — one point per criterion it passes. Mechanically
// enforces the rules human graders applied (features with an explicit `max`,
// i.e. the invariants, opt out of the caps and are scored purely per-criterion):
//   - JS console errors during a feature cap it at 2
//   - a feature that only works after a reload caps at 1
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
      case '--app': args.app = argv[++i]; break;
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

// Which requests count as writes worth capturing for replay and forgery. The
// default covers chat's routes; a scenario spec can widen it for an application
// whose endpoints are named differently (`writeUrlPattern`).
const DEFAULT_WRITE_URL = '\\/api\\/|\\/rooms|\\/messages';
let WRITE_URL_RE = new RegExp(DEFAULT_WRITE_URL);

const IDENTITY_FIELD = /^(user_?id|sender_?id|author_?id|from_?user|identity)$/i;
const CONTENT_FIELD = /^(content|text|message|body|msg)$/i;
const ROOM_FIELD = /^(room_?id|channel_?id|conversation_?id)$/i;

class Actor {
  constructor(name, page, context) {
    this.name = name;
    this.context = context;
    this.consoleErrors = [];
    // Everything this client is SENT, whatever the transport: WebSocket frames
    // (text or binary) and HTTP response bodies. Privacy is a property of what
    // reaches a browser, not of what that browser chooses to draw — an app that
    // ships every message to every client and hides the wrong ones in React has
    // no privacy at all, and this buffer is the only place that shows it.
    this.received = [];
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
    this.writes = [];       // every write, so another actor can replay a privileged one
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
      // Binary frames are decoded as UTF-8 too: a binary wire format still
      // carries message text as inline UTF-8 bytes, so a substring search finds
      // it without the harness knowing the encoding.
      ws.on('framereceived', f => this.record(f.payload));
    });
    page.on('request', req => {
      if (req.method() === 'GET' || req.method() === 'OPTIONS') return;
      const url = req.url();
      if (!WRITE_URL_RE.test(url)) return;
      let body = null;
      try { body = JSON.parse(req.postData() ?? ''); } catch { /* bodyless, e.g. a DELETE */ }
      // Forging needs a body to tamper with; replaying does not — a privileged
      // action is often a bare DELETE whose meaning is entirely in the URL.
      const write = { url, method: req.method(), headers: req.headers(), body };
      this.writes.push(write);
      if (this.writes.length > 200) this.writes.shift();
      if (body && typeof body === 'object') {
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
    page.on('response', async res => {
      const type = res.headers()['content-type'] ?? '';
      // Data only. Scripts and markup are served as text/* too, and a Vite
      // bundle would bury the buffer in megabytes of application source.
      if (!/(application\/json|application\/x-ndjson|text\/event-stream|text\/plain)/.test(type)) return;
      try { this.record(await res.text()); } catch { /* body gone, or page closed */ }
    });
  }
  record(payload) {
    const text = typeof payload === 'string' ? payload : Buffer.from(payload).toString('utf8');
    if (!text) return;
    this.received.push(text.slice(0, 200_000));
    if (this.received.length > 2000) this.received.shift();
  }
  wasSent(needle) {
    return this.received.some(chunk => chunk.includes(needle));
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

// ─── Retargeting a replayed request ──────────────────────────────────────────

// Match a token only on its own boundaries, so swapping user 7 for user 3 in
// /rooms/17/members/7 cannot silently rewrite the room.
const tokenRe = t => new RegExp(`(?<![A-Za-z0-9_-])${t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_-])`, 'g');
const mentions = (s, t) => tokenRe(t).test(s);
const swapToken = (s, from, to) => s.replace(tokenRe(from), to);

// Apps address a user by id, not by the name shown on screen. The id is already
// in traffic the harness recorded — find it next to the name the app sent back.
// EVERY nearby candidate is returned, not the first: in a member list the id
// sitting closest to a name is frequently the previous entry's, and picking it
// made the whole replay quietly unverifiable.
const ID_KEY = /"(?:_?id|user_?id|userId|memberId|member_id)"\s*:\s*"?([A-Za-z0-9_-]{1,64})"?/g;

function discoverIds(actor, username) {
  const found = new Set();
  const quoted = `"${username}"`;
  for (const chunk of actor.received) {
    let i = chunk.indexOf(quoted);
    while (i !== -1) {
      for (const m of chunk.slice(Math.max(0, i - 200), i + 200).matchAll(ID_KEY)) found.add(m[1]);
      i = chunk.indexOf(quoted, i + 1);
    }
  }
  return [...found];
}

// The credentials to replay WITH. Reusing the privileged user's token proves
// nothing, and replaying with none at all is worse: the server answers 401 and
// the test reads as "authorization works" when it only showed that anonymous
// requests are refused.
const AUTH_HEADER = /^(authorization|cookie|x-auth-token|x-session|x-token|x-user)/i;

function authFor(actor) {
  for (const w of [...actor.writes].reverse()) {
    const h = Object.entries(w.headers ?? {}).filter(([k, v]) => AUTH_HEADER.test(k) && v);
    if (h.length) return Object.fromEntries(h);
  }
  // Nothing signed-in was ever sent — recover the token the server handed back.
  for (const chunk of actor.received) {
    const m = chunk.match(/"(?:token|accessToken|access_token|jwt|sessionToken)"\s*:\s*"([^"]{8,})"/);
    if (m) return { authorization: `Bearer ${m[1]}` };
  }
  return null;
}

// Scenario strings may reference a run-scoped room as "{room:base}".
// "{room:base}" is a run-scoped room. "{user:Name}" is the scoped account name
// signUp actually created — scenarios name people as Alice and Target, but the
// app only ever sees "Alice-<scope>", so anything matched against real traffic
// has to be expanded the same way.
const expand = (s, ctx) =>
  typeof s === 'string'
    ? s.replace(/\{room:([^}]+)\}/g, (_, b) => ctx.roomName(b))
       // Scoped account names are alphanumeric ON PURPOSE. This used to join
       // the name and the scope with a hyphen, so the harness signed up as
       // "Alice-l1features". The level spec never states which characters a
       // username must accept, so an app validating them as letters, digits and
       // underscore — GitHub's rule, an ordinary choice — rejected every account
       // the harness tried to create, and all 49 criteria reported "setup
       // failed" for a defensible implementation. uniq() is base36, so dropping
       // the separator leaves the identifier alphanumeric, which every
       // reasonable rule accepts. Room names are unaffected: they are display
       // text, and their bases ({room:room-a}) already contain hyphens.
       .replace(/\{user:([^}]+)\}/g, (_, n) => `${n}${ctx.scope}`)
    : s;


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
    case 'fill': return `type "${step.text}" into ${step.testid}`;
    case 'pressKey': return `press ${step.key ?? 'Escape'}`;
    case 'reload': return 'reload the page';
    case 'closeClient': return 'close the browser';
    case 'openClient': return 'reopen the browser';
    case 'setOffline': return step.offline === false ? 'reconnect' : 'go offline';
    case 'restartBackend': return 'restart the backend';
    case 'ensureRegistered': return 'sign back in if needed';
    case 'scheduleMessage': return `schedule "${step.text}" for ${step.secondsAhead}s ahead`;
    case 'wait': return `wait ${step.ms}ms`;
    case 'expect': return `expect ${step.absent ? 'no ' : ''}${step.testid}${step.contains ? ` containing "${step.contains}"` : ''}`;
    case 'recordNumber': return `note the current ${step.testid}`;
    case 'expectNumber': {
      const want = [
        step.relativeTo !== undefined ? `has risen by ${step.plus ?? 0}` : null,
        step.equals !== undefined ? `is ${step.equals}` : null,
        step.atLeast !== undefined ? `is at least ${step.atLeast}` : null,
        step.atMost !== undefined ? `is at most ${step.atMost}` : null,
      ].filter(Boolean).join(' and ') || 'is a number';
      return `expect ${step.testid} ${want}`;
    }
    case 'expectAgreement': return `expect all clients agree on ${step.testid}`;
    case 'expectActorsWith': {
      const parts = [
        step.equals !== undefined ? `exactly ${step.equals} of ${step.actors.length} actors have` : `actors have`,
        `${step.testid}${step.contains ? ` containing "${step.contains}"` : ''}`,
        step.maxEach !== undefined ? `and none has more than ${step.maxEach}` : null,
      ].filter(Boolean);
      return `expect ${parts.join(' ')}`;
    }
    case 'race': return `two things happen at once (${(step.branches ?? []).length} branches)`;
    case 'runScript': return `run the app's ${step.script}${step.args?.length ? ` ${step.args.join(' ')}` : ''}`;
    case 'expectElementCount': return `expect exactly ${step.equals} ${step.testid}${step.contains ? ` containing "${step.contains}"` : ''}`;
    case 'expectAllPresent': return `expect all ${step.count} "${step.prefix}" messages exactly once`;
    case 'expectOrderMatches': return 'expect both clients agree on order';
    case 'expectForgeryRejected': return 'expect the forged write to be rejected';
    case 'expectNotReceived': return `expect "${step.contains}" is never sent to this client`;
    case 'replayAs': return `replay ${step.from}'s "${step.match}" request as this actor`;
    case 'expectReplayRejected': return 'expect the server refuses the replayed request';
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

// How many of these people ended up with the thing — and did anyone get it
// twice? A contention criterion is about the population, not one participant:
// "exactly three of six customers got an order" cannot be expressed by asking
// one customer about their own orders. Asking only actor `a` whether it has at
// most one order passes when NOBODY got one, which is the exact failure an
// oversell test must catch, so that phrasing was scoring nothing.
async function expectActorsWith(step, actors, ctx) {
  const contains = expand(step.contains, ctx);
  const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
  const counts = [];
  for (const name of step.actors) {
    const actor = actors.get(name);
    if (!actor) throw new Error(`expectActorsWith: no actor "${name}"`);
    // Give the UI the same settling budget a single expect would get, then
    // count. A zero here is a real zero, not an impatient one.
    const loc = actor.loc(step.testid, { contains, scope });
    await loc.waitFor({ state: 'visible', timeout: step.within ?? DEFAULT_WITHIN }).catch(() => {});
    const all = scope
      ? actor.page.locator(tid(scope.testid), { hasText: scope.contains }).first().locator(tid(step.testid))
      : (contains ? actor.page.locator(tid(step.testid), { hasText: contains }) : actor.page.locator(tid(step.testid)));
    counts.push([name, await all.count().catch(() => 0)]);
  }

  const held = counts.filter(([, n]) => n > 0);
  const detail = counts.map(([n, c]) => `${n}=${c}`).join(' ');
  if (step.equals !== undefined && held.length !== step.equals) {
    throw new Error(`expected exactly ${step.equals} actor(s) with ${tid(step.testid)}` +
      `${contains ? ` containing "${contains}"` : ''}, found ${held.length} (${detail})`);
  }
  if (step.maxEach !== undefined) {
    const greedy = counts.filter(([, n]) => n > step.maxEach);
    if (greedy.length) {
      throw new Error(`${greedy.map(([n, c]) => `${n} has ${c}`).join(', ')} ` +
        `— no actor may hold more than ${step.maxEach} (${detail})`);
    }
  }
}

async function runStep(step, actors, ctx) {
  // Cross-actor steps act on several actors at once, so they resolve first.
  if (step.do === 'expectOrderMatches') return expectOrderMatches(step, actors);
  if (step.do === 'expectAgreement') return expectAgreement(step, actors, ctx);
  if (step.do === 'expectActorsWith') return expectActorsWith(step, actors, ctx);
  if (step.do === 'replayConcurrently') {
    // Browser clicks arrive milliseconds apart and each request finishes in
    // less, so a race never happens. Replaying each actor's own captured write
    // through Promise.all issues them together and actually overlaps them.
    const method = step.method ?? 'POST';
    // `lastWrites` only holds requests that carried a JSON body, so a bodyless
    // action — a checkout, a "confirm" — is never eligible and the replay
    // silently races the wrong request instead. `match` picks the intended one
    // out of the full capture by URL, the same way replayAs does.
    const pick = a => {
      if (!a) return undefined;
      if (step.match) {
        return [...(a.writes ?? [])].reverse()
          .find(w => w.method === method && w.url.includes(step.match));
      }
      return a.lastWrites?.[method] ?? a.lastWrite;
    };
    const pending = step.actors.map(name => ({ actor: actors.get(name), write: pick(actors.get(name)) }))
      .filter(x => x.write);
    if (pending.length < 2) {
      throw new Error(`INCONCLUSIVE: fewer than two ${step.match ? `"${step.match}" ` : ''}write requests were captured, ` +
        'so nothing was contended. This backend may not write over HTTP, or the request carried no JSON body ' +
        '(only bodied writes reach lastWrites — pass `match` to select by URL instead).');
    }
    // A replay that the server refused looks identical to one it accepted if the
    // error is swallowed — and then an assertion about the result is really an
    // assertion about a request that never took effect. Record what came back.
    const replies = await Promise.all(pending.map(({ actor, write }) =>
      actor.page.request.fetch(write.url, {
        method: write.method,
        headers: replayHeaders(write),
        data: write.body === undefined || write.body === null ? undefined : JSON.stringify(write.body),
      }).then(r => r.status(), err => `error: ${String(err.message).split('\n')[0]}`)));
    // What matters is that both requests REACHED the server together — not what
    // it decided. Refusing the second is often the correct answer (a cart that
    // has already been checked out), and demanding two successes would mark a
    // correctly-behaving app inconclusive. Only a transport failure means the
    // race never happened.
    const answered = replies.filter(s => typeof s === 'number');
    if (answered.length < 2) {
      throw new Error(`INCONCLUSIVE: only ${answered.length} of ${pending.length} replayed ` +
        `${step.match ?? 'write'} requests reached the server (responses: ${replies.join(', ')}), ` +
        'so the two never contended.');
    }
    await pending[0].actor.page.waitForTimeout(step.settleMs ?? 3000);
    return;
  }
  if (step.do === 'clickConcurrently') {
    // Genuine simultaneity: every actor clicks without waiting for the others.
    // `targets` gives each actor its own element, so they compete for a shared
    // limit instead of all pressing the same button.
    const targets = step.targets ?? step.actors.map(actor => ({ actor }));
    // A click that never landed and a write the server lost look identical in
    // the result — the cart is simply short one item. Swallowing click errors
    // therefore turns a UI problem into a fabricated concurrency defect, so a
    // click that fails to dispatch is reported as a broken test, not a finding.
    const outcomes = await Promise.all(targets.map(t => {
      const a = actors.get(t.actor);
      const where = t.in ?? step.in;
      const scope = where ? { testid: where.testid, contains: expand(where.contains, ctx) } : undefined;
      return a.loc(step.testid, { scope }).click({ timeout: step.within ?? DEFAULT_WITHIN })
        .then(() => null, err => `${t.actor}: ${String(err.message).split('\n')[0]}`);
    }));
    const failed = outcomes.filter(Boolean);
    if (failed.length) {
      throw new Error(`INCONCLUSIVE: ${failed.length} of ${targets.length} concurrent clicks on ` +
        `${tid(step.testid)} never dispatched, so nothing was actually contended — ${failed.join(' | ')}`);
    }
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
  if (step.do === 'race') {
    // Two things happening at the same time, on purpose: one actor opening a
    // panel while another joins, a fetch racing the mutation it will miss.
    // Each branch runs its steps in order; the branches themselves overlap.
    // This is where fetch-then-merge architectures drop or duplicate rows —
    // a sequential test can never catch it, because sequencing is the bug's
    // absence.
    await Promise.all((step.branches ?? []).map(async branch => {
      for (const s of branch) await runStep(s, actors, ctx);
    }));
    await new Promise(r => setTimeout(r, step.settleMs ?? 2000));
    return;
  }
  if (step.do === 'runScript') {
    // Runs a script THE APP ITSELF shipped, from the app directory — the spec
    // requires a back-office script that writes directly to the database, the
    // way a cron job, an ETL, or another team's service would. The harness
    // knows nothing about the app's schema; the script is where that knowledge
    // lives. A missing script is a genuine failure (the spec asked for it),
    // not an INCONCLUSIVE.
    if (!ctx.appDir) throw new Error('INCONCLUSIVE: grader was not told the app directory (--app)');
    const { execFileSync } = await import('node:child_process');
    const { existsSync } = await import('node:fs');
    const { join } = await import('node:path');
    const path = join(ctx.appDir, step.script);
    if (!existsSync(path)) throw new Error(`the app does not ship ${step.script} — the spec requires it`);
    try {
      // Args expand the same tokens steps do — usernames are scope-suffixed,
      // so a raw name would purge a user that does not exist.
      execFileSync('node', [path, ...(step.args ?? []).map(a => expand(a, ctx))],
        { cwd: ctx.appDir, encoding: 'utf8', stdio: 'pipe', timeout: step.timeoutMs ?? 60000 });
    } catch (err) {
      throw new Error(`${step.script} failed: ${((err.stdout ?? '') + (err.stderr ?? '')).trim().slice(-200) || err.message}`);
    }
    await new Promise(r => setTimeout(r, step.settleMs ?? 3000));
    return;
  }
  if (step.do === 'stopAppServer' || step.do === 'startAppServer') {
    // The deploy window: a write lands while the app tier is down, and the app
    // must converge once it returns. Reuses the restart command with a mode
    // argument; on spacetime both are no-ops because there is no app tier —
    // which is the measurement, not an exemption.
    if (!ctx.restartCmd) throw new Error('INCONCLUSIVE: no --restart-cmd supplied, cannot control the app server');
    const { execFileSync } = await import('node:child_process');
    const mode = step.do === 'stopAppServer' ? 'stop' : 'start';
    try {
      execFileSync('bash', ['-c', `${ctx.restartCmd} ${mode}`], { stdio: 'ignore', timeout: 300000 });
    } catch (err) {
      if (err.status === 3) throw new Error('INCONCLUSIVE: app-server control refused on this host');
      throw new Error(`could not ${mode} the app server: ${(err.stdout || err.message || '').toString().trim().slice(-160)}`);
    }
    await new Promise(r => setTimeout(r, step.settleMs ?? (mode === 'start' ? 8000 : 2000)));
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
    case 'expectElementCount': {
      // Exactly-N, not at-least-N: the classic enumeration-during-mutation bug
      // is a fetched list MERGED with a live event for the same row, which
      // renders twice. `expect` can only say the row is there; this says it is
      // there once.
      const within = step.within ?? 10000;
      const deadline = Date.now() + within;
      const root = step.in
        ? page.locator(tid(step.in.testid), step.in.contains ? { hasText: step.in.contains } : {}).first()
        : page;
      for (;;) {
        const n = await root.locator(tid(step.testid), step.contains ? { hasText: step.contains } : {}).count();
        if (n === step.equals) return;
        if (Date.now() > deadline) {
          throw new Error(`expected exactly ${step.equals} ${tid(step.testid)}`
            + `${step.contains ? ` containing "${step.contains}"` : ''}, saw ${n} (after ${within}ms)`);
        }
        await page.waitForTimeout(400);
      }
    }
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
      // `exact` opts out of scoping, for an account the application seeds
      // under a fixed name rather than one this run creates.
      const user = step.exact ? step.name : `${step.name}${ctx.scope}`;
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
      const user = step.exact ? step.name : `${step.name}${ctx.scope}`;
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
      // Level 2 rooms can be private; the toggle is part of the creation
      // surface, so it belongs to this verb rather than to a separate click
      // that would race the form closing.
      if (step.private) await page.locator(tid('room-private-toggle')).first().click();
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
      // `contains` picks WHICH of a repeated testid to click — a member row, a
      // friend entry — the same way expect narrows what it looks at.
      await actor.loc(step.testid, { contains: expand(step.contains, ctx), scope })
        .click({ timeout: step.within ?? DEFAULT_WITHIN });
      if (step.settleMs) await page.waitForTimeout(step.settleMs);
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
    case 'replayAs': {
      // Take a privileged request another actor really made and re-issue it as
      // THIS actor, with this actor's own credentials. Hiding the control is not
      // authorization; the server has to say no to the request itself.
      const source = actors.get(step.from);
      if (!source) throw new Error(`replayAs: no actor "${step.from}"`);
      const needle = expand(step.match, ctx).toLowerCase();
      const write = [...source.writes].reverse()
        .find(w => `${w.method} ${w.url} ${JSON.stringify(w.body)}`.toLowerCase().includes(needle));

      if (!write) {
        // Either the app writes over its live connection (identity comes from
        // the connection, so there is nothing to re-issue) or the privileged
        // action never happened. Neither proves the server is safe, so this
        // records as unverified and the criterion rests on its DOM assertions.
        actor.replay = { inconclusive: true, reason: source.lastWsWrite
          ? `${step.from} writes over WebSocket ("${source.lastWsWrite.event}") — identity comes from the connection, replay not attempted`
          : `no HTTP write from ${step.from} matching "${step.match}"` };
        return;
      }

      // Swap in this actor's own credentials — otherwise the replay carries the
      // privileged user's token and proves nothing about who may do what.
      const mine = authFor(actor);
      if (!mine) {
        actor.replay = { inconclusive: true, reason: `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused` };
        return;
      }
      const creds = { ...mine };
      for (const k of Object.keys(write.headers)) {
        if (AUTH_HEADER.test(k) && !(k in creds)) creds[k] = '';
      }

      // The replay must name a DIFFERENT victim than the original request, or an
      // idempotent server could accept a no-op and look guilty. The target may
      // live in the URL or in the body, so swap across both.
      let url = write.url;
      let data = write.body === null ? undefined : JSON.stringify(write.body);
      if (step.swap) {
        const find = expand(step.swap.find, ctx), to = expand(step.swap.with, ctx);
        let a = find, b = to;
        if (!mentions(url, a) && !mentions(data ?? '', a)) {
          // Most apps address a user by id, not by display name. Scenarios stay
          // written in names; the harness resolves them to whatever ids the app
          // itself used, read out of traffic it has already seen.
          const candidates = [...discoverIds(source, find), ...discoverIds(actor, find)];
          const ra = candidates.find(c => mentions(url, c) || mentions(data ?? '', c));
          const rb = [...discoverIds(source, to), ...discoverIds(actor, to)].find(c => c !== ra);
          if (ra && rb) { a = ra; b = rb; }
          else {
            actor.replay = { inconclusive: true, reason: `neither "${find}" nor an id resolved for it appears in ${write.method} ${write.url} — cannot retarget the replay` };
            return;
          }
        }
        url = swapToken(url, a, b);
        if (data) data = swapToken(data, a, b);
      }

      const res = await page.request.fetch(url, {
        method: write.method,
        headers: replayHeaders(write, creds),
        ...(data === undefined ? {} : { data }),
      }).catch(e => ({ status: () => 0, ok: () => false, error: e.message }));

      actor.replay = { accepted: res.ok(), status: res.status(), url, method: write.method };
      await page.waitForTimeout(step.settleMs ?? 2000);
      return;
    }
    case 'expectReplayRejected': {
      const r = actor.replay;
      if (!r) throw new Error('no replayAs ran before this assertion');
      if (r.inconclusive) {
        // Unverified never scores and never penalises — but it must be VISIBLE.
        // A replay that quietly declines to run looks exactly like a server that
        // refused, and this criterion then rests on its DOM assertions alone.
        ctx.unverified.push(`${actor.name}: ${r.reason}`);
        ctx.serverCheck = 'unverified';
        return;
      }
      ctx.verified?.push(`${actor.name}: server refused ${r.method} ${r.url} (HTTP ${r.status})`);
      ctx.serverCheck = 'verified';
      if (r.accepted) {
        throw new Error(`server ACCEPTED ${r.method} ${r.url} from ${actor.name}, who is not allowed to do it (HTTP ${r.status}) — the check is in the interface, not the server`);
      }
      return;
    }
    case 'expectReceived': {
      // The positive control for the wire checks below. It proves this harness
      // can see what this app's live channel delivers, so that "not received"
      // means the server withheld it rather than that we were looking in the
      // wrong place.
      const needle = expand(step.contains, ctx);
      const deadline = Date.now() + (step.within ?? DEFAULT_WITHIN);
      while (!actor.wasSent(needle) && Date.now() < deadline) await page.waitForTimeout(250);
      if (!actor.wasSent(needle)) {
        throw new Error(`the harness never saw "${needle}" reach ${actor.name}, who should have it — traffic on this app is not visible to the wire checks`);
      }
      return;
    }
    case 'expectNotReceived': {
      const needle = expand(step.contains, ctx);
      // A client that was never sent the text cannot leak it. A client that was
      // sent it has already leaked it, however carefully the interface hides it.
      if (actor.wasSent(needle)) {
        throw new Error(`"${needle}" was delivered to ${actor.name}, who is not a participant — the server sends private data to everyone and relies on the client to hide it`);
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
        const user = step.exact ? step.name : `${step.name}${ctx.scope}`;
        await nameInput.fill(user);
        await page.locator(tid('signup-password')).first().fill(step.password ?? `pw-${user}`);
        await page.locator(tid('signup-submit')).first().click();
        // What proves the app is ready differs per application; chat's room list
        // is the default because that is what this step has always waited for.
        await page.locator(tid(step.readyTestid ?? 'room-list')).first()
          .waitFor({ state: 'attached', timeout: DEFAULT_WITHIN });
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
    case 'fill': {
      // Type into any input the contract names. Chat has dedicated steps for its
      // two inputs; every other application needs the general form.
      const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
      const loc = actor.loc(step.testid, { scope });
      await loc.waitFor({ state: 'visible', timeout: step.within ?? DEFAULT_WITHIN });
      const text = expand(step.text, ctx) ?? '';
      // A rating or a warehouse is as likely to be a dropdown as a text box, and
      // `fill` refuses a <select> outright. The scenario says what the value is;
      // how the app collects it is the app's business.
      const tag = await loc.evaluate(el => el.tagName).catch(() => '');
      if (tag === 'SELECT') {
        await loc.selectOption(text).catch(async () => { await loc.selectOption({ label: text }); });
      } else {
        await loc.fill(text);
      }
      if (step.enter) await loc.press('Enter');
      if (step.settleMs) await page.waitForTimeout(step.settleMs);
      return;
    }
    case 'recordNumber': {
      // Remember a number now so a later assertion can be about the CHANGE.
      // Features share a database, so absolute totals — revenue above all —
      // depend on what every earlier feature happened to buy. A delta does not.
      const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
      const loc = actor.loc(step.testid, { contains: expand(step.contains, ctx), scope });
      await loc.waitFor({ state: 'visible', timeout: step.within ?? DEFAULT_WITHIN });
      const value = parseNumber(await readValue(loc));
      if (value === null) throw new Error(`${tid(step.testid)} has no number to record`);
      ctx.recorded[step.as] = value;
      return;
    }
    case 'pressKey': {
      // Closing a panel by clicking its opener again assumes the opener
      // toggles, and the contract only promises it OPENS — an app whose panel
      // hides the storefront then blocks every later click. Escape is the one
      // close the specification actually requires of everyone.
      await page.keyboard.press(step.key ?? 'Escape');
      await page.waitForTimeout(step.settleMs ?? 600);
      return;
    }
    case 'expectNumber': return expectNumber(actor, step, ctx);
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

  // Zero can satisfy "at most N" — a customer who lost the oversell race has no
  // order at all, and that is exactly what the criterion wants to see. So when
  // only maxCount constrains the step, absence is a count of zero, not a
  // failure to appear.
  const visible = await loc.waitFor({ state: 'visible', timeout: within })
    .then(() => true).catch(() => false);
  if (!visible && !(step.maxCount !== undefined && step.count === undefined)) {
    throw new Error(`${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}${where} not visible within ${within}ms`);
  }

  if (step.count !== undefined || step.maxCount !== undefined) {
    const all = scope
      ? actor.page.locator(tid(scope.testid), { hasText: scope.contains }).first().locator(tid(step.testid))
      : (contains ? actor.page.locator(tid(step.testid), { hasText: contains }) : actor.page.locator(tid(step.testid)));
    const n = visible ? await all.count() : 0;
    if (step.count !== undefined && n !== step.count) {
      throw new Error(`expected exactly ${step.count} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
    if (step.maxCount !== undefined && n > step.maxCount) {
      throw new Error(`expected at most ${step.maxCount} ${tid(step.testid)}${contains ? ` containing "${contains}"` : ''}, found ${n}`);
    }
  }
  if (!visible) return;

  if (step.notContains) {
    const text = (await loc.innerText().catch(() => '')) || '';
    if (text.includes(step.notContains)) {
      throw new Error(`${tid(step.testid)} unexpectedly contains "${step.notContains}" (text: "${text.trim().slice(0, 80)}")`);
    }
  }
}

// What a person reads off this element. A quantity shown in a spin box lives in
// the value, not the text — `innerText` on an <input> is empty, which made a
// perfectly good cart line look like it had no quantity at all.
async function readValue(loc) {
  const tag = await loc.evaluate(el => el.tagName).catch(() => '');
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') {
    return (await loc.inputValue().catch(() => '')) || '';
  }
  return (await loc.innerText().catch(() => '')) || '';
}

// Pull the first number out of rendered text: "Stock: 1,024 left" -> 1024,
// "$12.50" -> 12.5. Returns null when there is no number to read.
function parseNumber(text) {
  const m = (text ?? '').replace(/[, ]/g, '').match(/-?\d+(\.\d+)?/);
  return m ? Number(m[0]) : null;
}

// Counting elements and substring-matching text cannot express "the stock is
// exactly 3": `contains: "3"` also matches 13, and `count` counts tags, not
// values. Quantities are what an inventory is made of, so they get a real
// comparison.
async function expectNumber(actor, step, ctx) {
  const within = step.within ?? DEFAULT_WITHIN;
  const contains = expand(step.contains, ctx);
  const scope = step.in ? { testid: step.in.testid, contains: expand(step.in.contains, ctx) } : undefined;
  const where = scope ? ` inside ${tid(scope.testid)} "${scope.contains}"` : '';
  const loc = actor.loc(step.testid, { contains, scope });
  await loc.waitFor({ state: 'visible', timeout: within }).catch(() => {
    throw new Error(`${tid(step.testid)}${where} not visible within ${within}ms`);
  });

  // `relativeTo` turns the comparison into one about the change since a
  // recordNumber step, which is the only honest way to assert on a running
  // total that earlier features have also contributed to.
  let equals = step.equals;
  if (step.relativeTo !== undefined) {
    const base = ctx.recorded?.[step.relativeTo];
    if (base === undefined) throw new Error(`no number recorded as "${step.relativeTo}"`);
    equals = base + (step.plus ?? 0);
  }

  const check = n => (equals === undefined || n === equals)
    && (step.atLeast === undefined || n >= step.atLeast)
    && (step.atMost === undefined || n <= step.atMost);

  // The value may still be settling — a live count arrives after the write that
  // changed it — so poll rather than read once.
  const deadline = Date.now() + within;
  let last = null;
  for (;;) {
    last = parseNumber(await readValue(loc));
    if (last !== null && check(last)) return;
    if (Date.now() > deadline) break;
    await actor.page.waitForTimeout(250);
  }
  const wanted = [
    equals !== undefined
      ? `exactly ${equals}${step.relativeTo !== undefined ? ` (${step.relativeTo} + ${step.plus ?? 0})` : ''}`
      : null,
    step.atLeast !== undefined ? `at least ${step.atLeast}` : null,
    step.atMost !== undefined ? `at most ${step.atMost}` : null,
  ].filter(Boolean).join(' and ') || 'a number';
  throw new Error(`${tid(step.testid)}${where} reads ${last === null ? 'no number' : last}, expected ${wanted}`);
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
      const text = (step.numeric ? await readValue(loc) : ((await loc.innerText().catch(() => '<missing>')) || '<missing>')).trim() || '<missing>';
      // Comparing raw text makes "97 left" and "Stock: 97" disagree even though
      // the clients are consistent. `numeric` compares the value instead.
      seen[name] = step.numeric ? String(parseNumber(text) ?? '<no number>') : text;
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
  const ctx = { ...runCtx, scope, roomName: base => `${base}-${scope}`, extraContexts, recorded: {},
    unverified: [], verified: [] };
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

  // A feature is worth what its criteria are worth. An explicit `max` is only
  // a consistency check (check-scenarios.mjs enforces it), never a top-up.
  const featureMax = feature.criteria.reduce((n, c) => n + (c.points ?? 1), 0);
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
    ctx.serverCheck = null;
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
    // A criterion the harness could not actually run is not a failure of the
    // application. SpacetimeDB writes over a WebSocket, so a replay-based check
    // captures nothing and reports INCONCLUSIVE — scoring that zero would
    // penalise a backend for being architecturally different rather than wrong,
    // which is the same thumb on the scale as crediting it for being
    // uninspectable. Inconclusive earns no credit AND costs nothing: it leaves
    // the scored total, and is reported separately so the gap is visible.
    const inconclusive = !passed && /^INCONCLUSIVE/.test(String(detail));
    result.criteria.push({ id: criterion.id, desc: criterion.desc, points: criterion.points,
      passed, inconclusive: inconclusive || undefined, detail,
      ...(ctx.serverCheck ? { serverCheck: ctx.serverCheck } : {}) });
    if (passed) result.score += criterion.points;
    else if (inconclusive) {
      result.max -= criterion.points;
      result.inconclusive = [...(result.inconclusive ?? []),
        { id: criterion.id, points: criterion.points, detail: String(detail).slice(0, 300) }];
    }
  }

  for (const actor of actors.values()) {
    for (const e of actor.consoleErrors) result.consoleErrors.push(`[${actor.name}] ${e}`);
  }

  // What the server-side checks could and could not actually test. A criterion
  // whose replay was unverified passed on its interface behaviour only; that is
  // a weaker claim and the report has to say so out loud.
  if (ctx.unverified.length) result.unverified = ctx.unverified;
  if (ctx.verified.length) result.verified = ctx.verified;

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

  if (spec.writeUrlPattern) WRITE_URL_RE = new RegExp(spec.writeUrlPattern);

  const features = args.feature ? spec.features.filter(f => f.id === args.feature) : spec.features;
  const runId = uniq();
  const ctx = { runId, roomName: base => `${base}-${runId}`, restartCmd: args.restartCmd, url: args.url,
    appDir: args.app };

  const browser = await chromium.launch({ headless: !args.headed });
  const report = {
    label: args.label ?? null, url: args.url, level: args.level, runId,
    total: 0, max: features.reduce((n, f) => n + f.criteria.reduce((m, c) => m + (c.points ?? 1), 0), 0), features: [],
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
    // Criteria the harness could not run are removed from the denominator by
    // gradeFeature; carry that up so the run's total is out of what was
    // actually testable against THIS backend.
    if (r.inconclusive?.length) {
      report.max -= r.inconclusive.reduce((n, c) => n + c.points, 0);
      report.inconclusive = [...(report.inconclusive ?? []),
        ...r.inconclusive.map(c => ({ feature: r.id, ...c }))];
    }
    console.log(`${r.score}/${r.max}${r.caps.length ? ` (${r.caps.join('; ')})` : ''}`);
    for (const c of r.criteria.filter(c => !c.passed)) {
      // An untestable criterion is not a defect, and must not read like one.
      console.log(`    ${c.inconclusive ? 'UNTESTABLE' : 'FAIL'} ${c.id} — ${c.detail}`);
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
