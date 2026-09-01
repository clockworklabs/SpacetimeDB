#!/usr/bin/env node
/// <reference lib="dom" />
// Score declared criteria from one observed run in isolated actor contexts.
//
import { chromium } from 'playwright';
import type { Browser, BrowserContext, Page } from 'playwright';
import { randomUUID } from 'node:crypto';
import { readFileSync, mkdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { harnessBrowserFailure, harnessProcessFailure,
  runBrowserInfrastructureOperation } from '../src/evidence/harness-errors.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { materializeScenarioCredentials } from '../src/composition/credential-aliases.js';
import { loadTrack } from '../src/composition/tracks.js';
import { recipeArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { resolveGradeRecipeArtifactBinding } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { ActionApplicationFailure, executeAction } from '../src/actions/action-contract.js';
import { createCheckEvidence, evidenceIsMeasured, evidencePassed } from '../src/evidence/check-evidence.js';
import { evidenceNowMs } from '../src/evidence/evidence-timing.js';
import { renderEvidenceConsoleLine } from '../src/evidence/evidence-presentation.js';
import { measureGradePackRuntime } from '../src/composition/pack-runtime.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import {
  createNamedActionsCapability,
} from '../src/actions/actor-transport-action-executors.js';
import type { ConcurrentCallResult }
  from '../src/actions/actor-transport-action-executors.js';
import {
  createDatabaseWriteCapability,
  createLifecycleCapability,
} from '../src/actions/runtime-action-executors.js';
import type { DatabaseWriteLease } from '../src/actions/runtime-action-executors.js';
import { controlAppServer, controlBackendRuntime, parseRuntimeControlSpec }
  from '../src/runtime/backend-control.js';
import type { RuntimeControlSpec } from '../src/runtime/backend-control.js';
import { leaseFromEnv } from '../src/runtime/backend-lease.js';
import type { LeasedSpacetimeTarget } from '../src/runtime/spacetime-target.js';

import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';
import type { ActionEvidence } from '../src/actions/action-contract.js';
import type { CheckEvidence, CheckEvidenceAttachment, CheckEvidencePhase,
  CheckEvidenceStatus } from '../src/evidence/check-evidence.js';
import type { CompletedGradeFeatureResult, CompletedGradeReport, GradeCleanupFailure }
  from '../src/evidence/grade-report.js';
import type { CompiledFeature, CompiledScenarioDefinition,
  CompiledStep } from '../src/composition/definition-compiler.js';
import type { RecipeCheck, RecipeGradeRelease, RecipeRelease } from '../src/composition/recipe-release.js';
import type { TrackAction } from '../src/composition/tracks.js';

type JsonRecord = Record<string, unknown>;
type ActorWrite = {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: JsonRecord | null;
};
type ActorWebSocketWrite = { event: unknown; body: JsonRecord };
type ActorContextEntry = { context: BrowserContext; name: string; page: Page | null };
type CleanupBrowserContext = {
  tracing: { stop(options: { path: string }): Promise<void> };
  close(): Promise<void>;
};
type CleanupVideo = { saveAs(path: string): Promise<void>; delete(): Promise<void> };
type CleanupPage = { video(): CleanupVideo | null };
type CleanupActorContextEntry = {
  context: CleanupBrowserContext;
  name: string;
  page: CleanupPage | null;
};
type FeatureResult = Omit<CompletedGradeFeatureResult, 'setupEvidence'> & {
  setupEvidence?: CheckEvidence;
};
type GradeArgs = {
  url?: string;
  level: number;
  headed: boolean;
  selectedCheckKeys: string[];
  out?: string;
  label?: string;
  feature?: number;
  spec?: string;
  restartSpec?: RuntimeControlSpec;
  backend?: string;
  track?: string;
  recipe?: string;
  expectedRecipeSha256?: string;
  credentialAliases?: unknown;
  selectionSha256?: string;
  parentAttemptId?: string;
  dbName?: string;
  app?: string;
  media?: string;
  failureMedia?: string;
  trace?: boolean;
  nullControl: boolean;
};
type GradeRunContext = {
  runId: string;
  roomName: (base: string) => string;
  restartSpec?: RuntimeControlSpec;
  url: string;
  backend?: string;
  actions: TrackAction[];
  spacetime: LeasedSpacetimeTarget | null;
  dbName?: string;
  databaseLease?: DatabaseWriteLease | null;
  appDir?: string;
  scope?: string;
  extraContexts?: ActorContextEntry[];
  recorded?: Record<string, unknown>;
  unverified?: string[];
  verified?: string[];
  actionEvidence?: Array<{ actor: string | null; evidence: ActionEvidence }>;
  serverCheck?: string | null;
  lastCalls?: ConcurrentCallResult | null;
  defaultWithin?: number;
  nullControl: boolean;
};
type ActionFailure = Error & { actionEvidence?: ActionEvidence; actionActor?: string | null };
type CheckFailure = {
  status: CheckEvidenceStatus;
  code: string;
  actor: string | null;
  summary: string | null;
  observation: unknown;
  expected: unknown;
  retryable: boolean;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function actionFailure(error: unknown): ActionFailure | null {
  return error instanceof Error ? error as ActionFailure : null;
}
const DEFAULT_WITHIN = 5000;
const SETUP_WITHIN = 20000;
// Keep the cause when Playwright prefixes it with locator retry details.
function keepReason(detail: unknown, limit = 600): string {
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

export function parseGradeArgs(argv: readonly string[]): GradeArgs {
  const { values } = parseNodeArgs({ args: [...argv.slice(2)], options: {
    url: { type: 'string' }, level: { type: 'string' }, out: { type: 'string' },
    label: { type: 'string' }, feature: { type: 'string' }, spec: { type: 'string' },
    'restart-spec': { type: 'string' }, backend: { type: 'string' }, track: { type: 'string' },
    recipe: { type: 'string' }, 'expected-recipe-sha256': { type: 'string' },
    'selected-check': { type: 'string', multiple: true },
    'credential-aliases-json': { type: 'string' }, 'selection-sha256': { type: 'string' },
    'parent-attempt-id': { type: 'string' }, 'db-name': { type: 'string' },
    app: { type: 'string' }, media: { type: 'string' }, 'failure-media': { type: 'string' },
    trace: { type: 'boolean' }, headed: { type: 'boolean' },
    'null-control': { type: 'boolean' },
  } });
  const args: GradeArgs = { url: values.url, level: values.level === undefined ? 1 : Number(values.level),
    out: values.out, label: values.label,
    feature: values.feature === undefined ? undefined : Number(values.feature), spec: values.spec,
    restartSpec: values['restart-spec'] === undefined ? undefined
      : parseRuntimeControlSpec(JSON.parse(values['restart-spec'])),
    backend: values.backend, track: values.track, recipe: values.recipe,
    expectedRecipeSha256: values['expected-recipe-sha256'],
    selectedCheckKeys: values['selected-check'] ?? [],
    credentialAliases: values['credential-aliases-json'] === undefined
      ? undefined : JSON.parse(values['credential-aliases-json']),
    selectionSha256: values['selection-sha256'], parentAttemptId: values['parent-attempt-id'],
    dbName: values['db-name'], app: values.app, media: values.media,
    failureMedia: values['failure-media'], trace: values.trace, headed: values.headed ?? false,
    nullControl: values['null-control'] ?? false };
  if (!args.url || !args.spec) {
    throw new Error('Usage: node dist/grader/grade.js --url <app-url> --spec <scenario.json> '
      + '--level <N> [--out <file>] [--label <s>] [--feature <N>]');
  }
  let url: URL;
  try { url = new URL(args.url); }
  catch { throw new Error('--url must be a valid HTTP or HTTPS URL'); }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('--url must use HTTP or HTTPS');
  }
  if (!Number.isInteger(args.level) || args.level < 1) {
    throw new Error('--level must be a positive integer');
  }
  if (args.feature !== undefined && (!Number.isInteger(args.feature) || args.feature < 1)) {
    throw new Error('--feature must be a positive integer');
  }
  if (args.selectionSha256 && !/^[a-f0-9]{64}$/.test(args.selectionSha256)) {
    throw new Error('--selection-sha256 must be 64 lowercase hexadecimal characters');
  }
  return args;
}

const tid = stableElementSelector;
const uniq = () => randomUUID().slice(0, 16);
const MAX_RECEIVED_BYTES = 8 * 1024 * 1024;
const MAX_CONSOLE_ERRORS = 200;

// Isolated browser actor

// Which requests count as writes worth capturing for replay and forgery. The
// default covers chat's routes; a scenario spec can widen it for an application
// whose endpoints are named differently (`writeUrlPattern`).
const DEFAULT_WRITE_URL = '\\/api\\/|\\/rooms|\\/messages';
let WRITE_URL_RE = new RegExp(DEFAULT_WRITE_URL);

class Actor {
  readonly name: string;
  readonly context: BrowserContext;
  page!: Page;
  readonly consoleErrors: string[];
  readonly received: string[];
  receivedBytes = 0;
  receivedOverflow = false;
  lastWrite: ActorWrite | null = null;
  lastWrites: Record<string, ActorWrite> = {};
  writes: ActorWrite[] = [];
  lastWsWrite: ActorWebSocketWrite | null = null;
  annotate = false;

  constructor(name: string, page: Page, context: BrowserContext) {
    this.name = name;
    this.context = context;
    this.consoleErrors = [];
    // Test privacy against delivered payloads, not rendered content.
    this.received = [];
    this.attach(page);
  }
  attach(page: Page): void {
    this.page = page;
    // Capture writes so checks can replay them with changed fields or actors.
    this.lastWrite = null;
    this.lastWrites = {};
    this.writes = [];
    this.lastWsWrite = null;
    // A missing client identity in a WebSocket write proves server-derived identity.
    page.on('websocket', ws => {
      ws.on('framesent', f => {
        const p = typeof f.payload === 'string' ? f.payload : '';
        const m = p.match(/^\d+(\[.*\])$/s);
        if (!m) return;
        try {
          const [event, arg] = JSON.parse(m[1] as string) as unknown[];
          if (arg && typeof arg === 'object' && !Array.isArray(arg)) {
            this.lastWsWrite = { event, body: arg as JsonRecord };
          }
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
      let body: JsonRecord | null = null;
      try {
        const candidate: unknown = JSON.parse(req.postData() ?? '');
        if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
          body = candidate as JsonRecord;
        }
      } catch { /* bodyless, e.g. a DELETE */ }
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
      // Expected 4xx responses are not application console failures.
      if (/Failed to load resource.*status of 4\d\d/.test(text)) return;
      this.consoleErrors.push(text.slice(0, 200));
      if (this.consoleErrors.length > MAX_CONSOLE_ERRORS) this.consoleErrors.shift();
    });
    page.on('pageerror', e => {
      this.consoleErrors.push(`pageerror: ${e.message.slice(0, 200)}`);
      if (this.consoleErrors.length > MAX_CONSOLE_ERRORS) this.consoleErrors.shift();
    });
    page.on('response', async res => {
      const type = res.headers()['content-type'] ?? '';
      // Data only. Scripts and markup are served as text/* too, and a Vite
      // bundle would bury the buffer in megabytes of application source.
      if (!/(application\/json|application\/x-ndjson|text\/event-stream|text\/plain)/.test(type)) return;
      const length = Number(res.headers()['content-length']);
      if (Number.isFinite(length) && length > MAX_RECEIVED_BYTES) {
        this.receivedOverflow = true;
        return;
      }
      try { this.record(await res.text()); } catch { /* body gone, or page closed */ }
    });
  }
  record(payload: string | Buffer): void {
    const text = typeof payload === 'string' ? payload : Buffer.from(payload).toString('utf8');
    if (!text) return;
    const chunk = text.slice(0, 200_000);
    this.received.push(chunk);
    this.receivedBytes += Buffer.byteLength(chunk);
    while (this.receivedBytes > MAX_RECEIVED_BYTES && this.received.length > 1) {
      this.receivedOverflow = true;
      this.receivedBytes -= Buffer.byteLength(this.received.shift()!);
    }
  }
  wasSent(needle: string): boolean {
    if (this.received.some(chunk => chunk.includes(needle))) return true;
    if (this.receivedOverflow) throw new Error('transport evidence exceeded its memory limit');
    return false;
  }
  loc(testid: string, { contains, scope }:
    { contains?: string; scope?: { testid: string; contains?: string } } = {}) {
    // `scope` narrows the search to inside a specific container (e.g. the badge
    // belonging to ONE room), so a stale element elsewhere can't satisfy it.
    const root = scope
      ? this.page.locator(tid(scope.testid), { hasText: scope.contains }).filter({ visible: true }).first()
      : this.page;
    return (contains
      ? root.locator(tid(testid), { hasText: contains })
      : root.locator(tid(testid))).filter({ visible: true }).first();
  }
}

// Expand scenario aliases to the run-scoped values used by the app.
const expand = (s: unknown, ctx: GradeRunContext): unknown =>
  typeof s === 'string'
    ? s.replace(/\{room:([^}]+)\}/g, (_, b) => ctx.roomName(b))
       // Keep generated usernames alphanumeric so ordinary validators accept them.
       .replace(/\{user:([^}]+)\}/g, (_, n) => `${n}${ctx.scope}`)
    : s;


// Put test context in recordings without exposing it to scoped app selectors.

const OVERLAY_ID = '__stackbench_overlay';

async function annotate(actor: Actor | undefined, { feature, criterion, step, status }:
  { feature?: string; criterion?: string; step?: string; status?: 'fail' | 'pass' } = {}): Promise<void> {
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

// Step execution

function abortableSleep(ms: number, signal: AbortSignal | null = null): Promise<void> {
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
      reject(signal?.reason ?? new Error('action cancelled'));
    }
    signal?.addEventListener('abort', cancelled, { once: true });
  });
}

function browserActionCapabilities(actors: Map<string, Actor>, ctx: GradeRunContext): Readonly<Record<string, unknown>> {
  const defaultWithin = ctx.defaultWithin ?? DEFAULT_WITHIN;
  const actorAccess = Object.freeze({ get: (name: string) => actors.get(name) });
  const runtimeValues = Object.freeze({
    defaultWithin,
    expand: (value: unknown) => expand(value, ctx),
    hyphenatedScopedUser: (name: string) => `${name}-${ctx.scope}`,
    roomName: (base: string) => ctx.roomName(base),
    scopedUser: (name: string) => `${name}${ctx.scope}`,
    recorded: Object.freeze({
      get: (key: string) => ctx.recorded?.[key],
      set: (key: string, value: unknown) => { (ctx.recorded ??= {})[key] = value; },
    }),
    sleep: abortableSleep,
    testId: tid,
    clients: Object.freeze({
      async open(actor: Actor, settleMs: number, signal: AbortSignal) {
        const fresh = await actor.context.newPage();
        fresh.setDefaultTimeout(defaultWithin);
        actor.attach(fresh);
        await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
        await abortableSleep(settleMs, signal);
      },
      async fresh(actor: Actor, sourceName: string) {
        const browser = actor.page.context().browser();
        if (!browser) throw new Error('actor browser is unavailable');
        const context = await browser.newContext();
        const fresh = await context.newPage();
        const name = `${sourceName}-fresh`;
        // Register teardown ownership before navigation. If goto fails, the
        // partially opened context must still be closed with the feature.
        ctx.extraContexts?.push({ context, name, page: fresh });
        fresh.setDefaultTimeout(defaultWithin);
        await fresh.goto(ctx.url, { waitUntil: 'domcontentloaded', timeout: 20000 });
        const observer = new Actor(`${actor.name}-fresh`, fresh, context);
        observer.annotate = actor.annotate;
        actors.set(name, observer);
        return name;
      },
    }),
  });
  const transportObservation = Object.freeze({
    defaultWithin,
    expand: (value: unknown) => expand(value, ctx),
    sleep: abortableSleep,
    verification: Object.freeze({
      structural(message: string) {
        ctx.verified?.push(message);
        ctx.serverCheck = ctx.serverCheck ?? 'structural';
      },
      unverified(message: string) {
        (ctx.unverified ??= []).push(message);
        ctx.serverCheck = 'unverified';
      },
      verified(message: string) {
        ctx.verified?.push(message);
        ctx.serverCheck = 'verified';
      },
    }),
  });
  const namedActions = createNamedActionsCapability({
    actions: ctx.actions,
    backend: ctx.backend!,
    url: ctx.url,
    spacetime: ctx.spacetime,
    lastCalls: Object.freeze({
      get: () => ctx.lastCalls ?? null,
      set: value => { ctx.lastCalls = value; },
    }),
    sleep: abortableSleep,
  });
  const concurrency = Object.freeze({
    defaultWithin,
    dispatch: (step: CompiledStep, signal: AbortSignal) => runRegisteredAction(step, actors, ctx, signal),
    expand: (value: unknown) => expand(value, ctx),
    sleep: abortableSleep,
    testId: tid,
  });
  return Object.freeze({
    actors: actorAccess,
    'application-files': Object.freeze({ root: ctx.appDir ?? null, expand: (value: unknown) => expand(value, ctx) }),
    'application-lifecycle': createLifecycleCapability({
      restartSpec: ctx.restartSpec,
      target: 'app-server',
      control: controlAppServer,
      sleep: abortableSleep,
    }),
    'backend-lifecycle': createLifecycleCapability({
      restartSpec: ctx.restartSpec,
      target: 'backend-runtime',
      control: controlBackendRuntime,
      sleep: abortableSleep,
    }),
    'browser-interaction': runtimeValues,
    'browser-observation': runtimeValues,
    clock: Object.freeze({ sleep: abortableSleep }),
    concurrency,
    'database-write': createDatabaseWriteCapability({
      backend: ctx.backend,
      spacetime: ctx.spacetime,
      databaseLease: ctx.databaseLease,
      skip: ctx.nullControl,
      expand: (value: string) => String(expand(value, ctx)),
    }),
    'named-actions': namedActions,
    subprocess: Object.freeze({ sleep: abortableSleep }),
    'transport-observation': transportObservation,
  });
}

async function runRegisteredAction(step: CompiledStep, actors: Map<string, Actor>, ctx: GradeRunContext,
  signal: AbortSignal | null = null): Promise<unknown> {
  const actionEvidence = await executeAction(ACTION_REGISTRY, step.do, step,
    {
      capabilities: browserActionCapabilities(actors, ctx),
      signal,
    });
  ctx.actionEvidence?.push({ actor: step.actor ?? null, evidence: actionEvidence });
  if (actionEvidence.status === 'passed') return actionEvidence.observation;
  const error = new Error(actionEvidence.summary ?? `${step.do} did not complete`);
  Object.defineProperty(error, 'actionEvidence', { value: actionEvidence });
  Object.defineProperty(error, 'actionActor', { value: step.actor ?? null });
  throw error;
}

function classifyCheckFailure(error: unknown, fallbackActor: string | null = null): CheckFailure {
  const actionError = actionFailure(error);
  const actionEvidence = actionError?.actionEvidence;
  if (actionEvidence) {
    return {
      status: actionEvidence.status,
      code: actionEvidence.code,
      actor: actionError?.actionActor ?? fallbackActor,
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
  if (error instanceof ActionApplicationFailure) {
    return { status: 'failed', code: 'application_failure', actor: fallbackActor,
      summary: error.message, observation: error.details.observation ?? null,
      expected: error.details.expected ?? null, retryable: false };
  }
  return { status: 'harness_failure', code: 'unclassified_exception', actor: fallbackActor,
    summary: errorMessage(error ?? 'unknown grader failure'),
    observation: null, expected: null, retryable: false };
}

function buildCheckEvidence({ ctx, phase, startedAtMs, failure = null, actor = null, summary = null,
  attachments = [], actions = ctx.actionEvidence ?? [], sensitivity = null }: {
    ctx: GradeRunContext; phase: CheckEvidencePhase; startedAtMs: number; failure?: unknown;
    actor?: string | null; summary?: string | null; attachments?: Array<string | CheckEvidenceAttachment>;
    actions?: Array<{ actor: string | null; evidence: ActionEvidence }>;
    sensitivity?: readonly string[] | null;
  }): CheckEvidence {
  const classified: CheckFailure = failure ? classifyCheckFailure(failure, actor) : {
    status: 'passed', code: 'completed', actor: null, summary: null,
    observation: null, expected: null, retryable: false,
  };
  const completedAtMs = Math.max(startedAtMs, evidenceNowMs());
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

async function runStep(step: CompiledStep, actors: Map<string, Actor>, ctx: GradeRunContext): Promise<unknown> {
  return runRegisteredAction(step, actors, ctx);
}

export async function closeActorContexts(entries: readonly CleanupActorContextEntry[], {
  trace = false, media = null, slug = 'grade',
}: { trace?: boolean; media?: string | null; slug?: string } = {}): Promise<GradeCleanupFailure[]> {
  const failures: GradeCleanupFailure[] = [];
  const record = (name: string, stage: string, error: unknown): void => { failures.push({
    actor: name,
    stage,
    reason: keepReason(errorMessage(error)),
  }); };
  for (const { context, name, page } of entries) {
    if (trace) {
      try {
        await context.tracing.stop({ path: join(media ?? '.', `${slug}-${name}.trace.zip`) });
      } catch (error) { record(name, 'trace', error); }
    }
    let video = null;
    if (media && page) {
      try { video = page.video(); }
      catch (error) { record(name, 'video-handle', error); }
    }
    try { await context.close(); }
    catch (error) { record(name, 'context-close', error); }
    if (video) {
      try { await video.saveAs(join(media!, `${slug}-${name}.webm`)); }
      catch (error) { record(name, 'video-save', error); }
      try { await video.delete(); }
      catch (error) { record(name, 'video-delete', error); }
    }
  }
  return failures;
}

// Feature grading

function completedFeatureResult(result: FeatureResult): CompletedGradeFeatureResult {
  if (!result.setupEvidence) {
    throw new Error(`feature ${result.id} completed without setup evidence`);
  }
  return { ...result, setupEvidence: result.setupEvidence };
}

async function gradeFeature(browser: Browser, feature: CompiledFeature, args: GradeArgs,
  runCtx: GradeRunContext): Promise<CompletedGradeFeatureResult> {
  // Features share the app's DATABASE even though each gets fresh browser
  // contexts, so user and room names are scoped per feature — otherwise a
  // defect in one feature (e.g. a hijacked account) corrupts later setups.
  const scope = `${runCtx.runId}f${feature.id}`;
  const extraContexts: ActorContextEntry[] = [];
  const ctx: GradeRunContext = { ...runCtx, scope, roomName: (base: string) => `${base}-${scope}`, extraContexts, recorded: {},
    unverified: [], verified: [], actionEvidence: [] };
  const actors = new Map();
  const contexts: ActorContextEntry[] = [];
  const slug = `${args.label ?? 'run'}-f${feature.id}`;

  // A feature is worth what its criteria are worth. An explicit `max` is only
  // a consistency check enforced by check-scenarios, never a top-up.
  const featureMax = feature.criteria.reduce((n, c) => n + (c.points ?? 1), 0);
  const result: FeatureResult = {
    id: feature.id, name: feature.name, score: 0, max: featureMax,
    criteria: [], consoleErrors: [],
  };
  const closeAll = async () => {
    const failures = await closeActorContexts([...contexts, ...extraContexts], {
      trace: args.trace, media: args.media, slug,
    });
    if (failures.length) result.cleanupEvidence = { status: 'harness_failure', failures };
    return failures;
  };
  const initializationStartedAtMs = evidenceNowMs();
  try {
    for (const name of feature.actors!) {
      // Isolated storage per actor. Video is per-context, so each actor gets its
      // own recording — you can watch what every participant saw, side by side.
      const context = await runBrowserInfrastructureOperation('context creation', () =>
        browser.newContext(
          args.media ? { recordVideo: { dir: args.media, size: { width: 1280, height: 800 } } } : {}
        ));
      contexts.push({ context, name, page: null });
      if (args.trace) {
        await runBrowserInfrastructureOperation('trace start', () =>
          context.tracing.start({ screenshots: true, snapshots: true }));
      }
      const page = await runBrowserInfrastructureOperation('page creation', () => context.newPage());
      contexts[contexts.length - 1]!.page = page;
      page.setDefaultTimeout(SETUP_WITHIN);
      try {
        await page.goto(args.url!, { waitUntil: 'domcontentloaded', timeout: 20000 });
      } catch (cause) {
        if (harnessBrowserFailure(cause)) throw cause;
        throw new ActionApplicationFailure('application did not load during browser setup', {
          observation: errorMessage(cause), expected: 'a reachable application page',
        });
      }
      const actor = new Actor(name, page, context);
      actor.annotate = Boolean(args.media);
      actors.set(name, actor);
    }
  } catch (error) {
    const classified = classifyCheckFailure(error);
    const reason = keepReason((classified.summary ?? '').trim());
    result.setupEvidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: initializationStartedAtMs,
      failure: error, summary: reason });
    for (const criterion of feature.criteria) {
      const points = criterion.points ?? 1;
      const evidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: initializationStartedAtMs,
        failure: error, summary: `browser setup failed: ${reason}`, actions: [] });
      result.criteria.push({ id: criterion.id, desc: criterion.desc, points, evidence });
      result.inconclusive = [...(result.inconclusive ?? []),
        { id: criterion.id, points, status: evidence.status, code: evidence.code,
          phase: evidence.phase, summary: evidence.summary }];
    }
    await closeAll();
    return completedFeatureResult(result);
  }

  const captureFailureScreenshots = async (label: string): Promise<string[]> => {
    if (!args.failureMedia) return [];
    mkdirSync(args.failureMedia, { recursive: true });
    const captured: string[] = [];
    for (const { name, page } of [...contexts, ...extraContexts]) {
      if (!page) continue;
      const path = join(args.failureMedia, `${slug}-${label}-${name}.png`);
      const ok = await page.screenshot({ path, fullPage: true, timeout: 5000 })
        .then(() => true, () => false);
      if (ok) captured.push(path);
    }
    return captured;
  };

  const setupStartedAtMs = evidenceNowMs();
  ctx.defaultWithin = SETUP_WITHIN;
  ctx.actionEvidence = [];
  try {
    // Setup is not scored, but a failure makes the feature untestable (0).
    for (const step of feature.setup) {
      await annotate(actors.get(step.actor), { feature: feature.name, criterion: 'setup', step: step.do });
      await runStep(step, actors, ctx);
    }
  } catch (err) {
    // Preserve the typed setup failure on every affected criterion.
    const classified = classifyCheckFailure(err);
    const why = keepReason((classified.summary ?? '').trim());
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
        result.inconclusive = [...(result.inconclusive ?? []),
          { id: c.id, points, status: evidence.status, code: evidence.code,
            phase: evidence.phase, summary: evidence.summary }];
      }
    }
    if (screenshots.length) result.screenshots = screenshots;
    await closeAll();
    return completedFeatureResult(result);
  }
  result.setupEvidence = buildCheckEvidence({ ctx, phase: 'setup', startedAtMs: setupStartedAtMs });
  ctx.defaultWithin = DEFAULT_WITHIN;
  for (const actor of actors.values()) actor.page.setDefaultTimeout(DEFAULT_WITHIN);

  for (const criterion of feature.criteria) {
    let failure: unknown = null, detail: string | null = null, activeActor: string | null = null;
    let criterionScreenshots: string[] = [];
    const criterionStartedAtMs = evidenceNowMs();
    ctx.actionEvidence = [];
    ctx.serverCheck = null;
    try {
      for (const step of criterion.steps) {
        activeActor = step.actor ?? activeActor;
        await annotate(actors.get(step.actor) ?? actors.values().next().value,
          { feature: feature.name, criterion: criterion.id, step: step.do });
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
          await annotate(a, { feature: feature.name, criterion: criterion.id,
            step: errorMessage(err).slice(0, 120), status: 'fail' });
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
    }
    const evidence = buildCheckEvidence({ ctx, phase: 'assertion', startedAtMs: criterionStartedAtMs,
      failure, actor: activeActor, summary: detail, attachments: criterionScreenshots });
    result.criteria.push({ id: criterion.id, desc: criterion.desc, points: criterion.points,
      evidence,
      ...(ctx.serverCheck ? { serverCheck: ctx.serverCheck } : {}) });
    if (evidencePassed(evidence)) result.score += criterion.points;
    else if (!evidenceIsMeasured(evidence)) {
      result.inconclusive = [...(result.inconclusive ?? []),
        { id: criterion.id, points: criterion.points, status: evidence.status, code: evidence.code,
          phase: evidence.phase, summary: evidence.summary }];
    }
  }

  for (const actor of actors.values()) {
    for (const e of actor.consoleErrors) result.consoleErrors.push(`[${actor.name}] ${e}`);
  }

  // Retain diagnostics for server-side checks that could not execute. The
  // action executor marks those criteria inconclusive, so they cannot score.
  if (ctx.unverified?.length) result.unverified = ctx.unverified;
  if (ctx.verified?.length) result.verified = ctx.verified;

  await closeAll();
  if (args.media) result.videos = contexts.map(c => join(args.media!, `${slug}-${c.name}.webm`));
  return completedFeatureResult(result);
}

// Main

async function main(): Promise<void> {
  const startedAt = new Date().toISOString();
  const args = parseGradeArgs(process.argv);
  const specPath = args.spec!;
  let spec: CompiledScenarioDefinition;
  try {
    const compiled = compileScenarioDefinition(JSON.parse(readFileSync(specPath, 'utf8')),
      { source: specPath });
    spec = materializeScenarioCredentials(compiled, args.credentialAliases);
  } catch (error) {
    throw new Error(`cannot compile scenario ${specPath}: ${errorMessage(error)}`, { cause: error });
  }

  if (typeof spec.writeUrlPattern === 'string' && spec.writeUrlPattern) {
    WRITE_URL_RE = new RegExp(spec.writeUrlPattern);
  }

  const candidateFeatures = args.feature ? spec.features.filter(f => f.id === args.feature) : spec.features;
  if (args.feature && candidateFeatures.length === 0) {
    throw new Error(`scenario ${specPath} has no feature ${args.feature}`);
  }
  const runId = uniq();
  // Where the named actions live. The track declares their names; the
  // authenticated backend lease—not generated application config—selects the
  // SpacetimeDB host, module and exact build container used for direct SQL.
  let actions: TrackAction[] = [], spacetime: LeasedSpacetimeTarget | null = null,
    recipeRelease: RecipeGradeRelease | null = null,
    recipeIdentityRelease: RecipeRelease | null = null,
    calibration: ReturnType<typeof resolveCalibrationForRelease> | null = null;
  if (args.track) {
    const track = loadTrack(args.track);
    actions = track.actions;
    const binding = resolveGradeRecipeArtifactBinding(track, args.level, specPath,
      args.feature ?? null, args.recipe);
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
    ? STACK_ADAPTER_REGISTRY.get(args.backend).grading.context({ requireBuildContainer: true })
    : null;
  const hostedBackend = args.backend === 'mongodb' || args.backend === 'postgres';
  const hasLeaseAuthority = Boolean(process.env.STACK_BENCH_LEASE
    || process.env.STACK_BENCH_LEASE_TOKEN);
  let databaseLease: DatabaseWriteLease | null = null;
  if (hostedBackend && hasLeaseAuthority) {
    const lease = leaseFromEnv(process.env, { backend: args.backend, active: true }).lease;
    const { container, database } = lease.resources;
    if (!container || !database) {
      throw new Error(`active ${args.backend} lease has no complete database identity`);
    }
    databaseLease = { resources: { container, database } };
  }

  const ctx: GradeRunContext = { runId, roomName: (base: string) => `${base}-${runId}`,
    restartSpec: args.restartSpec, url: args.url!,
    backend: args.backend, actions, spacetime, dbName: args.dbName,
    databaseLease,
    nullControl: args.nullControl,
    appDir: args.app };

  const browser = await chromium.launch({ headless: !args.headed });
  const report: JsonRecord & CompletedGradeReport & {
    inconclusive?: Array<JsonRecord>;
    cleanupEvidence?: { status: 'harness_failure'; failures: GradeCleanupFailure[] };
  } = {
    definitionSchemaVersion: spec.schemaVersion,
    recipeRelease,
    label: args.label ?? null, url: args.url, level: args.level, runId,
    total: 0, max: features.reduce((n, f) => n + f.criteria.reduce((m, c) => m + (c.points ?? 1), 0), 0), features: [],
    selection: recipeRelease ? {
      ...(args.selectionSha256 ? { sha256: args.selectionSha256 } : {}),
      checks: selectedChecks.map(({ stableKey, packId, checkGroupId, featureId, criterionId,
        description, points }) => {
        if (!packId) throw new Error(`selected check ${stableKey} has no pack id`);
        return { stableKey, packId, checkGroupId, featureId, criterionId, description, points };
      }),
    } : null,
  };
  const checkByCriterion = new Map<string, RecipeCheck>(selectedChecks.map(check => [
    `${String(check.featureId)}\0${String(check.criterionId)}`, check,
  ]));

  try {
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
      // The recipe owns the denominator. An unmeasured criterion earns zero and
      // remains explicitly inconclusive; it must never change the contract.
      if (r.inconclusive?.length) {
        report.inconclusive = [...(report.inconclusive ?? []),
          ...r.inconclusive.map(c => ({ feature: r.id, ...c }))];
      }
      console.log(`${r.score}/${r.max}`);
      for (const c of r.criteria.filter(c => !evidencePassed(c.evidence))) {
        console.log(`    ${renderEvidenceConsoleLine(c.evidence, c.id)}`);
      }
    }
  } finally {
    try { await browser.close(); }
    catch (error) {
      report.cleanupEvidence = { status: 'harness_failure', failures: [{
        actor: null, stage: 'browser-close', reason: keepReason(errorMessage(error)),
      }] };
    }
  }

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

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().catch(error => {
    console.error(error instanceof Error ? (error.stack ?? error.message) : String(error));
    process.exitCode = 2;
  });
}
