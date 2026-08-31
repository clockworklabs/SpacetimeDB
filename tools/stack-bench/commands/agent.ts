#!/usr/bin/env node

import { execFileSync } from 'node:child_process';
import type { ExecFileSyncOptionsWithStringEncoding } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, realpathSync,
         openSync, readSync, closeSync, readdirSync } from 'node:fs';
import { join, dirname, resolve, relative, isAbsolute, sep } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { loadTrack, levelPrompt, appendix, suitesFor, dbName, moduleName, portsFor, DEFAULT_TRACK } from '../src/composition/tracks.js';
import type { Track, TrackDefinition } from '../src/composition/tracks.js';
import type { BackendLease } from '../src/runtime/backend-lease.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { resolveDefaultGuidanceForStack, type ResolvedSkills }
  from '../src/campaigns/condition-compiler.js';
import type { ExactRecipeRequest, RecipeBinding } from '../src/composition/recipe-release.js';
import { createBoundRecipeTaskRequest, resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { agentVisibleContractText } from '../src/composition/agent-visible-contract.js';
import { leaseFromEnv } from '../src/runtime/backend-lease.js';
import { resolveContainerImage } from '../src/runtime/container-image.js';
import { hashDirectory, sessionProvenance, sha256 } from '../src/evidence/provenance.js';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.js';
import type { StackRunPorts } from '../src/stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { dockerMountArguments } from '../src/runtime/container-mount.js';
import { normalizePromptText, readAgentSkillDocuments, selectAgentSkills } from '../src/agents/agent-materials.js';
import { codingSessionFailure, DEFAULT_THROTTLE_MAX_WAIT_MS, providerSessionFailure,
  runCodingSessionWithRetries } from '../src/agents/coding-session-retry.js';
import type { CodingSessionRetryResult } from '../src/agents/coding-session-retry.js';
import { AGENT_PROCESS_TIMEOUT_MS } from '../src/agents/coding-session-timeouts.js';
import { assertNewOrEmptyDirectory } from '../src/runtime/path-safety.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { PRICING_UNIT, validatePricingAuthority }
  from '../src/evidence/pricing-authority.js';
import type { PricingAuthority } from '../src/evidence/pricing-authority.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../src/package-root.js';
const REPO = resolve(ROOT, '..', '..');
const CONTROL_COMMAND_TIMEOUT_MS = 120_000;
const DEFAULT_CODING_INTERRUPTION_RETRIES = 2;

type UnknownRecord = Record<string, unknown>;
interface GuidanceDocument {
  path: string;
  sha256: string;
  bytes: number;
  applicationInterface: 'http' | 'reducer';
}

interface PromptMaterials {
  skillsText?: string;
  requirementText?: string;
  contractText?: string;
  startingCatalog?: string;
}

type RecipeTaskRequest = Parameters<typeof resolveBoundRecipeTaskRequest>[1] & {
  recipe?: Exclude<ExactRecipeRequest, string>;
};

interface AgentArgs {
  mode: string;
  backend: string;
  app: string;
  level: number;
  runIndex: number;
  model: string;
  guidance: string;
  track: string;
  pricing: Readonly<PricingAuthority> | null;
  guidanceDocument?: GuidanceDocument;
  credentialAliases?: Readonly<Record<string, string>>;
  recipe?: string;
  recipeTask?: RecipeTaskRequest;
  thinking?: string;
  maxBudgetUsd?: number;
  skills?: string[];
  skillIdentity?: ResolvedSkills;
  apiKey?: string;
  printPrompt?: boolean;
}

interface ThinkingVolume {
  blocks: number;
  signatureBytes: number;
  bytesPerBlock: number;
}

interface SessionUsage {
  input_tokens?: number;
  output_tokens?: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
}

const isRecord = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const stringValue = (value: unknown): string | null => typeof value === 'string' ? value : null;

function sessionUsage(value: unknown): SessionUsage {
  return isRecord(value) ? {
    input_tokens: typeof value.input_tokens === 'number' ? value.input_tokens : undefined,
    output_tokens: typeof value.output_tokens === 'number' ? value.output_tokens : undefined,
    cache_creation_input_tokens: typeof value.cache_creation_input_tokens === 'number'
      ? value.cache_creation_input_tokens : undefined,
    cache_read_input_tokens: typeof value.cache_read_input_tokens === 'number'
      ? value.cache_read_input_tokens : undefined,
  } : {};
}

// Use only the benchmark-owned SpacetimeDB host.
const STDB_URI = process.env.STACK_BENCH_STDB_URI ?? 'http://127.0.0.1:3210';

// Test the CLI and SDK from this checkout.
const LOCAL_CLI = join(REPO, 'target', 'release', 'spacetimedb-cli.exe');
const STDB_BIN = process.env.SPACETIME_BIN ?? (existsSync(LOCAL_CLI) ? LOCAL_CLI : 'spacetime');
const LOCAL_PKG = process.env.STDB_PACKAGE ?? join(REPO, 'crates', 'bindings-typescript');

const fwd = (path: string): string => path.split('\\').join('/');

// Keep the provider's default thinking budget unless an experiment selects one
// explicitly. The run records observed reasoning volume so default changes are
// visible in the evidence.
const THINKING_TOKENS = process.env.STACK_BENCH_THINKING ?? null;

const EFFORT = process.env.STACK_BENCH_EFFORT ?? 'high';

const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;

// Containers reach host services through this address. App ports remain local.
export function hostServiceAddress(env: NodeJS.ProcessEnv = process.env): string {
  return env.STACK_BENCH_HOST_ALIAS
    ?? (env.STACK_BENCH_APPLIANCE === '1' ? '127.0.0.1' : 'host.docker.internal');
}

const HOST_ADDR = hostServiceAddress();
const hostUrl = (url: string): string => url.replace(/127\.0\.0\.1|localhost/g, HOST_ADDR);

const C_PKG = '/deps/spacetimedb.tgz';
const C_BIN = '/deps/spacetimedb-cli';

// The container requires the Linux CLI from this checkout.
const LINUX_CLI = process.env.STACK_BENCH_LINUX_CLI
  ?? join(ROOT, 'container', 'bin', 'spacetimedb-cli');

// The transcript exposes reasoning blocks and signature bytes, not reasoning tokens.
function thinkingVolume(appDir: string, sessionId: string | null | undefined): ThinkingVolume | null {
  if (!sessionId) return null;
  try {
    const store = join(homedir(), '.claude', 'projects');
    if (!existsSync(store)) return null;
    const want = resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase();
    const dir = readdirSync(store).find(d => {
      const n = d.toLowerCase();
      return n === want || n === want.replace(/^-+/, '');
    });
    const file = dir && join(store, dir, `${sessionId}.jsonl`);
    if (!file || !existsSync(file)) return null;

    let blocks = 0, bytes = 0;
    for (const line of readFileSync(file, 'utf8').split('\n')) {
      if (!line.includes('"thinking"')) continue;   // cheap filter before parsing
      let record: unknown;
      try { record = JSON.parse(line); } catch { continue; }
      if (!isRecord(record) || !isRecord(record.message)
        || !Array.isArray(record.message.content)) continue;
      for (const content of record.message.content) {
        if (!isRecord(content) || content.type !== 'thinking') continue;
        blocks++;
        bytes += stringValue(content.signature)?.length ?? 0;
      }
    }
    return { blocks, signatureBytes: bytes,
             bytesPerBlock: blocks ? Math.round(bytes / blocks) : 0 };
  } catch { return null; }
}

function combinedThinkingVolume(appDir: string, sessionIds: readonly (string | null | undefined)[]): ThinkingVolume | null {
  const volumes: ThinkingVolume[] = [...new Set(sessionIds.filter((id): id is string => Boolean(id)))]
    .map(id => thinkingVolume(appDir, id)).filter((item): item is ThinkingVolume => item !== null);
  if (!volumes.length) return null;
  const blocks = volumes.reduce((sum, item) => sum + item.blocks, 0);
  const signatureBytes = volumes.reduce((sum, item) => sum + item.signatureBytes, 0);
  return { blocks, signatureBytes,
    bytesPerBlock: blocks ? Math.round(signatureBytes / blocks) : 0 };
}


// Record the Linux CLI executed by the container. The host and container
// binaries can change independently and must not share an identity.
function linuxSpacetimeVersion(image: string): { commit: string | null; binarySha256: string | null; raw: string } {
  try {
    const releaseVolume = process.env.STACK_BENCH_RELEASE_DEPS_VOLUME?.trim() || null;
    const mountArgs = releaseVolume
      ? dockerMountArguments({ kind: 'volume', source: releaseVolume,
        target: '/release-deps', readOnly: true })
      : ['-v', `${LINUX_CLI}:/deps/spacetimedb-cli:ro`];
    const entrypoint = releaseVolume ? '/release-deps/spacetimedb-cli' : '/deps/spacetimedb-cli';
    const out = execFileSync('docker',
      ['run', '--rm', ...mountArgs, '--entrypoint', entrypoint, image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS });
    const commit = out.match(/Commit:\s*([0-9a-f]+)/i)?.[1] ?? null;
    return { commit, binarySha256: sha256(readFileSync(LINUX_CLI)),
      raw: out.trim().split(/\r?\n/).slice(0, 2).join(' ') };
  } catch { return { commit: null, binarySha256: null, raw: 'unknown' }; }
}

function bindingsIdentity(pkgDir: string): { package: string; sourceSha256: string | null; sourceFiles: number } {
  try {
    const p = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8'));
    const source = hashDirectory(pkgDir, { exclude: name =>
      /(^|\/)(node_modules|dist|target)(\/|$)/.test(name) });
    return { package: `${p.name}@${p.version}`, sourceSha256: source.sha256,
      sourceFiles: source.files.length };
  } catch { return { package: 'unknown', sourceSha256: null, sourceFiles: 0 }; }
}

// The CLI version inside the build image. Read by running it, not by trusting
// the tag: the image is pinned by ARG and a tag can be moved.
function imageCliVersion(image: string): string {
  try {
    return execFileSync('docker', ['run', '--rm', '--entrypoint', 'claude', image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
  } catch { return 'unknown'; }
}

function imageNodeVersion(image: string): string {
  try {
    return execFileSync('docker', ['run', '--rm', '--entrypoint', 'node', image, '--version'],
      { encoding: 'utf8', stdio: 'pipe', env: { ...process.env, MSYS_NO_PATHCONV: '1' },
        timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
  } catch { return 'unknown'; }
}

function containerImage(name: string): { reference: string; imageId: string | null | undefined } {
  try {
    const out = execFileSync('docker', ['inspect', '-f', '{{.Config.Image}} {{.Image}}', name],
      { encoding: 'utf8', stdio: 'pipe', timeout: CONTROL_COMMAND_TIMEOUT_MS }).trim();
    const [reference, imageId] = out.split(/\s+/, 2);
    return { reference: reference ?? '', imageId };
  } catch { return { reference: 'unknown', imageId: null }; }
}

// Record ambient provider configuration that can change model behaviour while
// replacing credential values with presence markers.
function ambientEnv(): Record<string, string | undefined> {
  const seen: Record<string, string | undefined> = {};
  for (const [k, v] of Object.entries(process.env)) {
    if (!/^(CLAUDE|ANTHROPIC|MAX_THINKING|DISABLE_AUTOUPDATER|FORCE_PROMPT)/.test(k)) continue;
    // Never record a credential, only that one was present.
    seen[k] = /KEY|TOKEN|SECRET/i.test(k) ? '<redacted, present>' : v;
  }
  return seen;
}

function normalizeGuidance(value: string): 'neutral' | 'prescribed' {
  if (value === 'neutral' || value === 'prescribed') return value;
  throw new Error(`guidance must be neutral or prescribed, received ${JSON.stringify(value)}`);
}

function parseArgs(argv: readonly string[]): AgentArgs {
  const a: Partial<AgentArgs> & Pick<AgentArgs, 'level' | 'runIndex' | 'model' | 'guidance' | 'track'> = { level: 1, runIndex: 0, model: 'claude-sonnet-5', guidance: 'prescribed',
    track: DEFAULT_TRACK };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--mode': a.mode = argv[++i] ?? ''; break;
      case '--track': a.track = argv[++i] ?? ''; break;
      case '--backend': a.backend = argv[++i] ?? ''; break;
      case '--level': a.level = parseInt(argv[++i] ?? '', 10); break;
      case '--app': a.app = argv[++i] ?? ''; break;
      case '--run-index': a.runIndex = parseInt(argv[++i] ?? '', 10); break;
      case '--model': a.model = argv[++i] ?? ''; break;
      case '--pricing-json': a.pricing = JSON.parse(argv[++i] ?? ''); break;
      case '--guidance': a.guidance = normalizeGuidance(argv[++i] ?? ''); break;
      case '--guidance-document-json': a.guidanceDocument = JSON.parse(argv[++i] ?? ''); break;
      case '--credential-aliases-json': a.credentialAliases = JSON.parse(argv[++i] ?? ''); break;
      case '--recipe': a.recipe = argv[++i] ?? ''; break;
      case '--recipe-task-json': a.recipeTask = JSON.parse(argv[++i] ?? ''); break;
      case '--thinking': a.thinking = argv[++i] ?? ''; break;
      case '--max-budget-usd': a.maxBudgetUsd = Number(argv[++i]); break;
      case '--skills': a.skills = (argv[++i] ?? '').split(',').map((skill: string) => skill.trim()).filter(Boolean); break;
      case '--skills-json': a.skills = JSON.parse(argv[++i] ?? ''); break;
      case '--skill-identity-json': a.skillIdentity = validateSkillIdentity(
        JSON.parse(argv[++i] ?? '')); break;
      case '--api-key': a.apiKey = argv[++i] ?? ''; break;
      case '--print-prompt': a.printPrompt = true; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.mode || !a.backend || !a.app) {
    console.error('Usage: node dist/commands/agent.js --mode build|upgrade|fix --backend <b> --app <dir> [--level N]');
    process.exit(2);
  }
  if (a.maxBudgetUsd !== undefined && (!Number.isFinite(a.maxBudgetUsd) || a.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  if (a.pricing !== undefined) {
    a.pricing = validatePricingAuthority(a.pricing, { at: '--pricing-json' });
  } else if (a.maxBudgetUsd !== undefined) {
    const rates = claudeRatesForModel(a.model);
    if (!rates) throw new Error(`no default pricing is recorded for model ${a.model}`);
    a.pricing = validatePricingAuthority({ unit: PRICING_UNIT, rates },
      { at: 'default pricing' });
  } else {
    a.pricing = null;
  }
  return { mode: a.mode, backend: a.backend, app: a.app, level: a.level,
    runIndex: a.runIndex, model: a.model, guidance: a.guidance, track: a.track,
    pricing: a.pricing ?? null, ...(a.guidanceDocument ? { guidanceDocument: a.guidanceDocument } : {}),
    ...(a.credentialAliases ? { credentialAliases: a.credentialAliases } : {}),
    ...(a.recipe ? { recipe: a.recipe } : {}), ...(a.recipeTask ? { recipeTask: a.recipeTask } : {}),
    ...(a.thinking ? { thinking: a.thinking } : {}),
    ...(a.maxBudgetUsd !== undefined ? { maxBudgetUsd: a.maxBudgetUsd } : {}),
    ...(a.skills ? { skills: a.skills } : {}),
    ...(a.skillIdentity ? { skillIdentity: a.skillIdentity } : {}),
    ...(a.apiKey ? { apiKey: a.apiKey } : {}),
    ...(a.printPrompt ? { printPrompt: true } : {}) };
}

const dbUrl = (backend: string, runIndex: number, dbPort: number | null, track: Track): string | null => {
  const url = executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'agent', 'connection-url',
    { dbPort, database: dbName(track, runIndex), hostUrl });
  return typeof url === 'string' ? url : null;
};

// Create the leased database before the app connects. A build clears its schema.
// A reset between suites preserves the schema required by the running app.
interface DatabasePreparationLease {
  runId: string;
  track: string;
  runIndex: number;
  resources: Pick<BackendLease['resources'], 'database'>;
}

type DatabaseCommandOptions = Pick<ExecFileSyncOptionsWithStringEncoding, 'stdio' | 'timeout'>;

type DatabaseCommandExecutor = (command: string, args: readonly string[],
  options: DatabaseCommandOptions) => string;

const databaseCommandExecutor: DatabaseCommandExecutor = (command, args, options) =>
  String(execFileSync(command, args, { ...options, encoding: 'utf8' }));

interface DatabasePreparationOptions {
  exec?: DatabaseCommandExecutor;
  stdbBin?: string;
  lease?: DatabasePreparationLease;
}

export function ensureDatabase(backend: string, runIndex: number, dbPort: number | null,
  track: Pick<TrackDefinition, 'name' | 'slug'>, wipe = false,
  { exec = databaseCommandExecutor, stdbBin = STDB_BIN, lease: suppliedLease }: DatabasePreparationOptions = {}) {
  const lease = suppliedLease ?? leaseFromEnv(process.env, { backend, active: true }).lease;
  if (lease.runIndex !== runIndex || lease.track !== track.name) {
    throw new Error(`backend lease ${lease.runId} belongs to ${lease.track}/run${lease.runIndex}, `
      + `not ${track.name}/run${runIndex}`);
  }
  const expectedName = dbName(track, runIndex);
  const name = lease.resources.database ?? expectedName;
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'database', 'prepare', {
    lease, name, expectedName, wipe, exec, cli: stdbBin,
    expectedServerUri: STDB_URI, expectedModule: moduleName(track, runIndex), dbPort,
  });
}

// Prescribed guidance chooses an implementation stack. Neutral guidance gives
// only stack access facts and the selected API references.
export function readBackendGuidanceDocument(document: GuidanceDocument | undefined, fallbackRelativePath: string): string {
  if (typeof fallbackRelativePath !== 'string' || !fallbackRelativePath) {
    throw new Error('backend guidance fallback path is required');
  }
  if (document !== undefined) {
    const fields = new Set(['path', 'sha256', 'bytes', 'applicationInterface']);
    if (!document || typeof document !== 'object' || Array.isArray(document)
      || Object.keys(document).some(field => !fields.has(field))
      || typeof document.path !== 'string' || !document.path || isAbsolute(document.path)
      || document.path.includes('\\')
      || !/^[a-f0-9]{64}$/.test(document.sha256)
      || !Number.isSafeInteger(document.bytes) || document.bytes < 0
      || !['http', 'reducer'].includes(document.applicationInterface)) {
      throw new Error('campaign guidance document identity is invalid');
    }
  }
  const root = realpathSync(ROOT);
  const candidate = resolve(root, document?.path ?? fallbackRelativePath);
  const candidateRel = relative(root, candidate);
  if (candidateRel === '..' || candidateRel.startsWith(`..${sep}`) || isAbsolute(candidateRel)) {
    throw new Error('campaign guidance document escapes the Stack Bench root');
  }
  const selectedPath = realpathSync(candidate);
  const resolvedRel = relative(root, selectedPath);
  if (resolvedRel === '..' || resolvedRel.startsWith(`..${sep}`) || isAbsolute(resolvedRel)) {
    throw new Error('campaign guidance document resolves outside the Stack Bench root');
  }
  const bytes = Buffer.from(normalizePromptText(readFileSync(selectedPath, 'utf8')), 'utf8');
  if (document && (sha256(bytes) !== document.sha256 || bytes.length !== document.bytes)) {
    throw new Error(`campaign guidance document changed after compilation: ${document.path}`);
  }
  return bytes.toString('utf8');
}

function validateSkillIdentity(value: unknown): ResolvedSkills {
  const fields = new Set(['ids', 'sha256', 'bytes']);
  if (!isRecord(value) || Object.keys(value).some(field => !fields.has(field))
    || !Array.isArray(value.ids) || new Set(value.ids).size !== value.ids.length
    || value.ids.some(id => typeof id !== 'string' || !/^[a-z][a-z0-9-]*$/.test(id))
    || typeof value.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(value.sha256)
    || !Number.isSafeInteger(value.bytes) || Number(value.bytes) < 0) {
    throw new Error('campaign skill identity is invalid');
  }
  return { ids: value.ids as string[], sha256: value.sha256, bytes: Number(value.bytes) };
}

function backendDoc(args: AgentArgs, p: StackRunPorts, track: Track): string {
  const defaultGuidance = resolveDefaultGuidanceForStack(args.guidance, args.backend);
  let defaultPath = defaultGuidance?.documents[args.backend]?.path;
  if (!defaultPath && args.guidance === 'neutral') {
    throw new Error(`neutral guidance has no document for ${args.backend}`);
  }
  defaultPath ??= join('backends', `${args.backend}.md`);
  const raw = readBackendGuidanceDocument(args.guidanceDocument, defaultPath);
  return raw
    .replaceAll('<VITE_PORT>', String(p.vite))
    .replaceAll('<EXPRESS_PORT>', String(p.express ?? ''))
    .replaceAll('<APP_NOUN>', track.title)
    .replaceAll('<MODULE_NAME>', moduleName(track, args.runIndex))
    .replaceAll('<DATABASE_URL>', p.dbPort ? dbUrl(args.backend, args.runIndex, p.dbPort, track) ?? '' : '')
    .replaceAll('<STDB_URI>', hostUrl(STDB_URI))
    .replaceAll('<STDB_BIN>', C_BIN)
    .replaceAll('<STDB_PACKAGE>', `file:${C_PKG}`);
}

// Fail before a paid session when the selected container cannot run this checkout.
function containerBlocker(backend: string): string | null {
  try {
    execFileSync('docker', ['image', 'inspect', IMAGE],
      { stdio: 'pipe', timeout: CONTROL_COMMAND_TIMEOUT_MS });
  } catch (error: unknown) {
    const detail = error instanceof Error ? error.message.split('\n')[0] : String(error).split('\n')[0];
    return `cannot verify isolation image ${IMAGE}: ${detail} — `
      + `build it with docker build -t ${IMAGE} ${fwd(join(ROOT, 'container'))}`;
  }
  if (!executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend),
    'agent', 'linux-cli-required')) return null;
  if (!existsSync(LINUX_CLI)) {
    return `no Linux SpacetimeDB CLI at ${fwd(LINUX_CLI)} — `
      + 'bash tools/stack-bench/container/build-linux-cli.sh';
  }
  // A file at this path must be a Linux executable, not the Windows build.
  const magic = Buffer.alloc(4);
  try {
    const fd = openSync(LINUX_CLI, 'r');
    try { readSync(fd, magic, 0, 4, 0); } finally { closeSync(fd); }
  } catch {
    return `cannot read the Linux SpacetimeDB CLI at ${fwd(LINUX_CLI)}`;
  }
  if (magic.toString('binary') !== '\x7fELF') {
    return `${fwd(LINUX_CLI)} is not a Linux binary; rebuild it with `
      + 'container/build-linux-cli.sh';
  }
  return null;
}

function decideIsolation(args: AgentArgs): { container: true; reason: null } {
  const blocker = containerBlocker(args.backend);
  if (!blocker) return { container: true, reason: null };
  console.error(`agent.js: isolated build unavailable: ${blocker}`);
  console.error('  benchmark coding sessions require the isolation container');
  process.exit(2);
}

// Pin every round to the build's recorded container topology.
function resolveIsolation(args: AgentArgs): { container: true; reason: null } {
  const marker = resolve(args.app, '..', '.stack-bench-isolation');
  const backendMarker = resolve(args.app, '..', '.stack-bench-backend');

  if (args.mode === 'build') {
    const decided = decideIsolation(args);
    if (!args.printPrompt) {
      mkdirSync(dirname(marker), { recursive: true });
      writeFileSync(marker, 'container');
    }
    return decided;
  }

  if (existsSync(marker)) {
    const pinned = readFileSync(marker, 'utf8').trim();
    if (pinned !== 'container') {
      console.error(`agent.js: unsupported isolation marker ${JSON.stringify(pinned)}; expected "container"`);
      process.exit(2);
    }
    const blocker = containerBlocker(args.backend);
    if (blocker) {
      console.error(`agent.js: this run's build ran in a container, but ${blocker}`);
      console.error('  refusing to run this round in a different environment');
      process.exit(2);
    }
    return { container: true, reason: null };
  }

  // A backend marker without an isolation marker is ambiguous prior state.
  if (existsSync(backendMarker)) {
    console.error('agent.js: app has prior benchmark state but no isolation marker');
    console.error('  refusing to guess where earlier rounds ran; start a clean run');
    process.exit(2);
  }
  const decided = decideIsolation(args);
  if (!args.printPrompt) {
    mkdirSync(dirname(marker), { recursive: true });
    writeFileSync(marker, 'container');
  }
  return decided;
}

export function buildPrompt(args: AgentArgs, p: StackRunPorts, track: Track,
  materials: PromptMaterials = {}): string {
  const applicationInterface = args.guidanceDocument?.applicationInterface
    ?? resolveDefaultGuidanceForStack(args.guidance, args.backend)
      ?.documents[args.backend]?.applicationInterface;
  if (applicationInterface !== 'http' && applicationInterface !== 'reducer') {
    throw new Error(`stack ${args.backend} has no application interface`);
  }
  const common = [
    'Build the app in /app.',
    // Published container ports require the app to bind to all interfaces.
    '',
      'The web application must listen on 0.0.0.0, not localhost, so it is reachable '
      + 'outside its process.',
    '',
    '## Stack',
    '',
    agentVisibleContractText(backendDoc(args, p, track), args.credentialAliases,
      applicationInterface),
  ];
  const skills = materials.skillsText ?? readAgentSkillDocuments(REPO, args.skills ?? []);
  if (skills) common.push('', '## Selected API reference', '', skills);

  if (args.mode === 'resume') {
    return [
      'Restore the existing application to a runnable state.',
      '',
      'This is a saved application from an earlier completed run. Install its',
      'dependencies and start its existing database module, server, and web client',
      'as needed. Do not implement features or fix application behavior. Do not',
      'change source files. The saved source must remain byte-for-byte identical.',
      '',
      'Output RESUME_COMPLETE when the existing app is running.',
      '',
      ...common,
    ].join('\n');
  }

  if (args.mode === 'fix') {
    return [
      'Fix the reported application bugs.',
      '',
      'Read BUG_REPORT.md in the app directory. Each entry says what was expected',
      'and what actually happened. Fix the app so the behaviour matches, redeploy,',
      'and make sure the dev server is running.',
      '',
      'Change only what is needed. Do not alter behaviour that is already correct.',
      '',
      'Output FIX_COMPLETE when done.',
      '',
      ...common,
    ].join('\n');
  }

  const verb = args.mode === 'upgrade'
    ? [
        `Add the level ${args.level} features below to the existing app.`,
        '',
        'Keep completed features working. Add only the current features below.',
      ]
    : [`Build the application described below and leave it running.`];
  const startingCatalog = args.mode === 'build' && materials.startingCatalog
    ? ['', '## Starting catalog', '', 'Use exactly this starting data:', '',
        '```json', materials.startingCatalog, '```'] : [];

  return [
    ...verb,
    '',
    `After the web application is running, reply with ${args.mode === 'upgrade'
      ? 'UPGRADE_COMPLETE' : 'DEPLOY_COMPLETE'}.`,
    '',
    ...common,
    '',
    agentVisibleContractText(materials.requirementText ?? levelPrompt(track, args.level),
      args.credentialAliases, applicationInterface),
    ...startingCatalog,
    '',
    '## Application interface',
    '',
    agentVisibleContractText(materials.contractText ?? appendix(track, args.level),
      args.credentialAliases, applicationInterface),
  ].join('\n');
}

export function agentScenarioPaths(track: Track, level: number,
  recipeBinding: RecipeBinding | null = null): string[] {
  const execution = recipeBinding?.execution;
  if (execution) return execution.map(entry => resolve(track.dir, entry.source ?? ''));
  return suitesFor(track, level).map(suite => suite.spec);
}

export function agentRecipeRequest(explicitRecipe: string | null = null,
  recipeTask: RecipeTaskRequest | null = null): ExactRecipeRequest | null {
  const bound = recipeTask?.recipe;
  if (!bound) return explicitRecipe;
  const identity = `${bound.id}@${bound.version}`;
  if (explicitRecipe && explicitRecipe !== identity) {
    throw new Error(`agent recipe ${explicitRecipe} does not match bound task ${identity}`);
  }
  return bound;
}

// The coding container must not contain the controller or grading inputs.

async function main() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const p = portsFor(track, args.backend, args.runIndex);
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const profileSkills = resolveDefaultGuidanceForStack(args.guidance, args.backend)
    ?.skills[args.backend]?.ids;
  const adapterSkills = executeStackCapability(adapter, 'agent', 'default-skills');
  if (!profileSkills && (!Array.isArray(adapterSkills)
    || adapterSkills.some(skill => typeof skill !== 'string'))) {
    throw new Error(`stack adapter ${args.backend} returned invalid default skills`);
  }
  const defaultSkills = profileSkills ?? adapterSkills as string[];
  const selectedSkills = selectAgentSkills(defaultSkills,
    args.skillIdentity?.ids ?? args.skills ?? null);
  const skillsText = readAgentSkillDocuments(REPO, selectedSkills);
  if (args.skillIdentity && (sha256(skillsText) !== args.skillIdentity.sha256
    || Buffer.byteLength(skillsText) !== args.skillIdentity.bytes)) {
    throw new Error('campaign skill material changed after compilation');
  }
  const recipeBinding = resolveRecipeRelease(track, args.level,
    agentRecipeRequest(args.recipe ?? null, args.recipeTask ?? null));
  if (args.recipeTask && !recipeBinding) {
    throw new Error(`L${args.level} has no recipe release for the requested task`);
  }
  const selectedTask = recipeBinding
    ? (args.recipeTask
        ? resolveBoundRecipeTaskRequest(recipeBinding, args.recipeTask)
        : createBoundRecipeTaskRequest(recipeBinding))
    : null;
  const requirementText = selectedTask?.task.requirementText ?? levelPrompt(track, args.level);
  const contractText = selectedTask?.task.contractText ?? appendix(track, args.level);
  const taskMode = isRecord(args.recipeTask?.task) ? args.recipeTask.task.mode : null;
  const startingCatalog = recipeBinding && taskMode === 'fresh' ? JSON.stringify({
    warehouses: recipeBinding.plan.fixture.warehouses,
    items: recipeBinding.plan.fixture.items,
  }, null, 2) : undefined;

  // Print the exact prompt without starting a session or changing the app.
  if (args.printPrompt) {
    process.stdout.write(buildPrompt(args, p, track,
      { skillsText, requirementText, contractText, startingCatalog }));
    return;
  }
  if (args.mode === 'build') {
    assertNewOrEmptyDirectory(args.app, 'build application directory');
  }
  resolveIsolation(args);
  const imageIdentity = resolveContainerImage(IMAGE);
  // Build wipes all backend state. Later rounds preserve it.
  ensureDatabase(args.backend, args.runIndex, p.dbPort, track, args.mode === 'build');
  // Never erase a caller-supplied application tree.
  mkdirSync(args.app, { recursive: true });
  writeFileSync(resolve(args.app, '..', '.stack-bench-backend'), args.backend);

  const prompt = buildPrompt(args, p, track,
    { skillsText, requirementText, contractText, startingCatalog });
  const bugReportPath = join(args.app, 'BUG_REPORT.md');
  const bugReportText = args.mode === 'fix' && existsSync(bugReportPath)
    ? readFileSync(bugReportPath, 'utf8') : null;
  const provenance = sessionProvenance({ prompt, skillsText, contractText, bugReportText,
    scenarioPaths: agentScenarioPaths(track, args.level, recipeBinding),
    trackDir: track.dir, trackManifestPath: join(track.dir, 'track.json') });
  const started = Date.now();
  const retryLimitRaw = process.env.STACK_BENCH_CODING_INTERRUPTION_RETRIES
    ?? String(DEFAULT_CODING_INTERRUPTION_RETRIES);
  const retryLimit = Number(retryLimitRaw);
  if (!Number.isInteger(retryLimit) || retryLimit < 0 || retryLimit > 3) {
    throw new Error('STACK_BENCH_CODING_INTERRUPTION_RETRIES must be an integer from 0 to 3');
  }
  // The throttle wait must fit inside the adapter deadline.
  const throttleWaitRaw = process.env.STACK_BENCH_PROVIDER_THROTTLE_MAX_WAIT_MINUTES
    ?? String(DEFAULT_THROTTLE_MAX_WAIT_MS / 60_000);
  const throttleMaxWaitMinutes = Number(throttleWaitRaw);
  if (!Number.isInteger(throttleMaxWaitMinutes) || throttleMaxWaitMinutes < 0
    || throttleMaxWaitMinutes > DEFAULT_THROTTLE_MAX_WAIT_MS / 60_000) {
    throw new Error('STACK_BENCH_PROVIDER_THROTTLE_MAX_WAIT_MINUTES must be an integer from 0 to '
      + `${DEFAULT_THROTTLE_MAX_WAIT_MS / 60_000}`);
  }
  // Concurrent campaign slots must not wake and retry as one burst. This
  // stable offset keeps retries reproducible while spreading them over 45s.
  const throttleJitterMs = parseInt(sha256(Buffer.from(
    `${args.backend}:${args.runIndex}:${args.level}:${args.mode}`)).slice(0, 8), 16) % 45_001;
  let coding: CodingSessionRetryResult;
  try {
    // Send prompts through stdin to avoid the Windows command-line limit.
    const cliEnv = { ...process.env,
      // Absent unless deliberately overridden — see THINKING_TOKENS above.
      ...((args.thinking ?? THINKING_TOKENS)
        ? { MAX_THINKING_TOKENS: String(args.thinking ?? THINKING_TOKENS) }
        : {}),
      // Keep the CLI fixed across the campaign.
      DISABLE_AUTOUPDATER: '1',
      // Pin cache lifetime so run order cannot change cost.
      FORCE_PROMPT_CACHING_5M: '1' };

    coding = runCodingSessionWithRetries({ prompt, retryLimit, maxBudgetUsd: args.maxBudgetUsd,
      throttleMaxWaitMs: throttleMaxWaitMinutes * 60_000,
      throttleJitterMs,
      invoke: ({ input, maxBudgetUsd, resumeSession, recoverStoppedContainer }) =>
        execFileSync(process.execPath, [
          compiledEntrypoint('container', 'run-build.js'),
          '--app', args.app,
          '--backend', args.backend,
          '--image', imageIdentity.id,
          '--effort', EFFORT,
          '--model', args.model,
          ...(args.pricing ? ['--pricing-json', JSON.stringify(args.pricing)] : []),
          '--completion-marker', args.mode === 'fix' ? 'FIX_COMPLETE'
            : args.mode === 'upgrade' ? 'UPGRADE_COMPLETE'
              : args.mode === 'resume' ? 'RESUME_COMPLETE' : 'DEPLOY_COMPLETE',
          ...(maxBudgetUsd != null ? ['--max-budget-usd', String(maxBudgetUsd)] : []),
          '--ports', [p.vite, p.express].filter(Boolean).join(','),
          ...(resumeSession ? ['--resume-session', resumeSession] : []),
          ...(recoverStoppedContainer ? ['--recover-stopped-container'] : []),
        ], { input, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
          env: { ...cliEnv, ...(args.apiKey ? { STACK_BENCH_AGENT_API_KEY: args.apiKey } : {}) },
          timeout: AGENT_PROCESS_TIMEOUT_MS }),
    });
  } catch (err: unknown) {
    coding = { raw: '', spawnError: codingSessionFailure(isRecord(err) ? err : {}), sessionResults: [],
      interruptions: [], result: { total_cost_usd: 0, num_turns: 0,
        usage: { input_tokens: 0, output_tokens: 0, cache_creation_input_tokens: 0,
          cache_read_input_tokens: 0 }, stack_bench_cost_receipts: [] },
      throttle: { waits: 0, waitedMs: 0, maxWaitMs: throttleMaxWaitMinutes * 60_000,
        jitterMs: throttleJitterMs } };
  }

  const { raw, spawnError, sessionResults, interruptions, result, throttle } = coding;
  const noOutput = !result.session_id && !raw.trim();
  const failed = Boolean(spawnError || noOutput);
  const providerFailure = providerSessionFailure(result);
  const usage = sessionUsage(result.usage);
  const input = usage.input_tokens ?? 0;
  const output = usage.output_tokens ?? 0;
  const cacheWrite = usage.cache_creation_input_tokens ?? 0;
  const cacheRead = usage.cache_read_input_tokens ?? 0;
  const turns = result.num_turns ?? 0;

  // Preserve the cost inputs needed to explain stack differences.
  const setupMetadata = executeStackCapability(adapter, 'agent', 'setup-metadata', {
    imageId: imageIdentity.id,
    localPackage: LOCAL_PKG,
    env: process.env,
    helpers: { linuxSpacetimeVersion, bindingsIdentity, containerImage },
  });
  const out = {
    appDir: args.app,
    mode: args.mode,
    level: args.level,
    track: args.track,
    backend: args.backend,
    model: args.model,
    guidance: args.guidance,
    setup: {
      thinkingTokens: (args.thinking ?? THINKING_TOKENS) ? Number(args.thinking ?? THINKING_TOKENS) : 'cli default',
      permissionMode: 'acceptEdits',
      effort: EFFORT,
      skills: selectedSkills,
      cacheTier: '5m',
      autoUpdater: 'disabled',
      codingInterruptionRetries: { limit: retryLimit,
        used: interruptions.filter(item => item.kind !== 'provider-throttled').length },
      providerThrottle: { maxWaitMinutes: throttleMaxWaitMinutes,
        waits: throttle?.waits ?? 0, waitedMs: throttle?.waitedMs ?? 0,
        jitterMs: throttle?.jitterMs ?? throttleJitterMs },
      cliVersion: imageCliVersion(imageIdentity.id),
      isolation: { mode: 'container', image: imageIdentity.reference,
        imageId: imageIdentity.id, hostAlias: HOST_ADDR },
      auth: (args.apiKey ?? process.env.ANTHROPIC_API_KEY) ? 'api-key'
        : (process.env.CLAUDE_CODE_OAUTH_TOKEN || process.env.CLAUDE_CODE_OAUTH_TOKEN_FILE)
          ? 'subscription-token' : 'not-selected',
      ...(isRecord(setupMetadata) ? setupMetadata : {}),
      env: ambientEnv(),
      node: { orchestrator: process.version, codingContainer: imageNodeVersion(imageIdentity.id) },
      platform: process.platform,
    },
    costUsd: Number((result.total_cost_usd ?? 0).toFixed(6)),
    costReceipts: result.stack_bench_cost_receipts ?? [],
    tokens: input + output + cacheWrite + cacheRead,
    outputTokens: output,
    usage: { input, output, cacheWrite, cacheRead },
    provenance,
    turns,
    promptBytes: Buffer.byteLength(prompt),
    tokensPerTurn: turns ? Math.round((input + output + cacheWrite + cacheRead) / turns) : null,
    thinking: combinedThinkingVolume(args.app, sessionResults.map(item => item.session_id)),
    durationMs: Date.now() - started,
    sessionId: result.session_id ?? null,
    ok: !failed && result.is_error === false,
    providerMetadata: { failureCode: failed
      ? String(spawnError ?? '').startsWith('provider stayed throttled')
        ? 'provider-throttle-exhausted'
        : providerFailure?.code ?? (noOutput ? 'coding-session-no-output' : 'coding-session-failed')
      : result.is_error === true ? 'provider-session-error' : null,
      failure: failed ? {
        providerStatus: result.api_error_status ?? null,
        waitedMs: throttle?.waitedMs ?? 0,
        waits: throttle?.waits ?? 0,
      } : null,
      interruptions, invocations: sessionResults.length,
      terminalRecovery: isRecord(result) ? result.terminal_recovery ?? null : null,
      credentialBroker: result.stack_bench_credential_broker ?? null,
      sessionIds: [...new Set(sessionResults.map(item => item.session_id).filter(Boolean))] },
  };
  console.log(JSON.stringify(out));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(err => { console.error(err); process.exit(1); });
}
