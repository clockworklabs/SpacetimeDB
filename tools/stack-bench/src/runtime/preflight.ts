import { execFileSync, spawnSync } from 'node:child_process';
import {
  existsSync, mkdirSync, openSync, closeSync, readFileSync, readSync, renameSync, rmSync,
  statfsSync, writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

import { AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.js';
import type { AgentAdapter } from '../agents/agent-adapter-contract.js';
import { resolveDefaultGuidanceForStack } from '../campaigns/condition-compiler.js';
import type { RequestedScope } from '../campaigns/condition-compiler.js';
import { isExactImageReference, parseImageId } from './container-image.js';
import { runContainerSmoke } from './container-smoke.js';
import { BUILD_OUTBOUND_DESTINATIONS, preflightResourceFloors } from '../composition/product-config.js';
import { resolveRecipeRelease } from '../composition/recipe-release.js';
import { createBoundRecipeTaskRequest, resolveRecipeSelection } from '../composition/recipe-selection.js';
import { validateFeatureCatalogInput } from '../progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../progression/progression-recipe-selection.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { databaseContainer, isDatabaseContainerBackend } from '../stacks/database-containers.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../stacks/hosted-database-identity.js';
import { assertNoPortCollisions, listTracks, loadTrack, portsFor } from '../composition/tracks.js';
import { packageRegistry } from './package-registry.js';
import { pidsOnPort } from './platform.js';
import { agentSkillPaths, selectAgentSkills } from '../agents/agent-materials.js';
import { STACK_BENCH_RUNNER_PLATFORM } from './runner-environment.js';
import { CODING_CONTAINER_AGENT_CREDENTIAL_FILE } from './coding-container-policy.js';

import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';
const REPO = resolve(ROOT, '..', '..');
const COMPOSE = process.env.STACK_BENCH_COMPOSE_FILE ?? join(ROOT, 'docker-compose.yaml');
const LINUX_CLI = process.env.STACK_BENCH_LINUX_CLI
  ?? join(ROOT, 'container', 'bin', 'spacetimedb-cli');
const GIB = 1024 ** 3;

type CheckStatus = 'pass' | 'fail' | 'warn';

export interface PreflightRequest {
  backends: string[];
  track: string;
  levels?: string;
  levelList: number[];
  runIndex: number;
  parallelism?: number;
  agentAdapter: string;
  guidance: string;
  packIds: string[];
  checkKeys: string[];
  smoke: boolean;
  image: string;
  resultsDir: string;
  recipe?: string;
  report?: string;
  json?: boolean;
  supervisorState?: string;
  requestedScopes?: RequestedScope[];
  featureCatalog?: unknown;
  mode?: { id?: string };
  admittedSmoke?: { id: string; [key: string]: unknown };
  agentSkills?: string[] | null;
}

export interface PreflightCheck {
  id: string;
  status: CheckStatus;
  summary: string;
  remediation?: string;
  evidence?: unknown;
}

export interface PreflightReport {
  schemaVersion: 1;
  generatedAt: string;
  request: Record<string, unknown>;
  ok: boolean;
  summary: { passed: number; failed: number; warnings: number };
  checks: PreflightCheck[];
}

type Command = (file: string, args: string[], options?: Record<string, unknown>) => unknown;
type PortProbe = (port: number | string) => { free: boolean };

interface PreflightDependencies {
  run?: Command;
  env?: NodeJS.ProcessEnv;
  now?: number | (() => number);
  home?: string;
  exists?: (path: string) => boolean;
  statfs?: (path: string) => { bavail: bigint; bsize: bigint };
  pidsOnPort?: (port: number) => Array<number | string>;
  probePort?: PortProbe;
  readCompose?: () => string;
}

interface CredentialStatus {
  ok: boolean;
  kind?: string;
  source?: string | null;
  reason?: string;
  environment?: string | null;
  files?: readonly string[];
  credentialEnvironments?: readonly string[];
  variable?: string;
  path?: string;
}

interface DockerInfo {
  OSType: string;
  Architecture: string;
  ServerVersion?: string;
  NCPU: number;
  MemTotal: number;
  SystemTime: string;
}

interface DockerContainerInspection {
  Image?: string;
  State?: { Running?: boolean; Health?: { Status?: string } };
  NetworkSettings?: { Ports?: Record<string, Array<{ HostPort?: string }>> };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function spacetimeServerUri(value: unknown): string {
  if (!isRecord(value) || !isRecord(value.lease) || typeof value.lease.serverUri !== 'string') {
    throw new Error('SpacetimeDB orchestrator config did not provide a server URI');
  }
  return value.lease.serverUri;
}

function requestIdentity(value: unknown): { selectionSha256: string; taskSha256: string } {
  if (!isRecord(value) || !isRecord(value.selection) || !isRecord(value.task)
    || typeof value.selection.sha256 !== 'string' || typeof value.task.sha256 !== 'string') {
    throw new Error('resolved request is missing selection or task identity');
  }
  return { selectionSha256: value.selection.sha256, taskSha256: value.task.sha256 };
}

export function verifyPostgresServiceIdentity(
  containerName: string,
  { execute = execFileSync }: { execute?: Command } = {},
): string {
  const { user, password, defaultDatabase: database } = POSTGRES_APPLICATION_IDENTITY;
  const output = String(execute('docker', ['exec', '-e', `PGPASSWORD=${password}`, containerName,
    'psql', '-h', '127.0.0.1', '-U', user, '-d', database,
    '-v', 'ON_ERROR_STOP=1', '-Atc', "SELECT current_user || '|' || current_database();"],
  { encoding: 'utf8', stdio: 'pipe' })).trim();
  const expected = `${user}|${database}`;
  if (output !== expected) throw new Error(`expected ${expected}, received ${output || 'no output'}`);
  return expected;
}

function commandRunner(run: Command): (file: string, args: string[]) => string {
  return (file: string, args: string[]): string => String(run(file, args, {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 120_000,
  })).trim();
}

function bytes(value: number | bigint): string {
  return `${(Number(value) / GIB).toFixed(1)} GiB`;
}

function composeImageReference(service: string, text: string): string | null {
  const section = text.match(new RegExp(`^  ${service}:\\r?\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:|^volumes:)`, 'm'))?.[1] ?? '';
  return section.match(/^ {4}image:\s*(\S+)\s*$/m)?.[1] ?? null;
}

function checkResult(
  id: string,
  status: CheckStatus,
  summary: string,
  remediation: string | null = null,
  evidence: unknown = null,
): PreflightCheck {
  return { id, status, summary, ...(remediation ? { remediation } : {}),
    ...(evidence ? { evidence } : {}) };
}

function credentialReady(
  adapter: AgentAdapter,
  env: NodeJS.ProcessEnv,
  home: string,
  exists: (path: string) => boolean,
): CredentialStatus {
  const environment = adapter.apiKeyEnvironmentVariable;
  let apiKey = null;
  if (environment && env[environment]) {
    apiKey = { ok: true, kind: 'api-key', source: `environment:${environment}` };
  }
  const keyFileVariable = environment ? `${environment}_FILE` : null;
  if (!apiKey && keyFileVariable && env[keyFileVariable] && exists(resolve(env[keyFileVariable]))) {
    apiKey = { ok: true, kind: 'api-key', source: `secret-file:${keyFileVariable}` };
  }
  let environmentCredential = null;
  for (const variable of adapter.credentialEnvironmentVariables) {
    const fileVariable = `${variable}_FILE`;
    const candidate = env[variable]
      ? { ok: true, kind: 'credential-environment', source: `environment:${variable}`, variable }
      : env[fileVariable] && exists(resolve(env[fileVariable]))
        ? { ok: true, kind: 'credential-secret-file', source: `secret-file:${fileVariable}`,
          variable, path: resolve(env[fileVariable]) }
        : null;
    if (candidate && environmentCredential) {
      return { ok: false, source: null, reason: 'Multiple non-API credential sources are selected' };
    }
    environmentCredential = candidate ?? environmentCredential;
  }
  if (apiKey && environmentCredential) {
    return { ok: false, source: null, reason: 'API-key and subscription credentials are both selected' };
  }
  if (apiKey) return apiKey;
  if (environmentCredential) return environmentCredential;
  const file = adapter.credentialFiles.find(relative => exists(join(home, relative)));
  if (file) return { ok: true, kind: 'credential-file',
    source: `file:${file.replaceAll('\\', '/')}`, path: join(home, file) };
  if (!environment && adapter.credentialEnvironmentVariables.length === 0
    && adapter.credentialFiles.length === 0) {
    return { ok: true, kind: 'not-required', source: 'not-required' };
  }
  return { ok: false, source: null, environment, files: adapter.credentialFiles,
    credentialEnvironments: adapter.credentialEnvironmentVariables };
}

function inspectLinuxCli(path: string = LINUX_CLI): { ok: boolean; arch: string | null } {
  if (!existsSync(path)) return { ok: false, arch: null };
  const header = Buffer.alloc(20);
  try {
    const descriptor = openSync(path, 'r');
    try { readSync(descriptor, header, 0, header.length, 0); } finally { closeSync(descriptor); }
    if (!header.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
      return { ok: false, arch: null };
    }
    const machine = header.readUInt16LE(18);
    return { ok: [0x3e, 0xb7].includes(machine), arch: machine === 0x3e ? 'x86_64'
      : machine === 0xb7 ? 'aarch64' : `machine-${machine}` };
  } catch { return { ok: false, arch: null }; }
}

export function probeLoopbackPort(
  port: number | string,
  { spawn = spawnSync }: { spawn?: typeof spawnSync } = {},
): { free: boolean } {
  if (!Number.isInteger(Number(port)) || Number(port) < 1 || Number(port) > 65535) {
    throw new Error(`invalid TCP port ${port}`);
  }
  const script = "const net=require('node:net');const s=net.createServer();"
    + "s.once('error',e=>{console.error(e.code||e.message);process.exit(e.code==='EADDRINUSE'?3:2)});"
    + "s.listen(Number(process.argv[1]),'127.0.0.1',()=>s.close(()=>process.exit(0)));";
  const result = spawn(process.execPath, ['-e', script, String(port)], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 10_000,
  });
  if (result.status === 0) return { free: true };
  if (result.status === 3 && String(result.stderr).includes('EADDRINUSE')) return { free: false };
  throw new Error(String(result.stderr || result.error?.message || `probe exited ${result.status}`).trim());
}

export function runPreflight(
  request: PreflightRequest,
  dependencies: PreflightDependencies = {},
): PreflightReport {
  const run = commandRunner(dependencies.run ?? execFileSync);
  const env = dependencies.env ?? process.env;
  const suppliedNow = dependencies.now;
  const readNow: () => number = typeof suppliedNow === 'function'
    ? suppliedNow : suppliedNow === undefined ? Date.now : () => suppliedNow;
  const startedAt = readNow();
  const home = dependencies.home ?? homedir();
  const exists = dependencies.exists ?? existsSync;
  const statfs = dependencies.statfs ?? ((path: string) => statfsSync(path, { bigint: true }));
  const inspectPorts = dependencies.pidsOnPort ?? pidsOnPort;
  const probePort = dependencies.probePort ?? probeLoopbackPort;
  const checks: PreflightCheck[] = [];
  const resourceFloors = preflightResourceFloors(request.parallelism ?? 1);
  const add = (...args: Parameters<typeof checkResult>): void => {
    checks.push(checkResult(...args));
  };
  let track;
  let agent;

  try {
    agent = AGENT_ADAPTER_REGISTRY.get(request.agentAdapter);
    for (const backend of request.backends) STACK_ADAPTER_REGISTRY.get(backend);
    if (!listTracks({ includeInternal: true }).includes(request.track)) {
      throw new Error(`unknown track ${JSON.stringify(request.track)}`);
    }
    track = loadTrack(request.track);
    assertNoPortCollisions();
    if (request.requestedScopes?.length) {
      const featureCatalog = request.featureCatalog
        ? validateFeatureCatalogInput(request.featureCatalog) : null;
      for (const scope of request.requestedScopes) {
        if (scope?.track !== request.track || !Array.isArray(scope.levels)
          || scope.levels.length === 0
          || JSON.stringify(scope.levels.map(entry => entry.level).sort((a, b) => a - b))
            !== JSON.stringify([...request.levelList].sort((a, b) => a - b))) {
          throw new Error('requested scope does not match the preflight track and levels');
        }
        for (const requested of scope.levels) {
          const recipe = `${requested.recipe.id}@${requested.recipe.version}`;
          const binding = resolveRecipeRelease(track, requested.level, recipe);
          if (!binding || binding.release.contentSha256 !== requested.recipe.contentSha256) {
            throw new Error(`L${requested.level} requested recipe ${recipe} changed`);
          }
          const selection = requested.selection.requested;
          const resolvedRequest = featureCatalog
            ? resolveProgressionRecipeLevelSelection(binding, featureCatalog, requested.level,
              { cumulative: request.mode?.id === 'dependency' }).grader.request
            : createBoundRecipeTaskRequest(binding, selection.features
              ? { featureIds: selection.features,
                  requestedSpecifications: selection.specifications?.requested,
                  expectedSpecifications: selection.specifications?.expected,
                  observedSpecifications: selection.specifications?.observed,
                  checkKeys: selection.checks }
              : { packIds: selection.packs, checkKeys: selection.checks }).request;
          const identity = requestIdentity(resolvedRequest);
          if (identity.selectionSha256 !== requested.selection.sha256
            || identity.taskSha256 !== requested.task.sha256) {
            throw new Error(`L${requested.level} requested scope changed`);
          }
        }
      }
    } else {
      for (const level of request.levelList) {
        const binding = resolveRecipeRelease(track, level, request.recipe);
        if (!binding && ((request.packIds ?? []).length || (request.checkKeys ?? []).length)) {
          throw new Error(`L${level} has no recipe release for --pack/--check selection`);
        }
        if (binding) resolveRecipeSelection(binding.release, request);
      }
    }
    add('request.scope', 'pass', `${request.track} L${request.levelList.join(',L')} on ${request.backends.join(', ')}`);
  } catch (error) {
    add('request.scope', 'fail', errorMessage(error),
      'Correct the requested track, level, pack, check, or adapter.');
  }

  const nodeMajor = Number(process.versions.node.split('.')[0]);
  add('runner.node', nodeMajor >= 22 ? 'pass' : 'fail', `Node ${process.versions.node}`,
    nodeMajor >= 22 ? null : 'Install Node 22 or newer.');
  const appliance = env.STACK_BENCH_APPLIANCE === '1';
  if (appliance) {
    const controllerReference = env.STACK_BENCH_CONTROLLER_IMAGE;
    const controllerPinned = isExactImageReference(controllerReference);
    const buildPinned = isExactImageReference(request.image);
    add('image.controller-reference', controllerPinned ? 'pass' : 'fail',
      controllerPinned
        ? `Controller image is digest-pinned: ${controllerReference}`
        : 'Controller image is not digest-pinned',
      controllerPinned ? null
        : 'Set STACK_BENCH_CONTROLLER_IMAGE to an exact image@sha256 digest reference.');
    add('image.build-reference', buildPinned ? 'pass' : 'fail',
      buildPinned
        ? `Build image is digest-pinned: ${request.image}`
        : 'Build image is not digest-pinned',
      buildPinned ? null
        : 'Set STACK_BENCH_IMAGE to an exact image@sha256 digest reference.');
  }
  const supportedHost = appliance
    ? process.platform === 'linux' && process.arch === 'x64'
    : ['win32', 'linux'].includes(process.platform) && ['x64', 'arm64'].includes(process.arch);
  add('runner.architecture', supportedHost ? 'pass' : 'fail', `${process.platform}/${process.arch}`,
    supportedHost ? null : appliance
      ? `Run the appliance on its supported ${STACK_BENCH_RUNNER_PLATFORM} dedicated runner.`
      : 'Use a supported x64 or arm64 Windows/Linux host.');

  for (const name of ['STACK_BENCH_LEASE', 'STACK_BENCH_LEASE_TOKEN']) {
    if (env[name]) add(`ambient.${name.toLowerCase()}`, 'fail', `${name} is already set`,
      `Unset ${name}; preflight must not inherit another run's ownership state.`);
  }
  if (env.STACK_BENCH_SUPERVISOR_STATE) {
    const expected = request.supervisorState
      && resolve(request.supervisorState) === resolve(env.STACK_BENCH_SUPERVISOR_STATE)
      && !exists(resolve(env.STACK_BENCH_SUPERVISOR_STATE));
    add('ambient.stack_bench_supervisor_state', expected ? 'pass' : 'fail', expected
      ? 'Fresh supervisor handoff path is reserved for this run'
      : 'STACK_BENCH_SUPERVISOR_STATE is inherited or already exists', expected ? null
        : 'Unset inherited supervisor state; supervised callers must reserve a fresh path for this exact run.');
  }
  if (env.DOCKER_HOST && /^(?:tcp|ssh):/i.test(env.DOCKER_HOST)) {
    add('ambient.docker-host', 'fail', 'Remote Docker endpoint is configured',
      'Use a local Docker engine; bind mounts and host routing are part of the measured environment.');
  } else add('ambient.docker-host', 'pass', 'Docker endpoint is local/default');

  const auth = agent ? credentialReady(agent, env, home, exists) : { ok: false };
  add('agent.credentials', auth.ok ? 'pass' : 'fail', auth.ok
    ? `Credential source available (${auth.source})`
    : auth.reason ?? 'No declared credential source is available',
  auth.ok ? null : auth.reason
    ? 'Select exactly one credential mode.'
    : `Set ${[auth.environment, ...(auth.credentialEnvironments ?? [])].filter(Boolean).join(' or ')}`
      + ` or install one of: ${(auth.files ?? []).join(', ')}`);

  mkdirSync(request.resultsDir, { recursive: true });
  const hostMarker = `.preflight-host-${process.pid}-${Math.random().toString(16).slice(2)}`;
  try {
    writeFileSync(join(request.resultsDir, hostMarker), 'host-write-ok', { flag: 'wx', mode: 0o600 });
    rmSync(join(request.resultsDir, hostMarker));
    const stats = statfs(request.resultsDir);
    const free = stats.bavail * stats.bsize;
    add('storage.results', free >= BigInt(resourceFloors.resultDiskBytes) ? 'pass' : 'fail',
      `${bytes(free)} free at ${request.resultsDir}`,
      free >= BigInt(resourceFloors.resultDiskBytes) ? null
        : `Provide at least ${bytes(resourceFloors.resultDiskBytes)} free for persistent results.`);
  } catch (error) {
    add('storage.results', 'fail', `Result directory is not safely writable: ${errorMessage(error)}`,
      'Choose a persistent writable result directory with --results-dir.');
  }

  let dockerInfo: DockerInfo | null = null;
  let imageId: string | null = null;
  try {
    const dockerInfoStartedAt = readNow();
    const info: DockerInfo = JSON.parse(run('docker', ['info', '--format', '{{json .}}']));
    dockerInfo = info;
    const dockerInfoFinishedAt = readNow();
    const ok = info.OSType === 'linux' && (appliance
      ? info.Architecture === 'x86_64'
      : ['x86_64', 'aarch64'].includes(info.Architecture));
    add('docker.engine', ok ? 'pass' : 'fail',
      `Docker ${info.ServerVersion ?? 'unknown'} (${info.OSType}/${info.Architecture})`,
      ok ? null : 'Use a supported x86_64 or aarch64 Linux-container Docker engine.');
    try {
      add('docker.compose', 'pass', `Docker Compose ${run('docker', ['compose', 'version', '--short'])}`);
    } catch {
      add('docker.compose', 'fail', 'Docker Compose plugin is unavailable',
        'Install the Docker Compose v2 plugin.');
    }
    // Coding containers share the runner's network with everything else on
    // it. Another workload is a fact the evidence must carry.
    try {
      const coResident = run('docker', ['ps', '--format', '{{.Names}}']).split(/\r?\n/)
        .map(name => name.trim()).filter(name => name && !name.startsWith('stack-bench'));
      add('runner.co-resident', coResident.length ? 'warn' : 'pass', coResident.length
        ? `${coResident.length} co-resident container(s): ${coResident.slice(0, 6).join(', ')}`
        : 'no co-resident containers',
      coResident.length ? 'Run paid campaigns on a dedicated runner.' : null);
    } catch (error) {
      add('runner.co-resident', 'warn', `could not list containers: ${errorMessage(error)}`,
        'Run paid campaigns on a dedicated runner.');
    }
    const enoughCpu = info.NCPU >= resourceFloors.cpuCount;
    add('docker.cpu', enoughCpu ? 'pass' : 'fail', `${info.NCPU} CPUs available`,
      enoughCpu ? null : `Allocate at least ${resourceFloors.cpuCount} CPUs to Docker.`);
    const enoughMemory = info.MemTotal >= resourceFloors.memoryBytes;
    add('docker.memory', enoughMemory ? 'pass' : 'fail',
      `${bytes(info.MemTotal)} available`,
      enoughMemory ? null : `Allocate at least ${bytes(resourceFloors.memoryBytes)} to Docker.`);
    const engineTime = Date.parse(info.SystemTime);
    const skew = !Number.isFinite(engineTime) ? Number.NaN
      : engineTime < dockerInfoStartedAt ? dockerInfoStartedAt - engineTime
        : engineTime > dockerInfoFinishedAt ? engineTime - dockerInfoFinishedAt : 0;
    const clockReady = Number.isFinite(skew) && skew <= resourceFloors.clockSkewMs;
    add('docker.clock', clockReady ? 'pass' : 'fail',
      `host/engine skew ${Number.isFinite(skew) ? skew : 'unknown'} ms`,
      clockReady ? null : 'Synchronize the host and Docker clocks.');
  } catch (error) {
    add('docker.engine', 'fail', `Docker is unavailable: ${errorMessage(error).split('\n')[0]}`,
      'Start a local Docker engine and ensure the docker CLI can reach it.');
  }

  if (dockerInfo) {
    try {
      imageId = parseImageId(run('docker', ['image', 'inspect', '--format', '{{.Id}}', request.image]));
      add('image.build', 'pass', `${request.image} -> ${imageId}`);
      const imagePlatform = run('docker', ['image', 'inspect', '--format', '{{.Os}}/{{.Architecture}}', request.image]);
      const expectedArch = dockerInfo.Architecture === 'x86_64' ? 'amd64' : 'arm64';
      add('image.platform', imagePlatform === `linux/${expectedArch}` ? 'pass' : 'fail', imagePlatform,
        imagePlatform === `linux/${expectedArch}` ? null : 'Build or pull the isolation image for the Docker engine architecture.');
    } catch {
      add('image.build', 'fail', `Build image ${request.image} is unavailable or unidentifiable`,
        `Build it with: docker build -t ${request.image} ${join(ROOT, 'container')}`);
    }
  }

  if (!request.smoke && request.admittedSmoke) {
    add('smoke.admission', 'pass',
      `Container smoke was admitted by ${request.admittedSmoke.id}`,
      null, { admission: request.admittedSmoke });
  } else if (!request.smoke) add('outbound.container', 'warn',
    'Container outbound access and result-volume persistence were not exercised',
    'Run the same command with --smoke before a paid campaign.');

  // Package installs in a session and in every clean-source start go through
  // the cache; without it they depend on the public registry at run time.
  try {
    const registry = packageRegistry(env);
    if (!registry) {
      add('registry.cache', env.STACK_BENCH_APPLIANCE === '1' ? 'fail' : 'warn',
        'No package registry cache; installs use the public registry directly',
        'Start the npm-cache service and set STACK_BENCH_NPM_REGISTRY.');
    } else {
      const status = run('curl', ['-sS', '-m', '5', '-o', '/dev/null', '-w', '%{http_code}',
        `${registry.href}-/ping`]).trim();
      add('registry.cache', status === '200' ? 'pass' : 'fail', `${registry.href} answered ${status || 'nothing'}`,
        status === '200' ? null : 'Start the npm-cache service before running.');
    }
  } catch (error) {
    add('registry.cache', 'fail', errorMessage(error), 'Start the npm-cache service before running.');
  }

  const composeText = exists(COMPOSE) ? String(dependencies.readCompose?.() ?? readFileSync(COMPOSE, 'utf8')) : '';
  for (const backend of (track ? request.backends : []).filter(isDatabaseContainerBackend)) {
    if (!track) continue;
    const service = backend;
    const container = databaseContainer(backend, env);
    const reference = composeImageReference(service, composeText);
    if (!reference || !/@sha256:[a-f0-9]{64}$/.test(reference)) {
      add(`image.${backend}`, 'fail', `${service} Compose image is not digest-pinned`,
        'Pin the service image to an exact sha256 digest.');
      continue;
    }
    try {
      const expectedId = parseImageId(run('docker', ['image', 'inspect', '--format', '{{.Id}}', reference]));
      const inspections: DockerContainerInspection[] = JSON.parse(
        run('docker', ['container', 'inspect', container.name]),
      );
      const inspected = inspections[0];
      if (!inspected) throw new Error(`${container.name} inspection returned no container`);
      const allocated = portsFor(track, backend, request.runIndex);
      const hostPort = String(allocated.dbPort);
      const mapping = inspected.NetworkSettings?.Ports?.[`${container.internalPort}/tcp`] ?? [];
      const healthy = !inspected.State?.Health || inspected.State.Health.Status === 'healthy';
      const ready = inspected.State?.Running === true && healthy && inspected.Image === expectedId
        && mapping.some((item: { HostPort?: string }) => item.HostPort === hostPort);
      add(`service.${backend}`, ready ? 'pass' : 'fail', ready
        ? `${container.name} is running exact image ${expectedId}`
        : `${container.name} is absent, stopped/unhealthy, on the wrong image, or mapped to the wrong port`,
      ready ? null : `Run docker compose -f ${COMPOSE} up -d ${service}.`);
      if (backend === 'postgres' && ready) {
        try {
          const identity = verifyPostgresServiceIdentity(container.name, { execute: run });
          add('service.postgres.identity', 'pass', `Authenticated as ${identity}`);
        } catch (error) {
          add('service.postgres.identity', 'fail',
            `PostgreSQL application identity is unavailable: ${errorMessage(error)}`,
            `Recreate or migrate the Stack Bench PostgreSQL volume, then restart ${container.name}.`);
        }
      }
    } catch {
      add(`service.${backend}`, 'fail', `${container.name} is not ready`,
        `Run docker compose -f ${COMPOSE} up -d ${service}.`);
    }
  }

  if (track) {
    for (const backend of request.backends) {
      const adapter = STACK_ADAPTER_REGISTRY.get(backend);
      const profileSkills = resolveDefaultGuidanceForStack(request.guidance, backend)
        ?.skills[backend]?.ids;
      const defaultSkills = profileSkills ?? [...adapter.agent.defaultSkills];
      const skills = agent?.usesStackSkills
        ? selectAgentSkills(defaultSkills, request.agentSkills ?? null) : [];
      const missingSkills = agentSkillPaths(REPO, skills).filter(path => !exists(path));
      add(`materials.${backend}.skills`, missingSkills.length ? 'fail' : 'pass', skills.length
        ? `${skills.length} selected skill document(s) are present`
        : 'No skill documents are required', missingSkills.length
          ? `Install the selected skill documents in ${join(REPO, 'skills')} before running.` : null,
      { skills, missing: missingSkills });
      const ports = portsFor(track, backend, request.runIndex);
      for (const [role, port] of Object.entries(ports)) {
        if (port == null || role === 'db' || role === 'dbPort') continue;
        try {
          const availability = probePort(port);
          const listeners = availability.free ? [] : inspectPorts(port);
          add(`port.${backend}.${role}`, availability.free ? 'pass' : 'fail', availability.free
            ? `Port ${port} is free`
            : `Port ${port} is already in use${listeners.length ? ` by PID(s) ${listeners.join(', ')}` : ''}`,
          availability.free ? null : 'Stop the listener or choose a different --run-index.');
        } catch {
          add(`port.${backend}.${role}`, 'fail', `Cannot prove port ${port} is free`,
            'Install netstat on Windows or lsof/ss on Linux so port ownership can be checked.');
        }
      }
    }
  }

  if (request.backends.includes('spacetime')) {
    try {
      const runtime = STACK_ADAPTER_REGISTRY.get('spacetime').orchestrator.config(
        { root: ROOT, env, helpers: { exists } });
      const port = Number(new URL(spacetimeServerUri(runtime)).port);
      const availability = probePort(port);
      const listeners = availability.free ? [] : inspectPorts(port);
      add('port.spacetime.host', availability.free ? 'pass' : 'fail', availability.free
        ? `Dedicated host port ${port} is free`
        : `Dedicated host port ${port} is already in use${listeners.length ? ` by PID(s) ${listeners.join(', ')}` : ''}`,
      availability.free ? null : 'Stop the listener or choose another loopback STACK_BENCH_STDB_URI port.');
    } catch (error) {
      add('port.spacetime.host', 'fail',
        `Cannot validate the dedicated SpacetimeDB host port: ${errorMessage(error)}`,
        'Use an explicit, free loopback STACK_BENCH_STDB_URI port.');
    }
    const cli = inspectLinuxCli();
    const expectedCliArch = dockerInfo?.Architecture ?? null;
    const cliReady = cli.ok && (!expectedCliArch || cli.arch === expectedCliArch);
    add('spacetime.linux-cli', cliReady ? 'pass' : 'fail', cliReady
      ? `Repository Linux CLI is ELF ${cli.arch}`
      : `Repository Linux CLI is missing, invalid, or ${cli.arch ?? 'unknown'} instead of ${expectedCliArch ?? 'the target architecture'}`,
    cliReady ? null : 'Run bash tools/stack-bench/container/build-linux-cli.sh on the target architecture.');
  }

  if (request.smoke && imageId) {
    const marker = `.preflight-container-${process.pid}-${Math.random().toString(16).slice(2)}`;
    const destinations = [...new Set([...BUILD_OUTBOUND_DESTINATIONS,
      ...(agent?.outboundDestinations ?? [])])].sort();
    const tcpPorts = track ? request.backends.filter(backend => ['postgres', 'mongodb'].includes(backend))
      .map(backend => portsFor(track, backend, request.runIndex).dbPort)
      .filter((port): port is number => typeof port === 'number').sort((a, b) => a - b) : [];
    const credentialStatusCommand = auth.kind !== undefined && ['credential-file',
      'credential-environment', 'credential-secret-file'].includes(auth.kind)
      ? agent?.credentialStatusCommand ?? null : null;
    const credentialMount = auth.kind === 'credential-file' && credentialStatusCommand
      && auth.path && auth.source
      ? { kind: 'bind', source: auth.path,
        target: `/root/${auth.source.slice('file:'.length)}`, readOnly: true }
      : auth.kind === 'credential-secret-file' && credentialStatusCommand && auth.path
        ? { kind: 'bind', source: auth.path,
          target: CODING_CONTAINER_AGENT_CREDENTIAL_FILE, readOnly: true }
        : null;
    const credentialEnvironment = auth.kind !== undefined
      && ['credential-environment', 'credential-secret-file'].includes(auth.kind)
      && credentialStatusCommand && auth.variable ? { name: auth.variable,
        ...(auth.kind === 'credential-secret-file'
          ? { file: CODING_CONTAINER_AGENT_CREDENTIAL_FILE }
          : {}) }
        : null;
    try {
      const smoke = runContainerSmoke({ command: run, imageId, resultsDir: request.resultsDir,
        destinations, tcpPorts, requiredExecutables: agent?.requiredExecutables ?? [],
        credentialStatusCommand, credentialMount, credentialEnvironment, marker,
        networkMode: env.STACK_BENCH_APPLIANCE === '1' ? 'host' : 'bridge' });
      const persisted = exists(join(request.resultsDir, marker));
      rmSync(join(request.resultsDir, marker), { force: true });
      const smokeReady = smoke.platform === 'linux' && Number(smoke.node?.match(/^v(\d+)/)?.[1]) >= 22
        && persisted && JSON.stringify(smoke.tcpReached) === JSON.stringify(tcpPorts)
        && JSON.stringify(Object.keys(smoke.executables ?? {}).sort())
          === JSON.stringify([...(agent?.requiredExecutables ?? [])].sort());
      add('smoke.container', smokeReady ? 'pass' : 'fail',
        `Container ${smoke.platform}/${smoke.arch} ${smoke.node}; ${smoke.reached.length} outbound destination(s); `
          + `${smoke.tcpReached?.length ?? 0} database route(s); `
          + `${Object.keys(smoke.executables ?? {}).length} agent executable(s); `
          + `result mount ${persisted ? 'persistent' : 'failed'}`,
      smokeReady ? null : 'Fix the agent executable, container Node/runtime, networking, or persistent results mount.', smoke);
      add('storage.container', smoke.diskFreeBytes >= resourceFloors.resultDiskBytes ? 'pass' : 'fail',
        `${bytes(smoke.diskFreeBytes)} free in Docker storage`,
        smoke.diskFreeBytes >= resourceFloors.resultDiskBytes ? null
          : `Provide at least ${bytes(resourceFloors.resultDiskBytes)} free in Docker storage.`);
      if (credentialStatusCommand) add('agent.authentication',
        smoke.credentialStatus === 'ready' ? 'pass' : 'fail',
        smoke.credentialStatus === 'ready' ? 'Local agent credential status is ready in the build image'
          : 'Agent credential is not logged in for the selected billing mode',
      smoke.credentialStatus === 'ready' ? null
        : 'Refresh the selected agent login before starting a campaign.');
      if (smoke.credentialStatus === 'ready') add('agent.authentication-provider', 'warn',
        'No-model preflight did not ask the provider to accept the credential',
        'Refresh/login before a campaign if the credential has not completed a recent provider request.');
    } catch (error) {
      rmSync(join(request.resultsDir, marker), { force: true });
      add('smoke.container', 'fail',
        `No-model container smoke failed: ${errorMessage(error).split('\n')[0]}`,
        'Fix build-image startup, outbound TLS/DNS, host routing, or the result-volume mount.');
    }
  }

  const report: PreflightReport = {
    schemaVersion: 1,
    generatedAt: new Date(startedAt).toISOString(),
    request: { backends: request.backends, track: request.track, levels: request.levelList,
      runIndex: request.runIndex, parallelism: request.parallelism ?? 1,
      agentAdapter: request.agentAdapter, guidance: request.guidance, packs: request.packIds,
      checks: request.checkKeys, recipe: request.recipe ?? null,
      requestedScopeCount: request.requestedScopes?.length ?? 0,
      image: request.image, resultsDir: request.resultsDir,
      agentSkills: request.agentSkills ?? null,
      smoke: request.smoke,
      admittedSmoke: request.admittedSmoke ?? null },
    ok: !checks.some(check => check.status === 'fail'),
    summary: { passed: checks.filter(check => check.status === 'pass').length,
      failed: checks.filter(check => check.status === 'fail').length,
      warnings: checks.filter(check => check.status === 'warn').length },
    checks,
  };
  return report;
}

export function writePreflightReport(path: string, report: PreflightReport): void {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
  renameSync(temporary, path);
}
