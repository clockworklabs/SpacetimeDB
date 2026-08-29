#!/usr/bin/env node

import { randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { appendFileSync, closeSync, createReadStream, existsSync, mkdirSync, openSync, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';

import { campaignDetail, discoverPlans, readCampaignArtifactBody, readDashboardOverview,
  readJsonLines, resolveCampaignArtifact,
} from './dashboard-model.js';
import type { DashboardPlan } from './dashboard-model.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const DASHBOARD_ROOT = dirname(fileURLToPath(import.meta.url));
const RUNTIME_ROOT = dirname(DASHBOARD_ROOT);
const PUBLIC_ROOT = join(DASHBOARD_ROOT, 'public');
const CAMPAIGN_CLI = join(RUNTIME_ROOT, 'commands', 'campaign-cli.js');
const SAFE_NAME = /^[a-z0-9][a-z0-9.-]{2,119}$/;
const LOOPBACK = new Set(['127.0.0.1', '::1', 'localhost']);
const STATIC = new Map<string, readonly [file: string, contentType: string]>([
  ['/', ['index.html', 'text/html; charset=utf-8']],
  ['/app.js', ['app.js', 'text/javascript; charset=utf-8']],
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

function requiredOption(argv: string[], index: number, option: string): string {
  const value = argv[index];
  if (value === undefined) throw new Error(`${option} requires a value`);
  return value;
}

export function parseDashboardArgs(argv: string[], env: NodeJS.ProcessEnv = process.env): DashboardArgs {
  const args: DashboardArgs = { host: '127.0.0.1', port: 7331,
    resultsRoot: resolve(env.STACK_BENCH_RESULTS_DIR ?? join(STACK_BENCH_ROOT, 'results')),
    plansRoot: '', allowContainerBind: false };
  for (let index = 2; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--host') args.host = requiredOption(argv, ++index, value);
    else if (value === '--port') args.port = Number(requiredOption(argv, ++index, value));
    else if (value === '--results') args.resultsRoot = resolve(requiredOption(argv, ++index, value));
    else if (value === '--plans') args.plansRoot = resolve(requiredOption(argv, ++index, value));
    else if (value === '--allow-container-bind') args.allowContainerBind = true;
    else throw new Error(`unknown dashboard option ${JSON.stringify(value)}`);
  }
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
      for (const event of readJsonLines<DashboardOperation>(path)) {
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
  const server = createServer(async (request, response) => {
    securityHeaders(response);
    try {
      if (!loopbackHost(request.headers.host)) {
        return json(response, 421, { error: 'Dashboard requests must use a loopback host.' });
      }
      const url = new URL(request.url ?? '/', `http://${request.headers.host ?? 'localhost'}`);
      const staticFile = STATIC.get(url.pathname);
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
        const overview = readDashboardOverview({ resultsRoot, plansRoot, operations: feed.list() });
        return json(response, 200, { ...overview, plans: plans(),
          canStart: allowLaunch, csrfToken: token });
      }
      const resumeRoute = url.pathname.match(/^\/api\/campaigns\/([^/]+)\/resume$/);
      if (request.method === 'POST' && resumeRoute) {
        if (!allowLaunch) return json(response, 503, { error: 'Run controls are available inside the StackBench appliance.' });
        if (!controlAuthorized(request, request.headers.host, token, controlSecret)) {
          return json(response, 403, { error: 'The run request is not authorized.' });
        }
        const key = decodeURIComponent(resumeRoute[1] ?? '');
        if (!SAFE_NAME.test(key)) return json(response, 400, { error: 'The campaign name is invalid.' });
        const campaign = campaignDetail(resultsRoot, key);
        const priorExecutions = campaign.summary?.executions ?? 0;
        if (campaign.mode !== 'dependency' || campaign.status !== 'prepared' || priorExecutions < 1) {
          return json(response, 409, { error: 'Only an interrupted campaign that is ready can resume.' });
        }
        const plan = plans().find(item => item.id === campaign.id && item.sha256 === campaign.sha256);
        if (!plan || plan.state !== 'frozen') {
          return json(response, 409, { error: 'The exact frozen plan is unavailable.' });
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
      if (request.method === 'GET' && url.pathname.startsWith('/api/campaigns/')) {
        const key = decodeURIComponent(url.pathname.slice('/api/campaigns/'.length));
        return json(response, 200, campaignDetail(resultsRoot, key));
      }
      if (request.method === 'POST' && url.pathname === '/api/campaigns') {
        if (!allowLaunch) return json(response, 503, { error: 'Run controls are available inside the StackBench appliance.' });
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
          return json(response, 400, { error: 'Choose a frozen plan and a simple output name.' });
        }
        const plan = plans().find(item => item.id === runRequest.planId);
        if (!plan || plan.state !== 'frozen') return json(response, 400, { error: 'The selected frozen plan is unavailable.' });
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
  return { server, token, allowLaunch };
}

async function main() {
  const args = parseDashboardArgs(process.argv);
  const { server, allowLaunch } = createDashboardServer(args);
  await new Promise<void>((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(args.port, args.host, resolveListen);
  });
  console.log(`StackBench dashboard: http://${args.host}:${args.port} (${allowLaunch ? 'controller' : 'read-only'})`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().catch(error => { console.error(`stack-bench-dashboard: ${errorMessage(error)}`); process.exitCode = 2; });
}
