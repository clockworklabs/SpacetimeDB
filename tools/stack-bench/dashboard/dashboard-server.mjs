#!/usr/bin/env node

import { randomBytes, randomUUID } from 'node:crypto';
import { appendFileSync, closeSync, createReadStream, existsSync, mkdirSync, openSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';

import { campaignDetail, discoverPlans, readCampaignArtifactBody, readDashboardOverview,
  readJsonLines, resolveCampaignArtifact,
} from './dashboard-model.mjs';

const DASHBOARD_ROOT = dirname(fileURLToPath(import.meta.url));
const STACK_BENCH_ROOT = dirname(DASHBOARD_ROOT);
const PUBLIC_ROOT = join(DASHBOARD_ROOT, 'public');
const CAMPAIGN_CLI = join(STACK_BENCH_ROOT, 'commands', 'campaign-cli.mjs');
const SAFE_NAME = /^[a-z0-9][a-z0-9.-]{2,119}$/;
const LOOPBACK = new Set(['127.0.0.1', '::1', 'localhost']);
const STATIC = new Map([
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

function loopbackHost(value) {
  return /^(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?$/i.test(String(value ?? ''));
}

export function parseDashboardArgs(argv, env = process.env) {
  const args = { host: '127.0.0.1', port: 7331,
    resultsRoot: resolve(env.STACK_BENCH_RESULTS_DIR ?? join(STACK_BENCH_ROOT, 'results')),
    plansRoot: null, allowContainerBind: false };
  for (let index = 2; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--host') args.host = argv[++index];
    else if (value === '--port') args.port = Number(argv[++index]);
    else if (value === '--results') args.resultsRoot = resolve(argv[++index]);
    else if (value === '--plans') args.plansRoot = resolve(argv[++index]);
    else if (value === '--allow-container-bind') args.allowContainerBind = true;
    else throw new Error(`unknown dashboard option ${JSON.stringify(value)}`);
  }
  args.plansRoot ??= join(args.resultsRoot, 'plans');
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

function json(response, status, value) {
  const body = Buffer.from(`${JSON.stringify(value)}\n`);
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8',
    'content-length': body.length, 'cache-control': 'no-store' });
  response.end(body);
}

function securityHeaders(response) {
  response.setHeader('content-security-policy', "default-src 'self'; connect-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'");
  response.setHeader('x-content-type-options', 'nosniff');
  response.setHeader('x-frame-options', 'DENY');
  response.setHeader('referrer-policy', 'no-referrer');
}

async function body(request) {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of request) {
    bytes += chunk.length;
    if (bytes > 16 * 1024) throw new Error('request body is too large');
    chunks.push(chunk);
  }
  try { return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch { throw new Error('request body must be valid JSON'); }
}

function createOperationFeed(resultsRoot) {
  const root = join(resolve(resultsRoot), 'dashboard');
  const path = join(root, 'operations.jsonl');
  mkdirSync(root, { recursive: true });
  return {
    path,
    append(event) { appendFileSync(path, `${JSON.stringify(event)}\n`, { encoding: 'utf8', mode: 0o600 }); },
    list() {
      const latest = new Map();
      for (const event of readJsonLines(path)) latest.set(event.id, { ...(latest.get(event.id) ?? {}), ...event });
      return [...latest.values()].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
    },
  };
}

function launchCampaign({ plan, output, operationId, resultsRoot, feed, env = process.env }) {
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

export function createDashboardServer(options) {
  const resultsRoot = resolve(options.resultsRoot);
  const plansRoot = resolve(options.plansRoot);
  const allowLaunch = options.allowLaunch ?? process.env.STACK_BENCH_APPLIANCE === '1';
  const token = options.token ?? randomBytes(24).toString('base64url');
  const feed = options.feed ?? createOperationFeed(resultsRoot);
  const launch = options.launch ?? launchCampaign;
  const plans = options.plans ?? (() => discoverPlans(plansRoot));
  const server = createServer(async (request, response) => {
    securityHeaders(response);
    try {
      if (!loopbackHost(request.headers.host)) {
        return json(response, 421, { error: 'Dashboard requests must use a loopback host.' });
      }
      const url = new URL(request.url, `http://${request.headers.host ?? 'localhost'}`);
      if (request.method === 'GET' && STATIC.has(url.pathname)) {
        const [file, type] = STATIC.get(url.pathname);
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
      const artifactRoute = url.pathname.match(/^\/api\/campaigns\/([^/]+)\/artifacts\/([^/]+)$/);
      if (request.method === 'GET' && artifactRoute) {
        let artifact;
        try {
          artifact = resolveCampaignArtifact(resultsRoot, decodeURIComponent(artifactRoute[1]),
            decodeURIComponent(artifactRoute[2]));
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
        const expectedOrigin = `http://${request.headers.host}`;
        if (request.headers.origin !== expectedOrigin || request.headers['x-stack-bench-token'] !== token) {
          return json(response, 403, { error: 'The run request did not come from this dashboard session.' });
        }
        if (!String(request.headers['content-type'] ?? '').toLowerCase().startsWith('application/json')) {
          return json(response, 415, { error: 'Run requests must use JSON.' });
        }
        const input = await body(request);
        if (typeof input?.planId !== 'string' || typeof input?.outputName !== 'string'
          || !SAFE_NAME.test(input.outputName)) {
          return json(response, 400, { error: 'Choose a frozen plan and a simple output name.' });
        }
        const plan = plans().find(item => item.id === input.planId);
        if (!plan || plan.state !== 'frozen') return json(response, 400, { error: 'The selected frozen plan is unavailable.' });
        const path = join(plansRoot, plan.file);
        const output = join(resultsRoot, 'campaigns', input.outputName);
        if (existsSync(output)) return json(response, 409, { error: 'That run output already exists.' });
        const now = new Date().toISOString();
        const operation = { schemaVersion: 1, id: randomUUID(), type: 'campaign.run', status: 'running',
          createdAt: now, updatedAt: now, actor: 'local-operator', campaignId: plan.id,
          campaignSha256: plan.sha256, outputName: input.outputName };
        feed.append(operation);
        try {
          const child = launch({ plan: { ...plan, path }, output, operationId: operation.id,
            resultsRoot, feed, env: process.env });
          feed.append({ ...operation, pid: child?.pid ?? null });
        } catch (error) {
          feed.append({ schemaVersion: 1, id: operation.id, status: 'failed',
            updatedAt: new Date().toISOString(), error: error.message });
          throw error;
        }
        return json(response, 202, operation);
      }
      return json(response, 404, { error: 'Not found' });
    } catch (error) {
      return json(response, 500, { error: error.message });
    }
  });
  return { server, token, allowLaunch };
}

async function main() {
  const args = parseDashboardArgs(process.argv);
  const { server, allowLaunch } = createDashboardServer(args);
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(args.port, args.host, resolveListen);
  });
  console.log(`StackBench dashboard: http://${args.host}:${args.port} (${allowLaunch ? 'controller' : 'read-only'})`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().catch(error => { console.error(`stack-bench-dashboard: ${error.message}`); process.exitCode = 2; });
}
