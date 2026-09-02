#!/usr/bin/env node

import { randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { appendFileSync, closeSync, createReadStream, existsSync, mkdirSync, openSync, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { parseArgs as parseNodeArgs } from 'node:util';

import { contained, discoverPlans, readCampaignArtifactBody,
  readJsonLines, resolveCampaignArtifact, summarizeCampaign,
} from './dashboard-model.js';
import type { DashboardPlan } from './dashboard-model.js';
import { attemptChecks, attemptLogSlice, attemptPackage, campaignProgression, campaignSheet,
  overviewSummary } from './dashboard-views.js';
import { watchCampaigns } from './dashboard-events.js';
import type { CampaignChange, CampaignWatcher } from './dashboard-events.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { stackBenchResultsRoot } from '../src/runtime/operational-paths.js';

const DASHBOARD_ROOT = dirname(fileURLToPath(import.meta.url));
const RUNTIME_ROOT = dirname(DASHBOARD_ROOT);
const PUBLIC_ROOT = join(DASHBOARD_ROOT, 'public');
const CAMPAIGN_CLI = join(RUNTIME_ROOT, 'commands', 'campaign-cli.js');
const SAFE_NAME = /^[a-z0-9][a-z0-9.-]{2,119}$/;
const SPA_PATH = /^\/(?:plans|c\/[^/]+(?:\/a\/[^/]+)?)$/;
const HEARTBEAT_MS = 25_000;
const LOOPBACK = new Set(['127.0.0.1', '::1', 'localhost']);
const STATIC = new Map<string, readonly [file: string, contentType: string]>([
  ['/', ['index.html', 'text/html; charset=utf-8']],
  ['/app.js', ['app.js', 'text/javascript; charset=utf-8']],
  ['/climb.js', ['climb.js', 'text/javascript; charset=utf-8']],
  ['/format.js', ['format.js', 'text/javascript; charset=utf-8']],
  ['/graph.js', ['graph.js', 'text/javascript; charset=utf-8']],
  ['/metrics.js', ['metrics.js', 'text/javascript; charset=utf-8']],
  ['/views/attempt.js', ['views/attempt.js', 'text/javascript; charset=utf-8']],
  ['/views/campaign.js', ['views/campaign.js', 'text/javascript; charset=utf-8']],
  ['/views/campaigns.js', ['views/campaigns.js', 'text/javascript; charset=utf-8']],
  ['/styles.css', ['styles.css', 'text/css; charset=utf-8']],
  ['/spacetimedb-mark.svg', ['spacetimedb-mark.svg', 'image/svg+xml']],
  // The brand faces are served from here rather than a CDN: the dashboard's own
  // content-security-policy allows 'self' only, and the appliance has no
  // outbound access to fetch them at view time.
  ['/fonts/inter-latin-variable.woff2', ['fonts/inter-latin-variable.woff2', 'font/woff2']],
  ['/fonts/source-code-pro-latin-variable.woff2', ['fonts/source-code-pro-latin-variable.woff2', 'font/woff2']],
]);

interface DashboardArgs {
  host: string;
  port: number;
  resultsRoot: string;
  plansRoot: string;
  allowContainerBind: boolean;
}

export interface DashboardOperation {
  id: string;
  updatedAt: string;
  [key: string]: unknown;
}

function dashboardOperation(value: unknown): DashboardOperation {
  if (!value || typeof value !== 'object') throw new Error('dashboard operation must be an object');
  const id = 'id' in value ? value.id : undefined;
  const updatedAt = 'updatedAt' in value ? value.updatedAt : undefined;
  if (typeof id !== 'string' || !id) throw new Error('dashboard operation id is required');
  if (typeof updatedAt !== 'string' || !updatedAt) {
    throw new Error('dashboard operation updatedAt is required');
  }
  return { ...value, id, updatedAt };
}

export interface OperationFeed {
  readonly path?: string;
  append(event: DashboardOperation): void;
  list(): DashboardOperation[];
}

export interface LaunchChild {
  pid?: number;
  once(event: 'error', listener: (error: Error) => void): unknown;
  once(event: 'exit', listener: (code: number | null, signal: NodeJS.Signals | null) => void): unknown;
}

export interface LaunchInput {
  plan: DashboardPlan & { path: string };
  output: string;
  operationId: string;
  resultsRoot: string;
  feed: OperationFeed;
  env?: NodeJS.ProcessEnv;
}

export interface DashboardServerOptions {
  resultsRoot: string;
  plansRoot: string;
  allowLaunch?: boolean;
  token?: string;
  controlSecret?: string;
  controlSecretFile?: string;
  feed?: OperationFeed;
  launch?: (input: LaunchInput) => LaunchChild;
  plans?: () => DashboardPlan[];
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function loopbackHost(value: unknown): boolean {
  return /^(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?$/i.test(String(value ?? ''));
}

function loadControlSecret(options: DashboardServerOptions, allowLaunch: boolean): string | null {
  if (!allowLaunch) return null;
  const file = options.controlSecretFile
    ?? process.env.STACK_BENCH_DASHBOARD_CONTROL_SECRET_FILE;
  let value = options.controlSecret;
  if (value === undefined && file) {
    try { value = readFileSync(resolve(file), 'utf8').trim(); }
    catch { throw new Error('dashboard run controls require a readable operator control secret file'); }
  }
  if (typeof value !== 'string' || value.length < 32 || value.length > 4096 || /[\r\n]/.test(value)) {
    throw new Error('dashboard run controls require a valid operator control secret file');
  }
  return value;
}

function sameSecret(actual: unknown, expected: unknown): boolean {
  if (typeof actual !== 'string' || typeof expected !== 'string') return false;
  const left = Buffer.from(actual);
  const right = Buffer.from(expected);
  return left.length === right.length && timingSafeEqual(left, right);
}

function controlAuthorized(request: IncomingMessage, host: string | undefined,
  csrfToken: string, controlSecret: string | null): boolean {
  return request.headers.origin === `http://${host}`
    && sameSecret(request.headers['x-stack-bench-token'], csrfToken)
    && sameSecret(request.headers['x-stack-bench-control-secret'], controlSecret);
}

export function parseDashboardArgs(argv: string[], env: NodeJS.ProcessEnv = process.env): DashboardArgs {
  const { values } = parseNodeArgs({ args: argv.slice(2), options: {
    host: { type: 'string' }, port: { type: 'string' }, results: { type: 'string' },
    plans: { type: 'string' }, 'allow-container-bind': { type: 'boolean' },
  } });
  const args: DashboardArgs = { host: values.host ?? '127.0.0.1',
    port: values.port === undefined ? 7331 : Number(values.port),
    resultsRoot: stackBenchResultsRoot(STACK_BENCH_ROOT, env),
    plansRoot: '', allowContainerBind: values['allow-container-bind'] ?? false };
  if (values.results) args.resultsRoot = resolve(values.results);
  if (values.plans) args.plansRoot = resolve(values.plans);
  args.plansRoot ||= join(args.resultsRoot, 'plans');
  const applianceContainerBind = args.allowContainerBind
    && env.STACK_BENCH_APPLIANCE === '1' && args.host === '0.0.0.0';
  if (!LOOPBACK.has(args.host) && !applianceContainerBind) {
    throw new Error('dashboard must bind to localhost or a loopback address');
  }
  if (!Number.isInteger(args.port) || args.port < 1 || args.port > 65535) {
    throw new Error('dashboard port must be an integer from 1 through 65535');
  }
  return args;
}

function json(response: ServerResponse, status: number, value: unknown): void {
  const body = Buffer.from(`${JSON.stringify(value)}\n`);
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8',
    'content-length': body.length, 'cache-control': 'no-store' });
  response.end(body);
}

function securityHeaders(response: ServerResponse): void {
  response.setHeader('content-security-policy', "default-src 'self'; connect-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'");
  response.setHeader('x-content-type-options', 'nosniff');
  response.setHeader('x-frame-options', 'DENY');
  response.setHeader('referrer-policy', 'no-referrer');
}

async function body(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let bytes = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += buffer.length;
    if (bytes > 16 * 1024) throw new Error('request body is too large');
    chunks.push(buffer);
  }
  try { return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch { throw new Error('request body must be valid JSON'); }
}

function createOperationFeed(resultsRoot: string): OperationFeed {
  const root = join(resolve(resultsRoot), 'dashboard');
  const path = join(root, 'operations.jsonl');
  mkdirSync(root, { recursive: true });
  return {
    path,
    append(event: DashboardOperation) {
      appendFileSync(path, `${JSON.stringify(event)}\n`, { encoding: 'utf8', mode: 0o600 });
    },
    list() {
      const latest = new Map<string, DashboardOperation>();
      for (const value of readJsonLines(path)) {
        const event = dashboardOperation(value);
        latest.set(event.id, { ...(latest.get(event.id) ?? {}), ...event });
      }
      return [...latest.values()].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
    },
  };
}

function launchCampaign({ plan, output, operationId, resultsRoot, feed,
  env = process.env }: LaunchInput): LaunchChild {
  const operationsRoot = join(resolve(resultsRoot), 'dashboard', 'operations');
  mkdirSync(operationsRoot, { recursive: true });
  const stdoutPath = join(operationsRoot, `${operationId}.stdout.log`);
  const stderrPath = join(operationsRoot, `${operationId}.stderr.log`);
  const stdout = openSync(stdoutPath, 'a', 0o600);
  const stderr = openSync(stderrPath, 'a', 0o600);
  const child = spawn(process.execPath, [CAMPAIGN_CLI, 'run', plan.path, '--out', output], {
    cwd: STACK_BENCH_ROOT, env, stdio: ['ignore', stdout, stderr], windowsHide: true,
  });
  closeSync(stdout);
  closeSync(stderr);
  child.once('error', error => feed.append({ schemaVersion: 1, id: operationId,
    status: 'failed', updatedAt: new Date().toISOString(), error: error.message }));
  child.once('exit', (code, signal) => feed.append({ schemaVersion: 1, id: operationId,
    status: code === 0 ? 'completed' : 'failed', updatedAt: new Date().toISOString(),
    exitCode: code, signal }));
  return child;
}

export function createDashboardServer(options: DashboardServerOptions) {
  const resultsRoot = resolve(options.resultsRoot);
  const plansRoot = resolve(options.plansRoot);
  const allowLaunch = options.allowLaunch ?? process.env.STACK_BENCH_APPLIANCE === '1';
  const token = options.token ?? randomBytes(24).toString('base64url');
  const controlSecret = loadControlSecret(options, allowLaunch);
  const feed = options.feed ?? createOperationFeed(resultsRoot);
  const launch = options.launch ?? launchCampaign;
  const plans = options.plans ?? (() => discoverPlans(plansRoot));
  const launchReservations = new Set<string>();
  const campaignsRoot = join(resultsRoot, 'campaigns');
  const listeners = new Set<ServerResponse>();
  let watcher: CampaignWatcher | null = null;
  let heartbeat: NodeJS.Timeout | null = null;
  const broadcast = (change: CampaignChange): void => {
    const frame = `event: ${change.type}\ndata: ${JSON.stringify({ key: change.key,
      ...(change.attemptId === undefined ? {} : { attemptId: change.attemptId }) })}\n\n`;
    for (const listener of listeners) listener.write(frame);
  };
  const stopEvents = (): void => {
    watcher?.close();
    watcher = null;
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = null;
  };
  const server = createServer(async (request, response) => {
    securityHeaders(response);
    try {
      if (!loopbackHost(request.headers.host)) {
        return json(response, 421, { error: 'Dashboard requests must use a loopback host.' });
      }
      const url = new URL(request.url ?? '/', `http://${request.headers.host ?? 'localhost'}`);
      // The client routes are pages, not fragments: each serves the shell.
      const staticFile = STATIC.get(url.pathname)
        ?? (SPA_PATH.test(url.pathname) ? STATIC.get('/') : undefined);
      if (request.method === 'GET' && staticFile) {
        const [file, type] = staticFile;
        const path = join(PUBLIC_ROOT, file);
        const size = existsSync(path) ? statSync(path).size : 0;
        if (!size) return json(response, 404, { error: 'Not found' });
        response.writeHead(200, { 'content-type': type, 'content-length': size,
          'cache-control': file === 'index.html' ? 'no-store' : 'public, max-age=300' });
        createReadStream(path).pipe(response);
        return;
      }
      if (request.method === 'GET' && url.pathname === '/api/health') {
        return json(response, 200, { ok: true, mode: allowLaunch ? 'controller' : 'read-only' });
      }
      if (request.method === 'GET' && url.pathname === '/api/overview') {
        return json(response, 200, { campaigns: overviewSummary(campaignsRoot),
          canStart: allowLaunch, csrfToken: token });
      }
      if (request.method === 'GET' && url.pathname === '/api/plans') {
        return json(response, 200, plans());
      }
      if (request.method === 'GET' && url.pathname === '/api/events') {
        response.writeHead(200, { 'content-type': 'text/event-stream; charset=utf-8',
          'cache-control': 'no-store', connection: 'keep-alive' });
        response.write(': open\n\n');
        listeners.add(response);
        watcher ??= watchCampaigns(campaignsRoot, broadcast);
        // A silent connection is dropped by proxies long before a campaign
        // writes anything.
        heartbeat ??= setInterval(() => {
          for (const listener of listeners) listener.write(': ping\n\n');
        }, HEARTBEAT_MS).unref();
        request.once('close', () => {
          listeners.delete(response);
          if (!listeners.size) stopEvents();
        });
        return;
      }
      const resumeRoute = url.pathname.match(/^\/api\/campaigns\/([^/]+)\/resume$/);
      if (request.method === 'POST' && resumeRoute) {
        if (!allowLaunch) return json(response, 503, { error: 'Run controls are available inside the Stack Bench appliance.' });
        if (!controlAuthorized(request, request.headers.host, token, controlSecret)) {
          return json(response, 403, { error: 'The run request is not authorized.' });
        }
        const key = decodeURIComponent(resumeRoute[1] ?? '');
        if (!SAFE_NAME.test(key)) return json(response, 400, { error: 'The campaign name is invalid.' });
        const campaign = summarizeCampaign(
          contained(join(resultsRoot, 'campaigns'), key, 'campaign'), { includeAttempts: false });
        const priorExecutions = campaign.summary?.executions ?? 0;
        if (campaign.mode !== 'dependency' || campaign.status !== 'prepared' || priorExecutions < 1) {
          return json(response, 409, { error: 'Only an interrupted campaign that is ready can resume.' });
        }
        const plan = plans().find(item => item.id === campaign.id && item.sha256 === campaign.sha256);
        if (!plan || plan.state !== 'frozen') {
          return json(response, 409, { error: 'The test plan used by this campaign is unavailable.' });
        }
        const reservation = `${campaign.id}:${campaign.sha256}:${key}`;
        if (launchReservations.has(reservation)) {
          return json(response, 409, { error: 'This campaign already has an active controller.' });
        }
        launchReservations.add(reservation);
        const now = new Date().toISOString();
        const operation = { schemaVersion: 1, id: randomUUID(), type: 'campaign.resume',
          status: 'running', createdAt: now, updatedAt: now, actor: 'local-operator',
          campaignId: campaign.id, campaignSha256: campaign.sha256, outputName: key };
        feed.append(operation);
        const output = join(resultsRoot, 'campaigns', key);
        try {
          const child = launch({ plan: { ...plan, path: join(plansRoot, plan.file) }, output,
            operationId: operation.id, resultsRoot, feed, env: process.env });
          if (typeof child?.once === 'function') {
            child.once('error', () => launchReservations.delete(reservation));
            child.once('exit', () => launchReservations.delete(reservation));
          } else {
            launchReservations.delete(reservation);
          }
          feed.append({ ...operation, pid: child?.pid ?? null });
        } catch (error) {
          launchReservations.delete(reservation);
          feed.append({ schemaVersion: 1, id: operation.id, status: 'failed',
            updatedAt: new Date().toISOString(), error: errorMessage(error) });
          throw error;
        }
        return json(response, 202, operation);
      }
      const artifactRoute = url.pathname.match(/^\/api\/campaigns\/([^/]+)\/artifacts\/([^/]+)$/);
      if (request.method === 'GET' && artifactRoute) {
        let artifact;
        try {
          artifact = resolveCampaignArtifact(resultsRoot, decodeURIComponent(artifactRoute[1] ?? ''),
            decodeURIComponent(artifactRoute[2] ?? ''));
        } catch {
          return json(response, 404, { error: 'Campaign artifact not found.' });
        }
        const body = readCampaignArtifactBody(artifact);
        const download = url.searchParams.get('download') === '1';
        const type = artifact.kind === 'visual' ? artifact.contentType
          : artifact.kind === 'report' && !download ? 'text/html; charset=utf-8'
            : 'text/plain; charset=utf-8';
        if (artifact.kind === 'report' && !download) {
          response.setHeader('content-security-policy', "sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:");
        }
        response.writeHead(200, { 'content-type': type, 'content-length': body.length,
          'cache-control': 'no-store', 'content-disposition': `${download ? 'attachment' : 'inline'}; filename*=UTF-8''${encodeURIComponent(basename(artifact.path))}` });
        response.end(body);
        return;
      }
      const campaignRoute = url.pathname.match(/^\/api\/campaigns\/([^/]+)(?:\/(.*))?$/);
      if (request.method === 'GET' && campaignRoute) {
        const key = decodeURIComponent(campaignRoute[1] ?? '');
        const rest = campaignRoute[2] ?? '';
        if (!SAFE_NAME.test(key)) {
          return json(response, 400, { error: 'The campaign name is invalid.' });
        }
        const attemptRoute = rest.match(/^attempts\/([^/]+)\/(checks|package|log)$/);
        const attemptId = attemptRoute ? decodeURIComponent(attemptRoute[1] ?? '') : '';
        if (attemptRoute && !SAFE_NAME.test(attemptId)) {
          return json(response, 400, { error: 'The attempt name is invalid.' });
        }
        const from = url.searchParams.get('from') ?? '0';
        if (attemptRoute?.[2] === 'log' && (!/^\d+$/.test(from) || !Number.isSafeInteger(Number(from)))) {
          return json(response, 400, { error: 'The log offset must be a whole number of bytes.' });
        }
        try {
          if (!rest) return json(response, 200, campaignSheet(resultsRoot, key));
          if (rest === 'progression') {
            const progression = campaignProgression(resultsRoot, key);
            return progression
              ? json(response, 200, progression)
              : json(response, 404, { error: 'Progression is recorded for dependency campaigns only.' });
          }
          if (attemptRoute?.[2] === 'checks') {
            return json(response, 200, attemptChecks(resultsRoot, key, attemptId));
          }
          if (attemptRoute?.[2] === 'package') {
            return json(response, 200, attemptPackage(resultsRoot, key, attemptId));
          }
          if (attemptRoute) {
            const slice = attemptLogSlice(resultsRoot, key, attemptId, Number(from));
            const text = Buffer.from(slice.text);
            response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8',
              'content-length': text.length, 'cache-control': 'no-store',
              'x-stack-bench-log-offset': String(slice.offset) });
            response.end(text);
            return;
          }
        } catch {
          // The evidence reader names real paths; a campaign or attempt that
          // cannot be read is a miss, not a message.
          return json(response, 404, { error: 'Not found' });
        }
        return json(response, 404, { error: 'Not found' });
      }
      if (request.method === 'POST' && url.pathname === '/api/campaigns') {
        if (!allowLaunch) return json(response, 503, { error: 'Run controls are available inside the Stack Bench appliance.' });
        if (!controlAuthorized(request, request.headers.host, token, controlSecret)) {
          return json(response, 403, { error: 'The run request is not authorized.' });
        }
        if (!String(request.headers['content-type'] ?? '').toLowerCase().startsWith('application/json')) {
          return json(response, 415, { error: 'Run requests must use JSON.' });
        }
        const input = await body(request);
        const runRequest = input !== null && typeof input === 'object'
          ? input as { planId?: unknown; outputName?: unknown } : {};
        if (typeof runRequest.planId !== 'string' || typeof runRequest.outputName !== 'string'
          || !SAFE_NAME.test(runRequest.outputName)) {
          return json(response, 400, { error: 'Choose a test plan and a simple run name.' });
        }
        const plan = plans().find(item => item.id === runRequest.planId);
        if (!plan || plan.state !== 'frozen') {
          return json(response, 400, { error: 'The selected test plan is not ready to run.' });
        }
        const path = join(plansRoot, plan.file);
        const output = join(resultsRoot, 'campaigns', runRequest.outputName);
        if (existsSync(output)) return json(response, 409, { error: 'That run output already exists.' });
        const reservation = `output:${runRequest.outputName}`;
        if (launchReservations.has(reservation)) {
          return json(response, 409, { error: 'That run output already has an active controller.' });
        }
        launchReservations.add(reservation);
        const now = new Date().toISOString();
        const operation = { schemaVersion: 1, id: randomUUID(), type: 'campaign.run', status: 'running',
          createdAt: now, updatedAt: now, actor: 'local-operator', campaignId: plan.id,
          campaignSha256: plan.sha256, outputName: runRequest.outputName };
        feed.append(operation);
        try {
          const child = launch({ plan: { ...plan, path }, output, operationId: operation.id,
            resultsRoot, feed, env: process.env });
          if (typeof child?.once === 'function') {
            child.once('error', () => launchReservations.delete(reservation));
            child.once('exit', () => launchReservations.delete(reservation));
          } else {
            launchReservations.delete(reservation);
          }
          feed.append({ ...operation, pid: child?.pid ?? null });
        } catch (error) {
          launchReservations.delete(reservation);
          feed.append({ schemaVersion: 1, id: operation.id, status: 'failed',
            updatedAt: new Date().toISOString(), error: errorMessage(error) });
          throw error;
        }
        return json(response, 202, operation);
      }
      return json(response, 404, { error: 'Not found' });
    } catch (error) {
      return json(response, 500, { error: errorMessage(error) });
    }
  });
  // An open event stream is not an idle connection: the watchers stop and the
  // streams end as the server closes, not once it has.
  const closeServer = server.close.bind(server);
  server.close = ((callback?: (error?: Error) => void) => {
    stopEvents();
    for (const listener of listeners) listener.end();
    listeners.clear();
    return closeServer(callback);
  }) as typeof server.close;
  return { server, token, allowLaunch };
}

async function main() {
  const args = parseDashboardArgs(process.argv);
  const { server, allowLaunch } = createDashboardServer(args);
  await new Promise<void>((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(args.port, args.host, resolveListen);
  });
  console.log(`Stack Bench dashboard: http://${args.host}:${args.port} (${allowLaunch ? 'controller' : 'read-only'})`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().catch(error => { console.error(`stack-bench-dashboard: ${errorMessage(error)}`); process.exitCode = 2; });
}
