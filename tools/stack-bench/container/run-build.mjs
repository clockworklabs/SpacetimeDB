#!/usr/bin/env node
// Run one build session inside the isolation image.
//
// This is the containerised replacement for agent.mjs spawning the CLI with
// `cwd: appDir`. The difference that matters is what is NOT here: no mount of
// tools/stack-bench, so the grader, the scenario files, the contracts and the
// prompts are not on the filesystem the build can reach. A fix round once read
// the scenario file defining the criteria it was failing and then ran
// grade.mjs; denying those paths is a blocklist against an agent that only
// needed grep and sed, so they are absent instead.
//
// The prompt arrives on stdin and the CLI's JSON result goes to stdout, exactly
// as the host path does, so callers do not care which one ran.
//
// Usage (argv mirrors what agent.mjs already computes):
//   node run-build.mjs --app <dir> --image <tag> --effort high \
//                      [--ports 6473,6573] [--model claude-sonnet-5] \
//                      [--settings /app/.sandbox-settings.json] [--api-key <key>]
//
// Run it by hand from Git Bash and export MSYS_NO_PATHCONV=1 first, or the shell
// rewrites `/app/...` arguments into `C:/Program Files/Git/app/...` before this
// script ever sees them. agent.mjs spawns it without a shell and is unaffected.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, mkdirSync } from 'node:fs';
import { join, resolve, basename, dirname } from 'node:path';
import { homedir } from 'node:os';
import { leaseFromEnv, updateBackendLease } from '../backend-lease.mjs';
import { resolveContainerImage } from '../container-image.mjs';
import { executeStackCapability } from '../stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stack-adapters.mjs';
import { DEFAULT_BUILD_IMAGE } from '../product-config.mjs';
import { dockerMountArguments } from '../container-mount.mjs';
import { dockerHostGatewayArguments } from '../docker-network.mjs';

const argv = process.argv.slice(2);
const opt = (k, d = null) => { const i = argv.indexOf(k); return i === -1 ? d : argv[i + 1]; };

const appDir = opt('--app');
if (!appDir) { console.error('run-build.mjs: --app is required'); process.exit(2); }
const backend = opt('--backend');
if (!backend) { console.error('run-build.mjs: --backend is required'); process.exit(2); }
let adapter;
try { adapter = STACK_ADAPTER_REGISTRY.get(backend); }
catch (error) { console.error(`run-build.mjs: ${error.message}`); process.exit(2); }
const prepareOnly = argv.includes('--prepare-only');
const DOCKER_TIMEOUT_MS = 120_000;
const DOCKER_PROBE_TIMEOUT_MS = 10_000;
const BUILD_SESSION_TIMEOUT_MS = 55 * 60_000;

const REPO = resolve(join(new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'), '..', '..', '..'));
const imageReference = opt('--image', DEFAULT_BUILD_IMAGE);
let imageIdentity;
try { imageIdentity = resolveContainerImage(imageReference); }
catch (error) {
  console.error(`run-build.mjs: cannot resolve image ${imageReference}: ${error.message}`);
  process.exit(2);
}
const image = imageIdentity.id;
const effort = opt('--effort', 'high');
const model = opt('--model', 'claude-sonnet-5');
const maxBudgetUsd = opt('--max-budget-usd');
if (maxBudgetUsd !== null && (!Number.isFinite(Number(maxBudgetUsd)) || Number(maxBudgetUsd) <= 0)) {
  console.error('run-build.mjs: --max-budget-usd must be a positive number');
  process.exit(2);
}
const ports = (opt('--ports', '') || '').split(',').filter(Boolean);

const containerPlan = executeStackCapability(adapter, 'build-container', 'plan', {
  repo: REPO, appDir, env: process.env,
});
if (!containerPlan || !Array.isArray(containerPlan.requiredPaths)
  || !Array.isArray(containerPlan.ensureDirectories) || !Array.isArray(containerPlan.mounts)
  || ![null, 'host'].includes(containerPlan.networkNamespace ?? null)
  || typeof containerPlan.init !== 'string' || !containerPlan.init) {
  console.error(`run-build.mjs: ${backend} adapter returned an invalid build-container plan`);
  process.exit(2);
}

// Auth. An API key is used when one is supplied, and otherwise the CLI
// authenticates from the mounted credential so runs bill to the plan rather
// than to a key. The credential is mounted read-write rather than copied
// because the host rotates that token and a copy stops working when it does.
//
// The key is preferred when present because it keeps a rotating credential
// off the build's filesystem entirely — the build can read anything mounted
// into it, and a token is worth more than a run.
const apiKey = opt('--api-key', process.env.ANTHROPIC_API_KEY ?? '');
const creds = join(homedir(), '.claude', '.credentials.json');
if (!prepareOnly && !apiKey && !existsSync(creds)) {
  console.error(`run-build.mjs: no --api-key/ANTHROPIC_API_KEY and no credentials at ${creds}`);
  console.error('  the container has no way to authenticate');
  process.exit(2);
}

// The audit trail has to survive `--rm`.
//
// leak-audit.mjs decides whether a run is contaminated, and cost-ledger.mjs
// reconstructs the bill; both read the session transcript. Inside the container
// the CLI files it under /root/.claude/projects/-app (cwd is /app, and the CLI
// names a project folder after its path with separators turned into dashes —
// checked, not assumed). Mounting the host folder that `leak-audit --app` looks
// in onto that exact path means the audit keeps working with no argument
// changes, and a containerised run stays as auditable as a host one.
//
// The host's whole ~/.claude/projects is deliberately NOT mounted: it holds
// every other run's transcripts and the user's own sessions.
const projects = prepareOnly ? null : join(homedir(), '.claude', 'projects',
  resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
if (projects) mkdirSync(projects, { recursive: true });

for (const directory of containerPlan.ensureDirectories) mkdirSync(directory, { recursive: true });

// The container OUTLIVES the build session, and that is the whole point.
//
// The first version used `docker run --rm`, so the container died the moment the
// coding session returned — taking the app's dev servers with it. The grader
// runs afterwards and had nothing to talk to: "reseed FAILED (server did not
// come back)", then "ABORTED: could not reset database", after a sweep spent
// $9.46 and graded nothing.
//
// Running the app from the host instead is not an option either: a container
// install produces linux-x64 esbuild and rollup binaries, so a Windows host
// cannot execute the app's node_modules at all (checked, not assumed).
//
// So the container is long-lived and the session is exec'd into it. The build
// starts its own dev servers exactly as it does on the host, and they keep
// running afterwards for exactly the same reason — the process that owns them
// is still alive.
//
// The name is derived from the run's work directory, which already carries a
// timestamp, so it is unique per run and reconstructible from the app path alone
// — restart-backend.sh needs the same name and is given only the app dir.
const containerName = `stack-bench-${basename(dirname(resolve(appDir)))}`;
const dockerEnv = { ...process.env, MSYS_NO_PATHCONV: '1' };

function resolveNetworkMode() {
  if (containerPlan.networkNamespace !== 'host') return 'bridge';
  if (process.env.STACK_BENCH_APPLIANCE !== '1') {
    throw new Error('the host network namespace is available only in appliance mode');
  }
  return 'host';
}

let expectedNetworkMode;
try { expectedNetworkMode = resolveNetworkMode(); }
catch (error) {
  console.error(`run-build.mjs: ${error.message}`);
  process.exit(2);
}

function inspectContainer(name) {
  const r = spawnSync('docker', ['inspect', '--format',
    '{{.Id}} {{.Image}} {{.State.Running}} {{.HostConfig.NetworkMode}}', name],
  { encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS });
  if (r.status !== 0) return null;
  const [id, inspectedImage, running, networkMode] = r.stdout.trim().split(/\s+/, 4);
  return { id, image: inspectedImage, running: running === 'true', networkMode };
}

// Resolve ownership before looking at, reusing, or creating a container. A
// same-name container is not evidence that this run owns it: adopting one
// would let a collision mount an unrelated filesystem into the benchmark, and
// deleting it by name would destroy somebody else's resource. The lease's
// immutable id is the only reuse authority.
let leaseContext;
try { leaseContext = leaseFromEnv(process.env, { backend, active: true }); }
catch (error) {
  console.error(`run-build.mjs: an authenticated active backend lease is required: ${error.message}`);
  process.exit(3);
}

const existing = inspectContainer(containerName);
const priorContainer = leaseContext.lease.resources.buildContainer ?? null;
if (existing) {
  if (!priorContainer) {
    console.error(`run-build.mjs: refusing to adopt existing unleased container ${containerName}`);
    process.exit(3);
  }
  if (priorContainer.name !== containerName || priorContainer.id !== existing.id) {
    console.error(`run-build.mjs: existing container ${containerName}/${existing.id} does not match lease `
      + `${priorContainer.name}/${priorContainer.id}`);
    process.exit(3);
  }
  if (!existing.running) {
    console.error(`run-build.mjs: leased container ${containerName} stopped unexpectedly; refusing to replace it`);
    process.exit(3);
  }
  if (existing.networkMode !== expectedNetworkMode) {
    console.error(`run-build.mjs: leased container ${containerName} uses network ${existing.networkMode}, `
      + `expected ${expectedNetworkMode}`);
    process.exit(3);
  }
} else if (priorContainer) {
  console.error(`run-build.mjs: leased container ${priorContainer.name}/${priorContainer.id} is missing`);
  process.exit(3);
}

// Create it if this is the first round of the run; reuse it for every round
// after, so a fix round finds the app, its node_modules and its servers exactly
// where the build round left them.
if (!existing) {
  const create = [
    'run', '-d', '--init', '--name', containerName,
    '-v', `${resolve(appDir)}:/app`,
  ];
  create.push('--network', expectedNetworkMode);
  create.push(...dockerHostGatewayArguments(expectedNetworkMode));
  if (projects) create.push('-v', `${projects}:/root/.claude/projects/-app`);
  if (!prepareOnly && !apiKey) create.push('-v', `${creds}:/root/.claude/.credentials.json`);
  // The selected adapter owns every stack-specific mount. Giving a treatment
  // another stack's artifacts would violate the "only artifacts under test"
  // boundary.
  for (const requiredPath of containerPlan.requiredPaths) {
    if (!existsSync(requiredPath)) {
      console.error(`run-build.mjs: ${backend} container artifact is missing: ${requiredPath}`);
      process.exit(2);
    }
  }
  for (const mount of containerPlan.mounts) {
    try { create.push(...dockerMountArguments(mount)); }
    catch (error) {
      console.error(`run-build.mjs: ${backend} adapter returned an invalid container mount: ${error.message}`);
      process.exit(2);
    }
  }

  // Dev servers start inside the container and the grader runs on the host, so
  // the track's port window has to be published. Publishing happens at create
  // time only — a port cannot be added to a running container, which is the
  // other reason the session cannot own the container's lifetime.
  if (expectedNetworkMode === 'bridge') for (const p of ports) create.push('-p', `${p}:${p}`);

  // `--init` gives the container a real PID 1. Without it the dev servers the
  // build leaves behind are reparented to `sleep`, which never reaps them.
  create.push('-w', '/app', image, 'sh', '-lc', containerPlan.init);

  const made = spawnSync('docker', create, {
    encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS,
  });
  if (made.status !== 0) {
    console.error(`run-build.mjs: could not start ${containerName}`);
    console.error(made.stderr || made.stdout || made.error?.message || '');
    process.exit(2);
  }
}

const containerInspection = inspectContainer(containerName);
if (!containerInspection) {
  console.error(`run-build.mjs: cannot inspect ${containerName}`);
  process.exit(2);
}
const { id: containerId, image: containerImage } = containerInspection;
try {
  const { path, lease } = leaseContext;
  const prior = lease.resources.buildContainer;
  if (prior && (prior.name !== containerName || prior.id !== containerId)) {
    throw new Error(`running container ${containerName}/${containerId} does not match lease `
      + `${prior.name}/${prior.id}`);
  }
  updateBackendLease(path, { token: lease.ownershipToken, backend, runId: lease.runId }, next => {
    next.resources.buildContainer = {
      name: containerName, id: containerId, image: containerImage, owned: true, running: true,
      networkMode: expectedNetworkMode,
    };
    return next;
  });
} catch (error) {
  // Creation succeeded but ownership recording did not. Remove only the exact
  // id created by this invocation; leaving an unleased container is not safe.
  if (!existing) {
    spawnSync('docker', ['rm', '-f', containerId], {
      stdio: 'ignore', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS,
    });
  }
  console.error(`run-build.mjs: ${error.message}`);
  process.exit(3);
}

if (containerPlan.readyFile) {
  // `docker run -d` returns while PID 1 is still staging the SDK. Do not race
  // the coding session against that copy/install: a partial file dependency
  // fails as an application error and charges the backend for harness setup.
  let ready = false;
  const readyDeadline = Date.now() + 90_000;
  while (Date.now() < readyDeadline) {
    const probe = spawnSync('docker', ['exec', containerName, 'test', '-f', containerPlan.readyFile],
      { stdio: 'ignore', env: dockerEnv, timeout: DOCKER_PROBE_TIMEOUT_MS });
    if (probe.status === 0) { ready = true; break; }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
  }
  if (!ready) {
    const logs = spawnSync('docker', ['logs', containerName], {
      encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS,
    });
    console.error(`run-build.mjs: timed out waiting for ${containerPlan.readyDescription ?? `${backend} setup`}`);
    console.error(logs.stderr || logs.stdout || '');
    process.exit(2);
  }
}

if (prepareOnly) {
  process.stdout.write(`${JSON.stringify({ containerName,
    identity: `${containerId} ${containerImage}`,
    networkMode: expectedNetworkMode })}\n`);
  process.exit(0);
}

const args = ['exec', '-i', '-w', '/app'];

args.push('-e', 'DISABLE_AUTOUPDATER=1', '-e', 'FORCE_PROMPT_CACHING_5M=1');
if (apiKey) args.push('-e', `ANTHROPIC_API_KEY=${apiKey}`);
// A container does not inherit the caller's environment, so anything the run is
// meant to be configured by has to be handed over explicitly. Only variables the
// harness sets deliberately are forwarded — passing the whole environment would
// put the host's shape, and CLAUDE_EFFORT, back inside the build.
if (process.env.MAX_THINKING_TOKENS) {
  args.push('-e', `MAX_THINKING_TOKENS=${process.env.MAX_THINKING_TOKENS}`);
}

args.push(
  containerName,
  'claude', '--print', '--output-format', 'json',
  '--permission-mode', 'acceptEdits',
  '--effort', effort,
  '--model', model,
  ...(maxBudgetUsd !== null ? ['--max-budget-usd', maxBudgetUsd] : []),
  // The app is the only directory a session may reach; inside the container
  // that is all there is, but the flag is kept so host and container runs are
  // configured identically.
  '--add-dir', '/app',
);
// The sandbox settings file is written into the app directory by the caller,
// so it arrives through the /app mount at a known container path.
if (opt('--settings')) args.push('--settings', opt('--settings'));

// MSYS_NO_PATHCONV: Git Bash rewrites container-side paths like /app into
// Windows paths (C:/Program Files/Git/app) and every mount silently lands
// somewhere wrong.
const res = spawnSync('docker', args, {
  input: process.stdin.isTTY ? '' : readFileSync(0, 'utf8'),
  encoding: 'utf8',
  maxBuffer: 256 * 1024 * 1024,
  env: { ...process.env, MSYS_NO_PATHCONV: '1' },
  timeout: BUILD_SESSION_TIMEOUT_MS,
});

if (res.stdout) process.stdout.write(res.stdout);
if (res.stderr) process.stderr.write(res.stderr);
if (res.error) process.stderr.write(`run-build.mjs: coding session failed: ${res.error.message}\n`);
process.exit(res.status ?? 1);
