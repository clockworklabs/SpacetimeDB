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
import { readFileSync, mkdirSync, existsSync } from 'node:fs';
import { basename, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { harnessBrowserFailure, harnessProcessFailure } from '../harness-errors.mjs';
import { compileScenarioDefinition } from '../definition-compiler.mjs';
import { loadTrack } from '../tracks.mjs';
import { recipeArtifactIdentities, writeArtifact } from '../artifacts.mjs';
import { resolveCalibrationForRelease } from '../calibration-compiler.mjs';
import { resolveGradeRecipeArtifactBinding } from '../recipe-release.mjs';
import { selectScenarioChecks } from '../recipe-selection.mjs';
import { ACTION_REGISTRY } from '../action-catalog.mjs';
import { createActionRunContext, executeAction } from '../action-contract.mjs';
import { createCheckEvidence, evidenceDisposition, evidenceIsMeasured, evidencePassed } from '../check-evidence.mjs';
import { renderEvidenceConsoleLine } from '../evidence-presentation.mjs';
import { measureGradePackRuntime } from '../pack-runtime.mjs';
import { executeStackCapability } from '../stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stack-adapters.mjs';
import {
  BROWSER_ACTION_IDS,
  BROWSER_ACTION_IMPLEMENTATIONS,
} from '../browser-action-executors.mjs';
import {
  ACTOR_TRANSPORT_ACTION_IDS,
  ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
  createNamedActionsCapability,
} from '../actor-transport-action-executors.mjs';
import {
  createDatabaseWriteCapability,
  createLifecycleCapability,
  LIFECYCLE_CONCURRENCY_ACTION_IDS,
  LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS,
} from '../lifecycle-concurrency-action-executors.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DEFAULT_WITHIN = 5000;
const REGISTERED_ACTIONS = new Set([
  ...BROWSER_ACTION_IDS,
  ...ACTOR_TRANSPORT_ACTION_IDS,
  ...LIFECYCLE_CONCURRENCY_ACTION_IDS,
]);
const REGISTERED_ACTION_IMPLEMENTATIONS = Object.freeze({
  ...BROWSER_ACTION_IMPLEMENTATIONS,
  ...ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
  ...LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS,
});
if (REGISTERED_ACTIONS.size !== ACTION_REGISTRY.ids.length
  || ACTION_REGISTRY.ids.some(id => !REGISTERED_ACTIONS.has(id))) {
  throw new Error('registered action implementations do not cover the action catalog exactly');
}

// Truncate a failure detail without throwing away the part that explains it.
//
// A blunt `slice(0, 300)` kept the wrong half. Playwright leads with ~200
// characters of `waiting for locator(...)` and only then says WHY —
// "<div class=backdrop> intercepts pointer events", "element is not enabled".
// One L2 detail was stored 188 characters long, cut off immediately before the
// line naming the cause, and the fix round was sent after a phantom.
//
// Keep the opening line, then the diagnostic lines from the call log, and drop
// the "waiting for / retrying / scrolling" noise in between.
function keepReason(detail, limit = 600) {
  const s = String(detail ?? '');
  if (s.length <= limit) return s;
  const [head, ...rest] = s.split('\n');
  const reasons = rest
    .map(l => l.trim())
    .filter(l => /^-\s/.test(l))
    .map(l => l.replace(/^-\s*/, ''))
    .filter(l => !/^(waiting for|retrying|attempting|scrolling|done scrolling|locator resolved to|\d+ ×)/i.test(l));
  const kept = [...new Set(reasons)].slice(0, 4);
  const out = kept.length ? `${head}\n  - ${kept.join('\n  - ')}` : s.slice(0, limit);
  return out.length > limit ? out.slice(0, limit) : out;
}

function parseArgs(argv) {
  const args = { level: 1, headed: false, selectedCheckKeys: [] };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': args.url = argv[++i]; break;
      case '--level': args.level = parseInt(argv[++i], 10); break;
      case '--out': args.out = argv[++i]; break;
      case '--label': args.label = argv[++i]; break;
      case '--feature': args.feature = parseInt(argv[++i], 10); break;
      case '--spec': args.spec = argv[++i]; break;
      case '--restart-cmd': args.restartCmd = argv[++i]; break;
      case '--restart-spec': args.restartSpec = JSON.parse(argv[++i]); break;
      // Needed to issue the spec's named actions: which stack to talk to, and
      // which track declares the action names.
      case '--backend': args.backend = argv[++i]; break;
      case '--track': args.track = argv[++i]; break;
      case '--expected-recipe-sha256': args.expectedRecipeSha256 = argv[++i]; break;
      case '--selected-check': args.selectedCheckKeys.push(argv[++i]); break;
      case '--selection-sha256': args.selectionSha256 = argv[++i]; break;
      case '--parent-attempt-id': args.parentAttemptId = argv[++i]; break;
      // Which database to write to directly for out-of-band writes.
      case '--db-name': args.dbName = argv[++i]; break;
      case '--app': args.app = argv[++i]; break;
      case '--media': args.media = argv[++i]; break;
      // Lightweight evidence for otherwise media-free qualification runs.
      // Unlike --media this records no video or trace; it only captures every
      // actor when a setup or criterion fails.
      case '--failure-media': args.failureMedia = argv[++i]; break;
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
    case 'dbSetStock': return `set ${step.item}'s ${step.warehouse} stock to ${step.quantity}, straight in the database`;
    case 'callConcurrently': return `${step.actors.length} actors issue "${step.action}" at the same instant`;
    case 'expectCallOutcomes': return `expect exactly ${step.accepted} of those requests were accepted`;
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


// ─── Step execution ──────────────────────────────────────────────────────────

function abortableSleep(ms, signal) {
  if (signal?.aborted) return Promise.reject(signal.reason ?? new Error('action cancelled'));
  return new Promise((resolve, reject) => {
    const timer = setTimeout(done, ms);
    function done() {
      signal?.removeEventListener('abort', cancelled);
      resolve();
    }
    function cancelled() {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancelled);
      reject(signal.reason ?? new Error('action cancelled'));
    }
    signal?.addEventListener('abort', cancelled, { once: true });
  });
}

function browserActionCapabilities(actors, ctx) {
  const actorAccess = Object.freeze({ get: name => actors.get(name) });
  const runtimeValues = Object.freeze({
    defaultWithin: DEFAULT_WITHIN,
    expand: value => expand(value, ctx),
    legacyScopedUser: name => `${name}-${ctx.scope}`,
    roomName: base => ctx.roomName(base),
    scopedUser: name => `${name}${ctx.scope}`,
    recorded: Object.freeze({
      get: key => ctx.recorded?.[key],
      set: (key, value) => { ctx.recorded[key] = value; },
    }),
    sleep: abortableSleep,
    testId: tid,
    clients: Object.freeze({
      async open(actor, settleMs, signal) {
        const fresh = await actor.context.newPage();
        fresh.setDefaultTimeout(DEFAULT_WITHIN);
        actor.attach(fresh);
        await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
        await abortableSleep(settleMs, signal);
      },
      async fresh(actor, sourceName) {
        const context = await actor.page.context().browser().newContext();
        const fresh = await context.newPage();
        const name = `${sourceName}-fresh`;
        // Register teardown ownership before navigation. If goto fails, the
        // partially opened context must still be closed with the feature.
        ctx.extraContexts?.push({ context, name, page: fresh });
        fresh.setDefaultTimeout(DEFAULT_WITHIN);
        await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
        const observer = new Actor(`${actor.name}-fresh`, fresh, context);
        observer.annotate = actor.annotate;
        actors.set(name, observer);
        return name;
      },
    }),
  });
  const transportObservation = Object.freeze({
    defaultWithin: DEFAULT_WITHIN,
    expand: value => expand(value, ctx),
    sleep: abortableSleep,
    verification: Object.freeze({
      structural(message) {
        ctx.verified?.push(message);
        ctx.serverCheck = ctx.serverCheck ?? 'structural';
      },
      unverified(message) {
        ctx.unverified.push(message);
        ctx.serverCheck = 'unverified';
      },
      verified(message) {
        ctx.verified?.push(message);
        ctx.serverCheck = 'verified';
      },
    }),
  });
  const namedActions = createNamedActionsCapability({
    actions: ctx.actions,
    backend: ctx.backend,
    url: ctx.url,
    spacetime: ctx.spacetime,
    lastCalls: Object.freeze({
      get: () => ctx.lastCalls,
      set: value => { ctx.lastCalls = value; },
    }),
    sleep: abortableSleep,
  });
  const concurrency = Object.freeze({
    defaultWithin: DEFAULT_WITHIN,
    dispatch: (step, signal) => runRegisteredAction(step, actors, ctx, signal),
    expand: value => expand(value, ctx),
    sleep: abortableSleep,
    testId: tid,
  });
  return Object.freeze({
    actors: actorAccess,
    'application-files': Object.freeze({ root: ctx.appDir ?? null, expand: value => expand(value, ctx) }),
    'application-lifecycle': createLifecycleCapability({
      restartSpec: ctx.restartSpec,
      restartCmd: ctx.restartCmd,
      application: true,
      sleep: abortableSleep,
    }),
    'backend-lifecycle': createLifecycleCapability({
      restartSpec: ctx.restartSpec,
      restartCmd: ctx.restartCmd,
      sleep: abortableSleep,
    }),
    'browser-interaction': runtimeValues,
    'browser-observation': runtimeValues,
    clock: Object.freeze({ sleep: abortableSleep }),
    concurrency,
    'database-write': createDatabaseWriteCapability({
      backend: ctx.backend,
      spacetime: ctx.spacetime,
      dbName: ctx.dbName,
      expand: value => expand(value, ctx),
    }),
    'named-actions': namedActions,
    subprocess: Object.freeze({ sleep: abortableSleep }),
    'transport-observation': transportObservation,
  });
}

async function runRegisteredAction(step, actors, ctx, signal = null) {
  ctx.actionSequence = (ctx.actionSequence ?? 0) + 1;
  const actionEvidence = await executeAction(ACTION_REGISTRY, step.do, step,
    createActionRunContext({
      capabilities: browserActionCapabilities(actors, ctx),
      implementations: REGISTERED_ACTION_IMPLEMENTATIONS,
      signal,
      attempt: {
        id: `${ctx.runId}:action:${ctx.actionSequence}`,
        parentId: ctx.runId,
      },
    }));
  ctx.actionEvidence?.push({ actor: step.actor ?? null, evidence: actionEvidence });
  if (evidencePassed(actionEvidence)) return actionEvidence.observation;
  const error = new Error(actionEvidence.summary ?? `${step.do} did not complete`);
  Object.defineProperty(error, 'actionEvidence', { value: actionEvidence });
  Object.defineProperty(error, 'actionActor', { value: step.actor ?? null });
  throw error;
}

function classifyCheckFailure(error, fallbackActor = null) {
  const actionEvidence = error?.actionEvidence;
  if (actionEvidence) {
    return {
      status: actionEvidence.status,
      code: actionEvidence.code,
      actor: error.actionActor ?? fallbackActor,
      summary: actionEvidence.summary ?? `${actionEvidence.action.id} did not complete`,
      observation: actionEvidence.observation,
      expected: actionEvidence.expected,
      retryable: actionEvidence.retryable,
    };
  }
  const processFailure = harnessProcessFailure(error);
  if (processFailure) return { status: 'harness_failure', code: 'process_failure', actor: fallbackActor,
    summary: processFailure, observation: null, expected: null, retryable: false };
  const browserFailure = harnessBrowserFailure(error);
  if (browserFailure) return { status: 'harness_failure', code: 'browser_failure', actor: fallbackActor,
    summary: browserFailure, observation: null, expected: null, retryable: false };
  return { status: 'failed', code: 'application_failure', actor: fallbackActor,
    summary: String(error?.message ?? error ?? 'unknown application failure'),
    observation: null, expected: null, retryable: false };
}

function buildCheckEvidence({ ctx, phase, startedAtMs, failure = null, actor = null, summary = null,
  attachments = [], actions = ctx.actionEvidence ?? [], sensitivity = null }) {
  const classified = failure ? classifyCheckFailure(failure, actor) : {
    status: 'passed', code: 'completed', actor: null, summary: null,
    observation: null, expected: null, retryable: false,
  };
  const completedAtMs = Date.now();
  const evidenceSummary = summary ?? classified.summary;
  return createCheckEvidence({
    ...classified,
    phase,
    summary: evidenceSummary == null ? null : keepReason(evidenceSummary),
    startedAtMs,
    completedAtMs,
    actions,
    attachments: attachments.map(attachment => typeof attachment === 'string'
      ? { kind: 'screenshot', ref: basename(attachment) } : attachment),
    sensitivity: sensitivity ?? actions.flatMap(entry => entry.evidence?.sensitivity ?? []),
  });
}

async function runStep(step, actors, ctx) {
  ACTION_REGISTRY.get(step.do);
  return runRegisteredAction(step, actors, ctx);
}

// ─── Feature grading ─────────────────────────────────────────────────────────

async function gradeFeature(browser, feature, args, runCtx) {
  // Features share the app's DATABASE even though each gets fresh browser
  // contexts, so user and room names are scoped per feature — otherwise a
  // defect in one feature (e.g. a hijacked account) corrupts later setups.
  const scope = `${runCtx.runId}f${feature.id}`;
  const extraContexts = [];
  const ctx = { ...runCtx, scope, roomName: base => `${base}-${scope}`, extraContexts, recorded: {},
    unverified: [], verified: [], actionEvidence: [] };
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

  const captureFailureScreenshots = async label => {
    if (!args.failureMedia) return [];
    mkdirSync(args.failureMedia, { recursive: true });
    const captured = [];
    for (const { name, page } of [...contexts, ...extraContexts]) {
      const path = join(args.failureMedia, `${slug}-${label}-${name}.png`);
      const ok = await page.screenshot({ path, fullPage: true, timeout: 5000 })
        .then(() => true, () => false);
      if (ok) captured.push(path);
    }
    return captured;
  };

  const setupStartedAtMs = Date.now();
  ctx.actionEvidence = [];
  try {
    // Setup is not scored, but a failure makes the feature untestable (0).
    for (const step of feature.setup) {
      await annotate(actors.get(step.actor), { feature: feature.name, criterion: 'setup', step: describeStep(step) });
      await runStep(step, actors, ctx);
    }
  } catch (err) {
    // Carry the structured reason onto every criterion so downstream consumers
    // can distinguish an application setup failure from missing harness evidence.
    // ECONNREFUSED can mean the generated app never started, so it remains an
    // application/setup failure. A child-process failure or vanished browser
    // target supplies no behavioral observation and is inconclusive.
    const classified = classifyCheckFailure(err);
    const why = keepReason(classified.summary.trim());
    const measured = evidenceDisposition(classified).measured;
    result.caps.push(measured ? 'setup-failed → 0' : 'setup-unmeasured → untestable');
    const screenshots = await captureFailureScreenshots('setup');
    result.setupEvidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: setupStartedAtMs,
      failure: err, summary: why, attachments: screenshots });
    for (const c of feature.criteria) {
      const base = why ? `setup failed: ${why}` : 'setup failed';
      const points = c.points ?? 1;
      const evidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: setupStartedAtMs,
        failure: err, summary: base, actions: [], sensitivity: result.setupEvidence.sensitivity,
        attachments: [{ kind: 'check-evidence', ref: 'feature.setupEvidence' }, ...screenshots] });
      const recorded = { id: c.id, desc: c.desc, points, evidence };
      result.criteria.push(recorded);
      if (!evidenceIsMeasured(evidence)) {
        result.max -= points;
        result.inconclusive = [...(result.inconclusive ?? []),
          { id: c.id, points, status: evidence.status, code: evidence.code,
            phase: evidence.phase, summary: evidence.summary }];
      }
    }
    if (screenshots.length) result.screenshots = screenshots;
    await closeAll();
    return result;
  }
  result.setupEvidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: setupStartedAtMs });

  let refreshDependent = false;
  for (const criterion of feature.criteria) {
    let failure = null, detail = null, activeActor = null;
    let criterionScreenshots = [];
    const criterionStartedAtMs = Date.now();
    ctx.actionEvidence = [];
    ctx.serverCheck = null;
    try {
      for (const step of criterion.steps) {
        activeActor = step.actor ?? activeActor;
        await annotate(actors.get(step.actor) ?? actors.values().next().value,
          { feature: feature.name, criterion: criterion.id, step: describeStep(step) });
        await runStep(step, actors, ctx);
      }
      for (const a of actors.values()) {
        await annotate(a, { feature: feature.name, criterion: criterion.id, step: 'passed', status: 'pass' });
      }
    } catch (err) {
      failure = err;
      const classified = classifyCheckFailure(err, activeActor);
      detail = classified.summary;
      if (args.media) {
        for (const a of actors.values()) {
          await annotate(a, { feature: feature.name, criterion: criterion.id, step: err.message.slice(0, 120), status: 'fail' });
        }
        const shotActor = actors.get(criterion.steps[criterion.steps.length - 1]?.actor) ?? actors.values().next().value;
        const shot = join(args.media, `${slug}-${criterion.id}.png`);
        const captured = await shotActor.page.screenshot({ path: shot, fullPage: true })
          .then(() => true, () => false);
        if (captured) criterionScreenshots.push(shot);
      } else {
        criterionScreenshots = await captureFailureScreenshots(criterion.id);
      }
      if (criterionScreenshots.length) {
        result.screenshots = [...(result.screenshots ?? []), ...criterionScreenshots];
      }
      // Refresh probe: does the assertion pass once the page is reloaded?
      // If so the feature is refresh-dependent, not realtime.
      const failing = criterion.steps[criterion.steps.length - 1];
      const primaryActionEvidence = [...ctx.actionEvidence];
      if (evidenceDisposition(classified).applicationFailure && failing?.do === 'expect' && !failing.absent) {
        try {
          const actor = actors.get(failing.actor);
          await actor.page.reload({ waitUntil: 'domcontentloaded' });
          await actor.page.waitForTimeout(2000);
          ctx.actionEvidence = [];
          await runStep({ ...failing, within: 6000 }, actors, ctx);
          refreshDependent = true;
          detail += ' — PASSES AFTER RELOAD (refresh-dependent)';
        } catch { /* genuinely absent, not just refresh-dependent */ }
      }
      ctx.actionEvidence = primaryActionEvidence;
    }
    const evidence = buildCheckEvidence({ ctx, phase: 'assertion', startedAtMs: criterionStartedAtMs,
      failure, actor: activeActor, summary: detail, attachments: criterionScreenshots });
    result.criteria.push({ id: criterion.id, desc: criterion.desc, points: criterion.points,
      evidence,
      ...(ctx.serverCheck ? { serverCheck: ctx.serverCheck } : {}) });
    if (evidencePassed(evidence)) result.score += criterion.points;
    else if (!evidenceIsMeasured(evidence)) {
      result.max -= criterion.points;
      result.inconclusive = [...(result.inconclusive ?? []),
        { id: criterion.id, points: criterion.points, status: evidence.status, code: evidence.code,
          phase: evidence.phase, summary: evidence.summary }];
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
  const startedAt = new Date().toISOString();
  const args = parseArgs(process.argv);
  const specPath = args.spec ? args.spec : join(ROOT, 'scenarios', `level-${String(args.level).padStart(2, '0')}.json`);
  let spec;
  try {
    spec = compileScenarioDefinition(JSON.parse(readFileSync(specPath, 'utf8')), { source: specPath });
  } catch (error) {
    throw new Error(`cannot compile scenario ${specPath}: ${error.message}`, { cause: error });
  }

  if (args.selectionSha256 && !/^[a-f0-9]{64}$/.test(args.selectionSha256)) {
    throw new Error('--selection-sha256 must be 64 lowercase hexadecimal characters');
  }
  if (spec.writeUrlPattern) WRITE_URL_RE = new RegExp(spec.writeUrlPattern);

  const candidateFeatures = args.feature ? spec.features.filter(f => f.id === args.feature) : spec.features;
  if (args.feature && candidateFeatures.length === 0) {
    throw new Error(`scenario ${specPath} has no feature ${args.feature}`);
  }
  const runId = uniq();
  // Where the named actions live. The track declares their names; the
  // authenticated backend lease—not generated application config—selects the
  // SpacetimeDB host, module and exact build container used for direct SQL.
  let actions = [], spacetime = null, recipeRelease = null, recipeIdentityRelease = null, calibration = null;
  if (args.track) {
    const track = loadTrack(args.track);
    actions = track.actions;
    const binding = resolveGradeRecipeArtifactBinding(track, args.level, specPath, args.feature ?? null);
    recipeRelease = binding?.release ?? null;
    recipeIdentityRelease = binding?.sourceRelease ?? null;
  }
  if (args.expectedRecipeSha256
    && recipeRelease?.contentSha256 !== args.expectedRecipeSha256) {
    throw new Error(`recipe changed before grading: expected ${args.expectedRecipeSha256}, ` +
      `resolved ${recipeRelease?.contentSha256 ?? 'no recipe'}`);
  }
  const selectedScenario = selectScenarioChecks(
    { ...spec, features: candidateFeatures }, recipeRelease, args.selectedCheckKeys);
  const features = selectedScenario.features;
  const selectedChecks = selectedScenario.checks;
  if (!features.length) throw new Error(`scenario ${specPath} has no selected checks`);
  if (args.track) {
    const track = loadTrack(args.track);
    calibration = resolveCalibrationForRelease(recipeIdentityRelease, {
      trackRoot: track.dir,
      stackBenchRoot: ROOT,
    });
  }
  spacetime = args.backend
    ? executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
      'grading', 'context', { requireBuildContainer: true })
    : null;

  const ctx = { runId, roomName: base => `${base}-${runId}`, restartCmd: args.restartCmd,
    restartSpec: args.restartSpec, url: args.url,
    backend: args.backend, actions, spacetime, dbName: args.dbName,
    appDir: args.app };

  const browser = await chromium.launch({ headless: !args.headed });
  const report = {
    definitionSchemaVersion: spec.schemaVersion,
    recipeRelease,
    label: args.label ?? null, url: args.url, level: args.level, runId,
    total: 0, max: features.reduce((n, f) => n + f.criteria.reduce((m, c) => m + (c.points ?? 1), 0), 0), features: [],
    selection: recipeRelease ? {
      ...(args.selectionSha256 ? { sha256: args.selectionSha256 } : {}),
      checks: selectedChecks.map(({ stableKey, packId, checkGroupId, featureId, criterionId,
        description, points }) => ({ stableKey, packId, checkGroupId, featureId, criterionId,
        description, points })),
    } : null,
  };
  const checkByCriterion = new Map(selectedChecks.map(check => [
    `${String(check.featureId)}\0${String(check.criterionId)}`, check,
  ]));

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
    if (recipeRelease) {
      for (const criterion of r.criteria) {
        const check = checkByCriterion.get(`${String(feature.id)}\0${String(criterion.id)}`);
        if (!check) throw new Error(`graded criterion ${feature.id}/${criterion.id} has no recipe check`);
        criterion.stableKey = check.stableKey;
      }
    }
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
    for (const c of r.criteria.filter(c => !evidencePassed(c.evidence))) {
      console.log(`    ${renderEvidenceConsoleLine(c.evidence, c.id)}`);
    }
  }

  await browser.close();

  if (recipeRelease) report.packRuntime = measureGradePackRuntime(report);

  console.log(`\nTOTAL ${report.total}/${report.max}`);
  if (args.out) {
    const artifactId = `grade-${runId}`;
    writeArtifact(args.out, {
      kind: 'grade',
      id: artifactId,
      attempt: { id: artifactId, parentId: args.parentAttemptId ?? null },
      timestamps: { startedAt, completedAt: new Date().toISOString() },
      identities: recipeArtifactIdentities(recipeIdentityRelease, {
        calibration: calibration ? { id: calibration.id, version: calibration.version,
          sha256: calibration.contentSha256, state: calibration.state } : null,
        stackAdapter: args.backend ? { id: args.backend } : null,
      }),
      payload: report,
    });
    console.log(`Report written to ${args.out}`);
  }
}

main();
