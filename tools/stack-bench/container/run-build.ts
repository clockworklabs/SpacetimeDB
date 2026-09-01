#!/usr/bin/env node
// The build container must not expose Stack Bench source or grading material.
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { chmodSync, existsSync, readFileSync, mkdirSync } from 'node:fs';
import { join, resolve, basename, dirname } from 'node:path';
import { homedir } from 'node:os';
import { parseArgs } from 'node:util';
import { leaseFromEnv, updateBackendLease } from '../src/runtime/backend-lease.js';
import { resolveContainerImage } from '../src/runtime/container-image.js';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.js';
import { leasedDatabaseEnvironment, STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { BUILD_CONTAINER_RESOURCE_LIMITS, DEFAULT_BUILD_IMAGE }
  from '../src/composition/product-config.js';
import { dockerMountArguments } from '../src/runtime/container-mount.js';
import { dockerHostGatewayArguments } from '../src/runtime/docker-network.js';
import { LEGACY_SUBSCRIPTION_TOKEN_TARGET, resolveContainerAuth } from './container-auth.js';
import { credentialBrokerDiagnostics, reconcileCredentialBrokerReceipt, startCredentialBroker,
  stopCredentialBroker } from './credential-broker.js';
import { clearMissingBuildContainerLease,
  recoverStoppedBuildContainer } from './recover-build-container.js';
import { BUILD_CONTAINER_CREATION_LABEL, containerIdFromDockerOutput,
  removeFailedBuildContainer } from './reconcile-build-container.js';
import { CODING_SESSION_TIMEOUT_MS } from '../src/agents/coding-session-timeouts.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_CONTROL_DIR, CODING_CONTAINER_PROCESS_IDENTITY,
  codingContainerAgentEnvironment, codingContainerTranscriptHandoffCommand }
  from '../src/runtime/coding-container-policy.js';
import { runTranscriptAwareProcess, snapshotClaudeTranscripts }
  from '../src/agents/claude-terminal-recovery.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { PRICING_UNIT, validatePricingAuthority }
  from '../src/evidence/pricing-authority.js';
import { REPOSITORY_ROOT } from '../src/package-root.js';

type JsonRecord = Record<string, unknown>;
type ContainerMount = { kind: 'bind' | 'volume'; source: string; target: string; readOnly: boolean };
type BuildContainerPlan = { requiredPaths: string[]; ensureDirectories: string[]; mounts: ContainerMount[];
  networkNamespace: 'host' | null; init: string; readyFile?: string; readyDescription?: string };
type InspectedMount = { type: string; source: string; name: string | null; destination: string; readOnly: boolean };
type InspectedContainer = { id: string; image: string; running: boolean; networkMode: string | null;
  readonlyRootfs: boolean; tmpfs: Record<string, string>; capAdd: string[]; capDrop: string[];
  securityOpt: string[]; pidsLimit: number | null; nanoCpus: number | null; memoryBytes: number | null;
  memorySwapBytes: number | null; mounts: InspectedMount[]; unsafeCredentialExposure: boolean };

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

function buildContainerPlan(value: unknown): BuildContainerPlan | null {
  if (!isRecord(value) || !Array.isArray(value.requiredPaths) || !Array.isArray(value.ensureDirectories)
    || !Array.isArray(value.mounts) || (value.networkNamespace !== null && value.networkNamespace !== 'host')
    || typeof value.init !== 'string' || !value.init
    || !value.requiredPaths.every((path): path is string => typeof path === 'string')
    || !value.ensureDirectories.every((path): path is string => typeof path === 'string')
    || !value.mounts.every((mount): mount is ContainerMount => isRecord(mount)
      && (mount.kind === 'bind' || mount.kind === 'volume') && typeof mount.source === 'string'
      && typeof mount.target === 'string' && typeof mount.readOnly === 'boolean')) return null;
  return { requiredPaths: value.requiredPaths, ensureDirectories: value.ensureDirectories,
    mounts: value.mounts, networkNamespace: value.networkNamespace, init: value.init,
    ...(typeof value.readyFile === 'string' ? { readyFile: value.readyFile } : {}),
    ...(typeof value.readyDescription === 'string' ? { readyDescription: value.readyDescription } : {}) };
}

const { values } = parseArgs({ args: process.argv.slice(2), options: {
  app: { type: 'string' }, backend: { type: 'string' }, 'prepare-only': { type: 'boolean' },
  image: { type: 'string' }, effort: { type: 'string' }, model: { type: 'string' },
  'max-budget-usd': { type: 'string' }, 'pricing-json': { type: 'string' },
  'resume-session': { type: 'string' }, 'recover-stopped-container': { type: 'boolean' },
  'completion-marker': { type: 'string' }, ports: { type: 'string' },
} });

const appDir = values.app;
if (!appDir) { console.error('run-build.js: --app is required'); process.exit(2); }
const backend = values.backend;
if (!backend) { console.error('run-build.js: --backend is required'); process.exit(2); }
let adapter;
try { adapter = STACK_ADAPTER_REGISTRY.get(backend); }
catch (error) { console.error(`run-build.js: ${errorMessage(error)}`); process.exit(2); }
const prepareOnly = values['prepare-only'] ?? false;
const DOCKER_TIMEOUT_MS = 120_000;
const DOCKER_PROBE_TIMEOUT_MS = 10_000;
const { uid: AGENT_UID, gid: AGENT_GID, home: AGENT_HOME } = CODING_CONTAINER_AGENT;
const AGENT_ENVIRONMENT = codingContainerAgentEnvironment();
const CONTROL_DIR = CODING_CONTAINER_CONTROL_DIR;
const REQUIRED_CAPABILITIES = Object.freeze([
  'DAC_OVERRIDE', 'FOWNER', 'KILL', 'SETGID', 'SETUID',
]);
const REQUIRED_TMPFS = Object.freeze({
  '/tmp': 'rw,nosuid,nodev,mode=1777',
  [AGENT_HOME]: `rw,nosuid,nodev,uid=${AGENT_UID},gid=${AGENT_GID},mode=0700`,
  [`${AGENT_HOME}/.claude`]: `rw,nosuid,nodev,uid=${AGENT_UID},gid=${AGENT_GID},mode=0700`,
  '/deps': 'rw,nosuid,nodev,mode=0755',
  [CONTROL_DIR]: 'rw,nosuid,nodev,mode=0700',
});

const REPO = REPOSITORY_ROOT;
const imageReference = values.image ?? DEFAULT_BUILD_IMAGE;
let imageIdentity;
try { imageIdentity = resolveContainerImage(imageReference); }
catch (error) {
  console.error(`run-build.js: cannot resolve image ${imageReference}: ${errorMessage(error)}`);
  process.exit(2);
}
const image = imageIdentity.id;
const effort = values.effort ?? 'high';
const model = values.model ?? 'claude-sonnet-5';
const maxBudgetUsd = values['max-budget-usd'] ?? null;
if (maxBudgetUsd !== null && (!Number.isFinite(Number(maxBudgetUsd)) || Number(maxBudgetUsd) <= 0)) {
  console.error('run-build.js: --max-budget-usd must be a positive number');
  process.exit(2);
}
let pricing = null;
try {
  const supplied = values['pricing-json'] ?? null;
  if (supplied !== null) {
    pricing = validatePricingAuthority(JSON.parse(supplied), { at: '--pricing-json' });
  } else if (maxBudgetUsd !== null) {
    const rates = claudeRatesForModel(model);
    if (!rates) throw new Error(`no default pricing is recorded for model ${model}`);
    pricing = validatePricingAuthority({ unit: PRICING_UNIT, rates },
      { at: 'default pricing' });
  }
} catch (error) {
  console.error(`run-build.js: ${errorMessage(error)}`);
  process.exit(2);
}
const resumeSession = values['resume-session'] ?? null;
if (resumeSession !== null
  && !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(resumeSession)) {
  console.error('run-build.js: --resume-session must be a UUID');
  process.exit(2);
}
const recoverStoppedContainer = values['recover-stopped-container'] ?? false;
const completionMarker = values['completion-marker'] ?? null;
if (!prepareOnly && !/^[A-Z][A-Z0-9_]*$/.test(completionMarker ?? '')) {
  console.error('run-build.js: --completion-marker must be an uppercase marker');
  process.exit(2);
}
const ports = (values.ports ?? '').split(',').filter(Boolean);

const containerPlan = buildContainerPlan(executeStackCapability(adapter, 'build-container', 'plan', {
  repo: REPO, appDir, env: process.env,
}));
if (!containerPlan) {
  console.error(`run-build.js: ${backend} adapter returned an invalid build-container plan`);
  process.exit(2);
}

// Auth is resolved in the controller. A short-lived broker forwards model API
// requests later. The coding container never receives the long-lived provider
// credential or a credential file.
const apiKey = process.env.STACK_BENCH_AGENT_API_KEY ?? process.env.ANTHROPIC_API_KEY ?? '';
const creds = join(homedir(), '.claude', '.credentials.json');
let auth = null;
if (!prepareOnly) {
  try { auth = resolveContainerAuth({ apiKey, env: process.env, credentialsPath: creds }); }
  catch (error) { console.error(`run-build.js: ${errorMessage(error)}`); process.exit(2); }
}

// Persist this run's transcript without exposing other local sessions.
const projects = prepareOnly ? null : join(homedir(), '.claude', 'projects',
  resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
function ensureAgentWritable(directory: string): void {
  mkdirSync(directory, { recursive: true });
  chmodSync(directory, process.env.STACK_BENCH_APPLIANCE === '1' ? 0o700 : 0o777);
}

ensureAgentWritable(appDir);
if (projects) ensureAgentWritable(projects);
for (const directory of containerPlan.ensureDirectories) ensureAgentWritable(directory);

// Grading and repair reuse this leased container.
const containerName = `stack-bench-${basename(dirname(resolve(appDir)))}`;
const dockerEnv: NodeJS.ProcessEnv = { ...process.env, MSYS_NO_PATHCONV: '1' };

function resolveNetworkMode(): 'bridge' | 'host' {
  if (containerPlan!.networkNamespace !== 'host') return 'bridge';
  if (process.env.STACK_BENCH_APPLIANCE !== '1') {
    throw new Error('the host network namespace is available only in appliance mode');
  }
  return 'host';
}

let expectedNetworkMode: 'bridge' | 'host';
try { expectedNetworkMode = resolveNetworkMode(); }
catch (error) {
  console.error(`run-build.js: ${errorMessage(error)}`);
  process.exit(2);
}

function inspectContainer(name: string): InspectedContainer | null {
  const r = spawnSync('docker', ['inspect', name],
  { encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS });
  if (r.status !== 0) return null;
  const parsed: unknown = JSON.parse(r.stdout);
  if (!Array.isArray(parsed) || !isRecord(parsed[0])) throw new Error('Docker returned an invalid container inspection');
  const inspected = parsed[0];
  const mounts = Array.isArray(inspected.Mounts) ? inspected.Mounts.filter(isRecord) : [];
  const config = isRecord(inspected.Config) ? inspected.Config : {};
  const hostConfig = isRecord(inspected.HostConfig) ? inspected.HostConfig : {};
  const state = isRecord(inspected.State) ? inspected.State : {};
  const sensitiveTargets = new Set([LEGACY_SUBSCRIPTION_TOKEN_TARGET, '/root/.claude/.credentials.json']);
  const unsafeCredentialExposure = mounts.some(mount => sensitiveTargets.has(String(mount.Destination)))
    || stringArray(config.Env).some(value => /^(?:ANTHROPIC_API_KEY|CLAUDE_CODE_OAUTH_TOKEN)=/.test(value));
  const capabilities = (values: unknown): string[] => stringArray(values).map(value => value.replace(/^CAP_/, ''));
  const tmpfs = isRecord(hostConfig.Tmpfs)
    ? Object.fromEntries(Object.entries(hostConfig.Tmpfs).filter((entry): entry is [string, string] => typeof entry[1] === 'string')) : {};
  return { id: String(inspected.Id), image: String(inspected.Image), running: state.Running === true,
    networkMode: typeof hostConfig.NetworkMode === 'string' ? hostConfig.NetworkMode : null,
    readonlyRootfs: hostConfig.ReadonlyRootfs === true, tmpfs,
    capAdd: capabilities(hostConfig.CapAdd), capDrop: capabilities(hostConfig.CapDrop),
    securityOpt: stringArray(hostConfig.SecurityOpt).map(option => option.replace(/:true$/, '')),
    pidsLimit: numberOrNull(hostConfig.PidsLimit), nanoCpus: numberOrNull(hostConfig.NanoCpus),
    memoryBytes: numberOrNull(hostConfig.Memory), memorySwapBytes: numberOrNull(hostConfig.MemorySwap),
    mounts: mounts.map(mount => ({ type: String(mount.Type), source: String(mount.Source),
      name: typeof mount.Name === 'string' ? mount.Name : null, destination: String(mount.Destination),
      readOnly: mount.RW !== true })),
    unsafeCredentialExposure };
}

function sameHostPath(left: string, right: string): boolean {
  const normalize = (value: string): string => resolve(value).replaceAll('\\', '/').toLowerCase();
  return normalize(left) === normalize(right);
}

function hasRequiredMounts(container: InspectedContainer, expectedMounts: ContainerMount[]): boolean {
  if (container.mounts.length !== expectedMounts.length) return false;
  return expectedMounts.every(expected => container.mounts.some(actual =>
    actual.type === expected.kind
      && actual.destination === expected.target
      && actual.readOnly === expected.readOnly
      && (expected.kind === 'volume'
        ? actual.name === expected.source
        : sameHostPath(actual.source, expected.source))));
}

function hasRequiredIsolation(container: InspectedContainer, expectedMounts: ContainerMount[]): boolean {
  return container.readonlyRootfs
    && Object.entries(REQUIRED_TMPFS).every(([path, options]) => container.tmpfs[path] === options)
    && Object.keys(container.tmpfs).length === Object.keys(REQUIRED_TMPFS).length
    && REQUIRED_CAPABILITIES.every(capability => container.capAdd.includes(capability))
    && container.capAdd.length === REQUIRED_CAPABILITIES.length
    && container.capDrop.includes('ALL')
    && container.securityOpt.includes('no-new-privileges')
    && container.pidsLimit === BUILD_CONTAINER_RESOURCE_LIMITS.pids
    && container.nanoCpus === BUILD_CONTAINER_RESOURCE_LIMITS.cpuCount * 1_000_000_000
    && container.memoryBytes === BUILD_CONTAINER_RESOURCE_LIMITS.memoryBytes
    && container.memorySwapBytes === BUILD_CONTAINER_RESOURCE_LIMITS.memorySwapBytes
    && container.image === image
    && hasRequiredMounts(container, expectedMounts);
}

const expectedMounts: ContainerMount[] = [
  { kind: 'bind' as const, source: resolve(appDir), target: '/app', readOnly: false },
  ...(projects ? [{ kind: 'bind' as const, source: projects,
    target: `${AGENT_HOME}/.claude/projects/-app`, readOnly: false }] : []),
  ...containerPlan.mounts,
];

// Only the lease's immutable container id grants reuse or deletion authority.
let leaseContext;
try { leaseContext = leaseFromEnv(process.env, { backend, active: true }); }
catch (error) {
  console.error(`run-build.js: an authenticated active backend lease is required: ${errorMessage(error)}`);
  process.exit(3);
}

let existing = inspectContainer(containerName);
const priorContainer = leaseContext.lease.resources.buildContainer ?? null;
if (existing) {
  if (!priorContainer) {
    console.error(`run-build.js: refusing to adopt existing unleased container ${containerName}`);
    process.exit(3);
  }
  if (priorContainer.name !== containerName || priorContainer.id !== existing.id) {
    console.error(`run-build.js: existing container ${containerName}/${existing.id} does not match lease `
      + `${priorContainer.name}/${priorContainer.id}`);
    process.exit(3);
  }
  if (!existing.running) {
    if (!recoverStoppedContainer) {
      console.error(`run-build.js: leased container ${containerName} stopped unexpectedly; refusing to replace it`);
      process.exit(3);
    }
    try {
      leaseContext = recoverStoppedBuildContainer({ existing: { ...existing, running: false }, containerName, leaseContext, backend,
        dockerEnv, timeoutMs: DOCKER_TIMEOUT_MS });
      existing = null;
    } catch (error) {
      console.error(`run-build.js: could not recover stopped container: ${errorMessage(error)}`);
      process.exit(3);
    }
  }
  if (existing && existing.networkMode !== expectedNetworkMode) {
    console.error(`run-build.js: leased container ${containerName} uses network ${existing.networkMode}, `
      + `expected ${expectedNetworkMode}`);
    process.exit(3);
  }
  if (existing?.unsafeCredentialExposure) {
    console.error(`run-build.js: leased container ${containerName} was created with a provider credential; `
      + 'reconcile the run and start it with the isolated credential broker');
    process.exit(3);
  }
  if (existing && !hasRequiredIsolation(existing, expectedMounts)) {
    console.error(`run-build.js: leased container ${containerName} does not have the required isolation`);
    process.exit(3);
  }
} else if (priorContainer) {
  const leasedById = inspectContainer(priorContainer.id);
  if (leasedById) {
    console.error(`run-build.js: leased container ${priorContainer.id} still exists under an unexpected name`);
    process.exit(3);
  }
  if (!recoverStoppedContainer) {
    console.error(`run-build.js: leased container ${priorContainer.name}/${priorContainer.id} is missing`);
    process.exit(3);
  }
  try {
    leaseContext = clearMissingBuildContainerLease({ containerName, leaseContext, backend });
  } catch (error) {
    console.error(`run-build.js: could not recover missing container lease: ${errorMessage(error)}`);
    process.exit(3);
  }
}

// Create it if this is the first round of the run; reuse it for every round
// after, so a fix round finds the app, its node_modules and its servers exactly
// where the build round left them.
let containerInspection = existing;
if (!existing) {
  const creationToken = randomBytes(16).toString('hex');
  const create = [
    'create', '--init', '--name', containerName,
    '--label', `${BUILD_CONTAINER_CREATION_LABEL}=${creationToken}`,
    '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
    '--pids-limit', String(BUILD_CONTAINER_RESOURCE_LIMITS.pids),
    '--cpus', String(BUILD_CONTAINER_RESOURCE_LIMITS.cpuCount),
    '--memory', String(BUILD_CONTAINER_RESOURCE_LIMITS.memoryBytes),
    '--memory-swap', String(BUILD_CONTAINER_RESOURCE_LIMITS.memorySwapBytes),
    // The agent may write the app, its own home directory, and temporary files.
    // It must not replace system binaries or libraries used by later grading.
    '--read-only',
    '-v', `${resolve(appDir)}:/app`,
  ];
  for (const capability of REQUIRED_CAPABILITIES) create.push('--cap-add', capability);
  for (const [path, options] of Object.entries(REQUIRED_TMPFS)) {
    create.push('--tmpfs', `${path}:${options}`);
  }
  create.push('--network', expectedNetworkMode);
  create.push(...dockerHostGatewayArguments(expectedNetworkMode));
  if (projects) create.push('-v', `${projects}:${AGENT_HOME}/.claude/projects/-app`);
  // The selected adapter owns every stack-specific mount. Giving a treatment
  // another stack's artifacts would violate the "only artifacts under test"
  // boundary.
  for (const requiredPath of containerPlan.requiredPaths) {
    if (!existsSync(requiredPath)) {
      console.error(`run-build.js: ${backend} container artifact is missing: ${requiredPath}`);
      process.exit(2);
    }
  }
  for (const mount of containerPlan.mounts) {
    try { create.push(...dockerMountArguments(mount)); }
    catch (error) {
      console.error(`run-build.js: ${backend} adapter returned an invalid container mount: ${errorMessage(error)}`);
      process.exit(2);
    }
  }

  // Publish the track's ports for the host grader.
  if (expectedNetworkMode === 'bridge') for (const p of ports) create.push('-p', `${p}:${p}`);

  // `--init` gives the container a real PID 1. Without it the dev servers the
  // build leaves behind are reparented to `sleep`, which never reaps them.
  const init = 'export HOME=/tmp npm_config_cache=/tmp/npm-cache; '
    + containerPlan.init;
  create.push('-w', '/app', image, 'sh', '-c', init);

  const made = spawnSync('docker', create, {
    encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS,
  });
  if (made.status !== 0) {
    console.error(`run-build.js: could not create ${containerName}`);
    console.error(made.stderr || made.stdout || made.error?.message || '');
    try {
      removeFailedBuildContainer({ containerName, creationToken,
        createdId: containerIdFromDockerOutput(made.stdout), dockerEnv,
        timeoutMs: DOCKER_TIMEOUT_MS });
    } catch (cleanupError) {
      console.error(`run-build.js: ${errorMessage(cleanupError)}`);
      process.exit(3);
    }
    process.exit(2);
  }

  const createdId = containerIdFromDockerOutput(made.stdout);
  try { containerInspection = inspectContainer(containerName); }
  catch (error) {
    console.error(`run-build.js: cannot inspect ${containerName}: ${errorMessage(error)}`);
  }
  if (!containerInspection) {
    try {
      removeFailedBuildContainer({ containerName, creationToken, createdId, dockerEnv,
        timeoutMs: DOCKER_TIMEOUT_MS });
    } catch (cleanupError) {
      console.error(`run-build.js: ${errorMessage(cleanupError)}`);
      process.exit(3);
    }
    console.error(`run-build.js: cannot inspect ${containerName}`);
    process.exit(2);
  }
  if (containerInspection.unsafeCredentialExposure
    || !hasRequiredIsolation(containerInspection, expectedMounts)) {
    try {
      removeFailedBuildContainer({ containerName, creationToken, createdId, dockerEnv,
        timeoutMs: DOCKER_TIMEOUT_MS });
    } catch (cleanupError) {
      console.error(`run-build.js: ${errorMessage(cleanupError)}`);
      process.exit(3);
    }
    console.error(`run-build.js: created container ${containerName} does not have the required isolation`);
    process.exit(2);
  }
}

if (!containerInspection) {
  console.error(`run-build.js: cannot inspect ${containerName}`);
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
      name: containerName, id: containerId, image: containerImage, owned: true, running: existing !== null,
      networkMode: expectedNetworkMode,
      resourceLimits: structuredClone(BUILD_CONTAINER_RESOURCE_LIMITS),
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
  console.error(`run-build.js: ${errorMessage(error)}`);
  process.exit(3);
}

if (!existing) {
  const started = spawnSync('docker', ['start', containerId], {
    encoding: 'utf8', env: dockerEnv, timeout: DOCKER_TIMEOUT_MS,
  });
  if (started.status !== 0) {
    console.error(`run-build.js: could not start leased container ${containerName}/${containerId}`);
    console.error(started.stderr || started.stdout || started.error?.message || '');
    process.exit(2);
  }
  try {
    const { path, lease } = leaseContext;
    updateBackendLease(path, { token: lease.ownershipToken, backend, runId: lease.runId }, next => {
      if (next.resources.buildContainer?.id !== containerId) {
        throw new Error(`leased container changed before start: expected ${containerId}`);
      }
      next.resources.buildContainer.running = true;
      return next;
    });
  } catch (error) {
    console.error(`run-build.js: started container ownership could not be recorded: ${errorMessage(error)}`);
    process.exit(3);
  }
}

if (containerPlan.readyFile) {
  // Wait until SDK staging finishes before starting the paid session.
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
    console.error(`run-build.js: timed out waiting for ${containerPlan.readyDescription ?? `${backend} setup`}`);
    console.error(logs.stderr || logs.stdout || '');
    process.exit(2);
  }
}

if (process.env.STACK_BENCH_APPLIANCE === '1') {
  const writableTargets = [AGENT_HOME,
    ...expectedMounts.filter(mount => !mount.readOnly).map(mount => mount.target)];
  for (const [command, commandArgs] of [
    ['chown', ['-R', `${AGENT_UID}:${AGENT_GID}`, '--', ...writableTargets]],
    ['chmod', ['-R', 'u+rwX,go-rwx', '--', ...writableTargets]],
  ] as const) {
    const permissions = spawnSync('docker', ['exec', containerName, command, ...commandArgs], {
      encoding: 'utf8', env: dockerEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
    });
    if (permissions.status !== 0) {
      console.error(`run-build.js: could not secure writable paths in ${containerName}`);
      console.error(permissions.stderr || permissions.stdout || permissions.error?.message || '');
      process.exit(2);
    }
  }
}

// A nested transcript mount makes Docker create its parent directories as
// root. Confirm that Claude can create its private session state before a
// provider request can spend money.
const homeProbe = spawnSync('docker', [
  'exec', '--user', `${AGENT_UID}:${AGENT_GID}`, '-e', `HOME=${AGENT_HOME}`,
  containerName, 'sh', '-c',
  'umask 077; mkdir -p "$HOME/.claude/session-env" && test -w "$HOME/.claude/session-env"',
], { encoding: 'utf8', env: dockerEnv, timeout: DOCKER_PROBE_TIMEOUT_MS });
if (homeProbe.status !== 0) {
  console.error(`run-build.js: agent home is not writable in ${containerName}`);
  console.error(homeProbe.stderr || homeProbe.stdout || homeProbe.error?.message || '');
  process.exit(2);
}

if (prepareOnly) {
  process.stdout.write(`${JSON.stringify({ containerName,
    identity: `${containerId} ${containerImage}`,
    networkMode: expectedNetworkMode })}\n`);
  process.exit(0);
}

const args = ['exec', '-i', '--user', `${AGENT_UID}:${AGENT_GID}`, '-w', '/app'];

args.push('-e', `HOME=${AGENT_ENVIRONMENT.HOME}`, '-e', `USER=${AGENT_ENVIRONMENT.USER}`,
  '-e', 'DISABLE_AUTOUPDATER=1', '-e', 'FORCE_PROMPT_CACHING_5M=1');
const leasedEnvironment = leasedDatabaseEnvironment(adapter, {
  database: leaseContext.lease.resources.database, networkMode: expectedNetworkMode,
});
for (const [key, value] of Object.entries(leasedEnvironment)) args.push('-e', `${key}=${value}`);
const dockerExecEnv: NodeJS.ProcessEnv = { ...process.env, MSYS_NO_PATHCONV: '1' };
// Forward only benchmark-owned environment settings.
if (process.env.MAX_THINKING_TOKENS) {
  args.push('-e', `MAX_THINKING_TOKENS=${process.env.MAX_THINKING_TOKENS}`);
}

const claudeArgs = [
  '--print', '--output-format', 'json',
  // Isolate the session from project memory, plugins, and integrations.
  '--bare',
  '--permission-mode', 'acceptEdits',
  '--settings', JSON.stringify({ permissions: { allow: ['Bash'] } }),
  '--effort', effort,
  '--model', model,
  ...(maxBudgetUsd !== null ? ['--max-budget-usd', maxBudgetUsd] : []),
  // The app is the only directory a session may reach; inside the container
  // that is all there is, but the flag is kept so host and container runs are
  // configured identically.
  '--add-dir', '/app',
  ...(resumeSession !== null ? ['--resume', resumeSession] : []),
];
// Record the exact remote PID. Killing the local `docker exec` client does not
// guarantee that Claude stops inside the long-lived build container.
const invocationToken = randomBytes(16).toString('hex');
const processRecord = `${CODING_CONTAINER_PROCESS_IDENTITY.recordPrefix}${invocationToken}.pid`;
const claudeWrapper = 'umask 022; record="$1"; shift; '
  + 'start="$(awk \'{print $22}\' /proc/$$/stat)" || exit 1; '
  + 'printf \'%s %s\\n\' "$$" "$start" > "$record"; exec "$@"';

if (!auth) throw new Error('container authentication is unavailable');
let credentialBroker: Awaited<ReturnType<typeof startCredentialBroker>> | null = null;
try {
  credentialBroker = await startCredentialBroker(auth,
    { networkMode: expectedNetworkMode, deadlineMs: CODING_SESSION_TIMEOUT_MS, model,
      maxBudgetUsd: maxBudgetUsd === null ? null : Number(maxBudgetUsd),
      pricingRates: maxBudgetUsd === null ? null : pricing!.rates });
} catch (error) {
  console.error(`run-build.js: ${errorMessage(error)}`);
  process.exit(2);
}
if (!credentialBroker) throw new Error('credential broker is unavailable');
dockerExecEnv.ANTHROPIC_AUTH_TOKEN = credentialBroker.sessionToken;
args.push('-e', 'ANTHROPIC_AUTH_TOKEN', '-e', `ANTHROPIC_BASE_URL=${credentialBroker.baseUrl}`,
  containerName, 'sh', '-c', claudeWrapper, CODING_CONTAINER_PROCESS_IDENTITY.sessionLabel,
  processRecord,
  'claude', ...claudeArgs);

// MSYS_NO_PATHCONV: Git Bash rewrites container-side paths like /app into
// Windows paths (C:/Program Files/Git/app) and every mount silently lands
// somewhere wrong.
if (!projects) throw new Error('transcript directory is unavailable');
const transcriptSnapshot = snapshotClaudeTranscripts(projects);
const promptInput = process.stdin.isTTY ? '' : readFileSync(0, 'utf8');
function signalClaude(signal: 'TERM' | 'KILL') {
  const script = 'record="$1"; signal="$2"; test -r "$record" || exit 4; '
    + 'read -r pid expected < "$record"; '
    + 'current="$(awk \'{print $22}\' "/proc/$pid/stat" 2>/dev/null)" || exit 5; '
    + 'test "$current" = "$expected" || exit 3; kill "-$signal" "$pid"';
  return spawnSync('docker', ['exec', containerName, 'sh', '-c', script,
    CODING_CONTAINER_PROCESS_IDENTITY.stopLabel, processRecord, signal], {
    encoding: 'utf8', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
  });
}
function terminateClaude(child: { kill(signal?: NodeJS.Signals): boolean }): void {
  const term = signalClaude('TERM');
  if (term.status !== 0) child.kill('SIGTERM');
  const force = setTimeout(() => {
    signalClaude('KILL');
    child.kill('SIGKILL');
  }, 5_000);
  force.unref();
}

let res;
let brokerLedger = null;
let brokerDiagnostics = null;
try {
  res = await runTranscriptAwareProcess({ command: 'docker', args,
    input: promptInput,
    maxBuffer: 256 * 1024 * 1024,
    env: dockerExecEnv,
    timeoutMs: CODING_SESSION_TIMEOUT_MS,
    transcriptDirectory: projects,
    transcriptSnapshot,
    marker: completionMarker as string,
    model,
    pricingRates: pricing?.rates ?? null,
    resumeSession: resumeSession ?? undefined,
    terminate: terminateClaude,
  });
} finally {
  brokerLedger = await stopCredentialBroker(credentialBroker);
  brokerDiagnostics = credentialBrokerDiagnostics(credentialBroker);
  if (process.env.STACK_BENCH_APPLIANCE !== '1') {
    spawnSync('docker', ['exec', containerName, ...codingContainerTranscriptHandoffCommand()], {
      stdio: 'ignore', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
    });
  }
  const appMode = process.env.STACK_BENCH_APPLIANCE === '1' ? 'u+rwX,go-rwx' : 'a+rwX';
  spawnSync('docker', ['exec', containerName, 'chmod', '-R', appMode, '/app'], {
    stdio: 'ignore', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
  });
  spawnSync('docker', ['exec', containerName, 'rm', '-f', processRecord], {
    stdio: 'ignore', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
  });
}

let cliResult = null;
const stdout = String(res.stdout ?? '').trim();
try { cliResult = JSON.parse(stdout); }
catch {
  for (const line of stdout.split(/\r?\n/).reverse()) {
    try { cliResult = JSON.parse(line); break; } catch { /* Keep looking. */ }
  }
}
if (maxBudgetUsd !== null) {
  const reconciled = reconcileCredentialBrokerReceipt({
    ledger: brokerLedger,
    cliResult,
    model,
    maxBudgetUsd: Number(maxBudgetUsd),
    pricingRates: pricing!.rates,
    brokerDiagnostics,
  });
  res.stdout = `${JSON.stringify(reconciled.result)}\n`;
  if (!reconciled.ok) {
    res.status = res.status === 0 ? 3 : res.status ?? 3;
    res.stderr = `${res.stderr ?? ''}${res.stderr ? '\n' : ''}`
      + `run-build.js: ${reconciled.receipt.error}\n`;
  }
} else if (cliResult && typeof cliResult === 'object' && !Array.isArray(cliResult)) {
  cliResult.stack_bench_credential_broker = brokerDiagnostics;
  res.stdout = `${JSON.stringify(cliResult)}\n`;
}

if ((res.status ?? 1) !== 0) {
  const state = spawnSync('docker', ['inspect', '--format', '{{json .State}}', containerName], {
    encoding: 'utf8', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
  });
  const memory = spawnSync('docker', ['exec', containerName, 'sh', '-c',
    'for f in memory.events memory.current memory.peak memory.max; do '
      + 'p="/sys/fs/cgroup/$f"; if test -r "$p"; then echo "[$f]"; cat "$p"; fi; done'], {
    encoding: 'utf8', env: dockerExecEnv, timeout: DOCKER_PROBE_TIMEOUT_MS,
  });
  let containerState = null;
  try { containerState = JSON.parse(state.stdout?.trim() || 'null'); } catch { /* retain raw text below */ }
  const diagnostic = {
    schemaVersion: 1,
    kind: 'coding-process-exit',
    status: res.status ?? null,
    signal: res.signal ?? null,
    error: res.error instanceof Error ? res.error.message : null,
    container: containerState ?? { inspectError: state.stderr?.trim()
      || (state.error instanceof Error ? state.error.message : null) },
    cgroupMemory: memory.stdout?.trim() || null,
    cgroupProbeError: memory.status === 0 ? null
      : memory.stderr?.trim() || (memory.error instanceof Error ? memory.error.message : null)
        || `exit ${memory.status}`,
  };
  process.stderr.write(`STACK_BENCH_CODING_PROCESS_DIAGNOSTIC ${JSON.stringify(diagnostic)}\n`);
}

if (res.stdout) process.stdout.write(res.stdout);
if (res.stderr) process.stderr.write(res.stderr);
if (res.error) process.stderr.write(`run-build.js: coding session failed: ${errorMessage(res.error)}\n`);
process.exit(res.status ?? 1);
