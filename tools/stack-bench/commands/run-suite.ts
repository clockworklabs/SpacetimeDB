#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import type { ExecFileSyncOptionsWithStringEncoding, SpawnSyncOptionsWithStringEncoding } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { dbName, loadTrack, suitesFor, DEFAULT_TRACK } from '../src/composition/tracks.js';
import { controlBackendRuntime, parseRuntimeControlSpec }
  from '../src/runtime/backend-control.js';
import type { RuntimeControlSpec } from '../src/runtime/backend-control.js';
import { ARTIFACT_FILE, readArtifactPayload, recipeArtifactIdentities, writeArtifact }
  from '../src/evidence/artifacts.js';
import { bundleRecipeRelease, resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { createBoundRecipeTaskRequest, resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { contractControlIds } from '../src/composition/agent-visible-contract.js';
import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { criterionEvidence, evidencePassed } from '../src/evidence/check-evidence.js';
import { renderEvidenceConsoleLine } from '../src/evidence/evidence-presentation.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from '../src/stacks/backend-reset-guard.js';
import { aggregatePackRuntime, exceededPackBudgets } from '../src/composition/pack-runtime.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { GENERATED_APP_LAYOUT_EXIT_CODE } from '../src/stacks/backend-reset.js';
import { readBackendLease } from '../src/runtime/backend-lease.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { sha256 } from '../src/evidence/provenance.js';
import { GRADER_SOURCE_TIMEOUT_MS } from '../src/runtime/grading-timeout.js';
import type { BackendLease, BackendLeaseExpectation } from '../src/runtime/backend-lease.js';
import type { CheckEvidence } from '../src/evidence/check-evidence.js';
import type { AggregatedPackRuntimeEvidence, PackRuntimeEvidence } from '../src/composition/pack-runtime.js';
import { isModularRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import type { BoundRecipeTaskRequestResult, RecipeSelection } from '../src/composition/recipe-selection.js';
import type { RecipeBinding, RecipeCheck } from '../src/composition/recipe-release.js';
import type { Track, TrackSuite } from '../src/composition/tracks.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../src/package-root.js';
const RESET = compiledEntrypoint('commands', 'reset-backend.js');

type Observation = 'scored' | 'observed';
type Selection = {
  schemaVersion: number;
  recipe: { id: string; version: string; contentSha256: string };
  requested: RecipeSelection['requested'];
  sha256: string;
  checks: RecipeCheck[];
  scoredPoints: number;
  observedChecks?: Array<RecipeCheck & { treatment: string }>;
  observedPoints?: number;
  evaluationSha256?: string;
  regressionChecks?: Array<RecipeCheck & { treatment: string }>;
  regressionPoints?: number;
  observation?: Observation;
};
type DeclaredSuite = TrackSuite;
type Failure = Error & { stdout?: string; stderr?: string; status?: number | null; signal?: string | null;
  code?: string };
type FailureDetail = { message?: unknown; stderr?: unknown } | null;
type RecipeTaskArgument = { recipe: { id: string; version: string; contentSha256?: string } } & Record<string, unknown>;
type RunArguments = {
  app: string;
  url: string;
  backend: string;
  label: string;
  out: string;
  level: string;
  reset: boolean;
  media: boolean;
  runIndex: number;
  track: string;
  packIds: string[];
  checkKeys: string[];
  observation: Observation;
  recipe?: string;
  recipeTask?: RecipeTaskArgument;
  credentialAliases?: unknown;
  regressionChecks: string[];
  sourceSha256?: string;
  restartSpec?: RuntimeControlSpec;
  applicationFailure?: ApplicationFailure;
  parentAttemptId?: string;
  databaseLease?: BackendLease | null;
  selection?: Selection | null;
  bundleArtifactId: string;
};
type GradeCriterion = { id: string; stableKey?: string; serverCheck?: string; evidence?: CheckEvidence };
type GradeFeature = { name: string; criteria: GradeCriterion[] };
type GradePayload = { total: number; max: number; features: GradeFeature[];
  selection?: { checks?: RecipeCheck[] }; packRuntime?: PackRuntimeEvidence };
type LintPayload = { pass: boolean; counts: { pass: number; fail: number; blocked: number } };
type ActionsPayload = { missing: string[]; results: unknown[] };
type RuntimeProvenance = { ok: boolean | null; verified: boolean; reason: string };
type ApplicationProbeResult = { ok: boolean; detail: string | null };
type ResetOutcome = { kind: string; phase: string; appFailures?: string[] };
type ApplicationFailure = ResetOutcome & { kind: 'app_failure'; reason: string };
type DatabaseProvenance = { ok: boolean; reason: string; url?: string };
type GradeLeaseReader = typeof readBackendLease;
type MutationDirectoryEntry = { name: string; isDirectory(): boolean; isFile(): boolean };
type MutationDirectoryReader = (path: string, options: { withFileTypes: true }) => readonly MutationDirectoryEntry[];
type GraderChildResult = { status: number | null; signal: string | null; stdout?: unknown; stderr?: unknown;
  error?: Error };
type GraderChildExecutor = (command: string, argv: readonly string[], options: SpawnSyncOptionsWithStringEncoding) => GraderChildResult;
type ProbeResponse = { ok: boolean; status: number };
type ApplicationFetch = (url: string, init: { signal: AbortSignal }) => Promise<ProbeResponse>;
type DatabaseProvenanceDefinition = Track['databaseProvenance'];
type DatabaseNameLease = { resources: { database?: string | null } };
type ProvenanceFetch = (url: string, init: { method: string; headers: Record<string, string>;
  body: string; signal: AbortSignal }) => Promise<{ ok: boolean; status: number }>;
type ProvenanceWrite = { ok: true; marker: string } | { ok: false; marker: null; reason: string };
type ApplicationFailureSelection = { checks: Array<{ executionId: string; points?: number }> };
type ContractLintArguments = Pick<RunArguments,
  'url' | 'level' | 'track' | 'label' | 'out' | 'bundleArtifactId' | 'credentialAliases'>;
type BundleSelection = Selection & { attemptedChecks: string[]; reportedChecks: string[];
  notRun: Array<{ stableKey: string; reason: string }> };
type Bundle = {
  definitionSchemaVersion: number;
  recipeRelease: ReturnType<typeof bundleRecipeRelease>;
  calibration: { id: string; version: string; state: string; contentSha256: string } | null;
  label: string; track: string; backend: string; url: string; app: string; level: number;
  observation: Observation; source?: { sha256: string };
  suites: Record<string, GradePayload | LintPayload | null>;
  totals: Record<string, unknown>;
  selection: BundleSelection | null;
  code?: ReturnType<typeof codeMetrics>;
  error?: string;
  outcome?: { kind: string; phase: string; reason?: string; appFailures?: string[] };
  provenance?: DatabaseProvenance & { runtime?: RuntimeProvenance };
  actions?: ActionsPayload | null;
  packRuntime?: AggregatedPackRuntimeEvidence;
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const parseObservation = (value: string): Observation => {
  if (value === 'scored' || value === 'observed') return value;
  throw new Error('--observation must be scored or observed');
};

export function suitesForRecipe(track: Track, binding: RecipeBinding): DeclaredSuite[] {
  if (!binding?.execution?.length) throw new Error('recipe has no typed execution plan');
  return binding.execution.map(entry => ({
    id: entry.id,
    spec: resolve(track.dir, entry.source ?? ''),
    ...(entry.ownership.kind === 'inherited'
      ? { inherited: true, fromLevel: entry.ownership.fromLevel }
      : {}),
  }));
}

export function childFailureDetail(failure: FailureDetail = null, stdout = '', limit = 600): string {
  const processOutput = [failure?.stderr, stdout]
    .filter(value => value !== undefined && value !== null && String(value).trim())
    .join('\n').trim();
  const diagnostic = processOutput || String(failure?.message ?? '').trim();
  const lines = diagnostic.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  if (!lines.length) return '';
  const punctuationOnly = (line: string) =>
    [...line].every(character => '[]{},'.includes(character));
  const noise = (line: string) => line.startsWith('at ') || /^Node\.js v/.test(line)
    || /^node:internal\//.test(line) || /^\^+$/.test(line) || punctuationOnly(line);
  const cause = lines.find(line => !noise(line) && /(?:error|failed|timeout|closed|econn|killed)/i.test(line))
    ?? lines.find(line => !noise(line)) ?? lines[0];
  const selected = [cause, ...lines.slice(-4)].filter((line, index, all) => all.indexOf(line) === index);
  return selected.join(' | ').slice(0, limit);
}

export function resetFailureOutcome(error: unknown): ResetOutcome {
  const failure = isRecord(error) ? error : {};
  return failure.status === GENERATED_APP_LAYOUT_EXIT_CODE
    ? { kind: 'app_failure', phase: 'application-layout',
      appFailures: ['application-layout'] }
    : failure.code === 'generated_app_not_restartable'
    ? { kind: 'app_failure', phase: 'application-restart',
      appFailures: ['application-restart'] }
    : { kind: 'harness_failure', phase: 'database-reset' };
}

export function applicationFailureTotals(selection: ApplicationFailureSelection | null | undefined,
  declaredSuites: Array<Pick<DeclaredSuite, 'id' | 'inherited'>>): Record<string, unknown> {
  if (!selection?.checks?.length) return {};
  const inherited = new Set(declaredSuites.filter(suite => suite.inherited).map(suite => suite.id));
  const currentMax = selection.checks.filter(check => !inherited.has(check.executionId))
    .reduce((total, check) => total + Number(check.points ?? 0), 0);
  const regressionMax = selection.checks.filter(check => inherited.has(check.executionId))
    .reduce((total, check) => total + Number(check.points ?? 0), 0);
  return { score: 0, max: currentMax, dirty: false, contractPass: null,
    regression: regressionMax ? { score: 0, max: regressionMax } : null };
}

export function clearPreviousGradeOutputs(output: string): void {
  const generated = existsSync(output) ? readdirSync(output).filter(name =>
    /^grading-.+\.json$/.test(name) || /^grader-.+\.(?:stdout|stderr)\.log$/.test(name)) : [];
  for (const name of [ARTIFACT_FILE.gradeBundle, ARTIFACT_FILE.contractLint,
    ARTIFACT_FILE.actions, 'media', 'failure-media',
    'database-provenance', ...generated]) {
    rmSync(join(output, name), { recursive: true, force: true });
  }
}

export function runGraderChild(argv: string[], output: string, suiteId: string, {
  execute = spawnSync,
}: { execute?: GraderChildExecutor } = {}) {
  const options: SpawnSyncOptionsWithStringEncoding = { encoding: 'utf8', cwd: ROOT, timeout: COMMAND_TIMEOUT_MS,
    maxBuffer: 64 * 1024 * 1024 };
  const result = execute(process.execPath, argv, options);
  const stdout = redactCredentials(String(result.stdout ?? ''));
  const stderr = redactCredentials(String(result.stderr ?? ''));
  const safeId = String(suiteId).replace(/[^A-Za-z0-9._-]/g, '_');
  const stdoutName = `grader-${safeId}.stdout.log`;
  const stderrName = `grader-${safeId}.stderr.log`;
  writeFileSync(join(output, stdoutName), stdout);
  writeFileSync(join(output, stderrName), stderr);
  let failure = result.error ?? null;
  if (!failure && result.status !== 0) {
    failure = new Error(`grader exited ${result.status ?? result.signal ?? 'without status'}`);
  }
  if (failure) Object.assign(failure, { stdout, stderr, status: result.status, signal: result.signal });
  return { stdout, stderr, failure, stdoutName, stderrName };
}

function gradeLeaseInput(backend: string, env: NodeJS.ProcessEnv): { path: string;
  expected: BackendLeaseExpectation } | null {
  if (!['mongodb', 'postgres', 'spacetime'].includes(backend)) return null;
  const path = String(env.STACK_BENCH_LEASE ?? '').trim();
  const token = String(env.STACK_BENCH_LEASE_TOKEN ?? '').trim();
  if (!path && !token) return null;
  if (!path || !token) throw new Error('database grading requires both lease path and lease token');
  return { path, expected: { token, backend, active: true } };
}

export function databaseLeaseForGrading(backend: string, env = process.env, {
  readLease = readBackendLease,
}: { readLease?: GradeLeaseReader } = {}) {
  const input = gradeLeaseInput(backend, env);
  if (!input) return null;
  const lease = readLease(input.path, input.expected);
  if (backend === 'spacetime') {
    if (!lease.resources.module || !lease.resources.serverUri) {
      throw new Error('active spacetime lease has no complete module target');
    }
    return lease;
  }
  const container = String(lease.resources?.container?.name ?? '').trim();
  const containerId = String(lease.resources?.container?.id ?? '').trim();
  if (!container || !containerId) {
    throw new Error(`active ${backend} lease has no complete database container identity`);
  }
  return lease;
}

export function databaseNameForGrading(track: Pick<Track, 'slug'>, runIndex: number,
  lease: DatabaseNameLease | null = null): string {
  if (!lease) return dbName(track, runIndex);
  const database = String(lease.resources?.database ?? '').trim();
  if (!database) throw new Error('active database lease has no database name');
  return database;
}

function parseArgs(argv: string[]): RunArguments {
  const { values } = parseNodeArgs({ args: argv.slice(2), options: {
    app: { type: 'string' }, url: { type: 'string' }, backend: { type: 'string' },
    label: { type: 'string' }, out: { type: 'string' }, level: { type: 'string' },
    recipe: { type: 'string' }, 'recipe-task-json': { type: 'string' },
    'credential-aliases-json': { type: 'string' }, 'regression-checks-json': { type: 'string' },
    observation: { type: 'string' }, 'source-sha256': { type: 'string' },
    'no-media': { type: 'boolean' }, track: { type: 'string' },
    pack: { type: 'string', multiple: true }, check: { type: 'string', multiple: true },
    'restart-spec': { type: 'string' }, 'application-failure-json': { type: 'string' },
    'run-index': { type: 'string' }, 'no-reset': { type: 'boolean' },
    'parent-attempt-id': { type: 'string' },
  } });
  const a: RunArguments = { app: values.app ?? '', url: values.url ?? '',
    backend: values.backend ?? '', label: values.label ?? '', out: values.out ?? '',
    level: values.level ?? '1', reset: !(values['no-reset'] ?? false),
    media: !(values['no-media'] ?? false), runIndex: Number(values['run-index'] ?? 0),
    track: values.track ?? DEFAULT_TRACK,
    packIds: (values.pack ?? []).flatMap(value => value.split(',').filter(Boolean)),
    checkKeys: (values.check ?? []).flatMap(value => value.split(',').filter(Boolean)),
    observation: parseObservation(values.observation ?? 'scored'),
    recipe: values.recipe,
    recipeTask: values['recipe-task-json'] === undefined ? undefined : JSON.parse(values['recipe-task-json']),
    credentialAliases: values['credential-aliases-json'] === undefined
      ? undefined : JSON.parse(values['credential-aliases-json']),
    regressionChecks: values['regression-checks-json'] === undefined
      ? [] : JSON.parse(values['regression-checks-json']),
    sourceSha256: values['source-sha256'],
    restartSpec: values['restart-spec'] === undefined
      ? undefined : parseRuntimeControlSpec(JSON.parse(values['restart-spec'])),
    applicationFailure: values['application-failure-json'] === undefined
      ? undefined : JSON.parse(values['application-failure-json']),
    parentAttemptId: values['parent-attempt-id'], bundleArtifactId: '' };
  if (!a.app || !a.url || !a.backend || !a.label) {
    console.error('Usage: node dist/commands/run-suite.js --app <dir> --url <url> --backend <stack> --label <id> [--out <dir>] [--media] [--no-reset]');
    process.exit(2);
  }
  if (!['scored', 'observed'].includes(a.observation)) {
    throw new Error('--observation must be scored or observed');
  }
  if (a.observation === 'observed' && !/^[a-f0-9]{64}$/.test(a.sourceSha256 ?? '')) {
    throw new Error('observed specifications require --source-sha256');
  }
  if (a.sourceSha256 !== undefined && !/^[a-f0-9]{64}$/.test(a.sourceSha256)) {
    throw new Error('--source-sha256 must be a SHA-256 digest');
  }
  if (a.applicationFailure && (a.applicationFailure.kind !== 'app_failure'
    || typeof a.applicationFailure.phase !== 'string' || !a.applicationFailure.phase
    || typeof a.applicationFailure.reason !== 'string' || !a.applicationFailure.reason)) {
    throw new Error('--application-failure-json must describe an application failure');
  }
  a.out ||= join(a.app, 'stack-bench');
  if (!Array.isArray(a.regressionChecks)
    || a.regressionChecks.some(key => typeof key !== 'string' || !key)) {
    throw new Error('--regression-checks-json must contain stable check keys');
  }
  return a;
}

export function selectObservationScope(selectedTask: BoundRecipeTaskRequestResult | null,
  observation: Observation = 'scored'): Selection | null {
  if (observation === 'scored') return selectedTask?.selection ?? null;
  if (observation !== 'observed') throw new Error(`unknown observation scope ${observation}`);
  if (!selectedTask || !isModularRecipeTaskRequest(selectedTask)) {
    throw new Error('observed specifications require a modular schema-3 task request');
  }
  const selection = selectedTask.selection;
  if (!selection.observedChecks.length) throw new Error('observed specification scope is empty');
  return {
    ...selection,
    observation: 'observed',
    checks: selection.observedChecks,
    scoredPoints: 0,
    observedPoints: selection.observedChecks.reduce((total, check) => total + check.points, 0),
  };
}

export function attachRegressionScope(selection: Selection | null, recipeBinding: RecipeBinding | null,
  declaredSuites: DeclaredSuite[], stableKeys: string[] = []): Selection | null {
  if (!stableKeys.length) return selection;
  if (!selection || !recipeBinding?.release?.checkCatalog) {
    throw new Error('regression checks require a recipe-bound scored selection');
  }
  const uniqueKeys = [...new Set(stableKeys)];
  if (uniqueKeys.length !== stableKeys.length) throw new Error('regression checks contain duplicates');
  const currentKeys = new Set(selection.checks.map(check => check.stableKey));
  const catalog = new Map(recipeBinding.release.checkCatalog
    .map(check => [check.stableKey, check]));
  const inheritedSuites = new Set(declaredSuites.filter(suite => suite.inherited)
    .map(suite => suite.id));
  const regressionChecks = uniqueKeys.map(key => {
    if (currentKeys.has(key)) throw new Error(`regression check ${key} is already in the current score`);
    const check = catalog.get(key);
    if (!check) throw new Error(`regression check ${key} is absent from the cumulative recipe`);
    if (!inheritedSuites.has(check.executionId)) {
      throw new Error(`regression check ${key} does not belong to an inherited execution`);
    }
    return { ...check, treatment: check.treatment ?? 'regression' };
  });
  const evaluationDocument = { schemaVersion: 1, selectionSha256: selection.sha256,
    regressionChecks: uniqueKeys.slice().sort() };
  return {
    ...selection,
    checks: [...selection.checks, ...regressionChecks],
    regressionChecks: regressionChecks.map(check => ({ ...check, treatment: check.treatment ?? 'regression' })),
    regressionPoints: regressionChecks.reduce((total, check) => total + check.points, 0),
    evaluationSha256: sha256(Buffer.from(canonicalDefinitionJson(evaluationDocument))),
  };
}

const sleep = (ms: number): Promise<void> => new Promise(resolveSleep => setTimeout(resolveSleep, ms));
const COMMAND_TIMEOUT_MS = GRADER_SOURCE_TIMEOUT_MS;
const run = (cmd: string, args: readonly string[], opts: Omit<ExecFileSyncOptionsWithStringEncoding, 'encoding'> = {}): string =>
  execFileSync(cmd, args, {
    encoding: 'utf8', stdio: 'pipe', cwd: ROOT, timeout: COMMAND_TIMEOUT_MS, ...opts,
  });

export async function verifyApplicationProbe(url: string, {
  fetchImpl = fetch, timeoutMs = 5000,
}: { fetchImpl?: ApplicationFetch; timeoutMs?: number } = {}): Promise<ApplicationProbeResult> {
  let response;
  try {
    response = await fetchImpl(url, { signal: AbortSignal.timeout(timeoutMs) });
  } catch (error) {
    return { ok: false,
      detail: `application did not respond: ${error instanceof Error ? error.message : String(error)}` };
  }
  if (!response.ok) {
    return { ok: false, detail: `application returned HTTP ${response.status}` };
  }
  return { ok: true, detail: null };
}

export async function waitForApplicationProbe(url: string, {
  attempts = 9, intervalMs = 250, probeTimeoutMs = 1000,
  probe = verifyApplicationProbe, sleepImpl = sleep,
}: { attempts?: number; intervalMs?: number; probeTimeoutMs?: number;
  probe?: typeof verifyApplicationProbe;
  sleepImpl?: (ms: number) => Promise<void> } = {}): Promise<ApplicationProbeResult> {
  if (!Number.isInteger(attempts) || attempts < 1) {
    throw new Error('application probe attempts must be a positive integer');
  }
  let result = null;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    result = await probe(url, { timeoutMs: probeTimeoutMs });
    if (result.ok || attempt === attempts) return result;
    await sleepImpl(intervalMs);
  }
  return result ?? { ok: false, detail: 'application readiness probe did not run' };
}

// Confirm the app uses the database leased to this run.
export function checkDatabaseProvenance(args: Pick<RunArguments, 'app' | 'backend'>): DatabaseProvenance {
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const expected = adapter.ports.allocations().db;
  if (!expected) return { ok: true, reason: 'no external database for this backend' };
  // Neutral guidance does not prescribe project layout. Search the app for the
  // connection string instead of assuming it is in server/.env.
  const urls: string[] = [];
  let usesLeasedEnvironment = false;
  const walk = (dir: string): void => {
    if (!existsSync(dir)) return;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (/^(node_modules|dist|\.vite|\.git|module_bindings)$/.test(e.name)) continue;
      const p = join(dir, e.name);
      if (e.isDirectory()) { walk(p); continue; }
      if (!/\.(env|ts|tsx|js|mjs|json|yaml|yml)$|^\.env/.test(e.name)) continue;
      try {
        const text = readFileSync(p, 'utf8');
        if (/process\.env(?:\.DATABASE_URL|\[['"]DATABASE_URL['"]\])/.test(text)) {
          usesLeasedEnvironment = true;
        }
        urls.push(...adapter.agent.findDatabaseUrls({ text }));
      } catch { /* unreadable file proves nothing */ }
    }
  };
  walk(args.app);
  if (usesLeasedEnvironment) {
    return { ok: true, url: 'process.env.DATABASE_URL',
      reason: 'app reads the database URL supplied by its authenticated backend lease' };
  }
  if (!urls.length) return { ok: false,
    reason: 'app neither reads process.env.DATABASE_URL nor contains a database connection string' };
  const matchesExpectedPort = (value: string): boolean => {
    try { return Number(new URL(value).port) === Number(expected); }
    catch { return false; }
  };
  const ok = urls.some(matchesExpectedPort);
  return { ok, url: urls[0],
    reason: ok ? 'ok' : `app targets ${urls[0]} but the benchmark database is on port ${expected}` };
}

export async function writeApplicationDatabaseMarker(
  args: Pick<RunArguments, 'backend' | 'url'>,
  track: Pick<Track, 'actions'>,
  definition: DatabaseProvenanceDefinition,
  fetchImpl: ProvenanceFetch = fetch,
): Promise<ProvenanceWrite> {
  if (!definition) throw new Error('track does not define runtime database provenance');
  const action = track.actions.find(candidate => candidate.id === definition.action);
  if (!action) throw new Error(`database provenance action is not declared: ${definition.action}`);
  const marker = `sb${randomUUID().replaceAll('-', '').slice(0, 16)}`;
  const body = { ...definition.body, [definition.markerParameter]: marker };
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const request = adapter.namedAction.request({ action,
    input: { values: body },
    url: args.url,
    spacetime: args.backend === 'spacetime' ? adapter.grading.context() : null });
  if (!request.url) throw new Error('database provenance action has no URL');
  try {
    const response = await fetchImpl(request.url, {
      method: request.method,
      headers: { 'Content-Type': 'application/json' },
      body: request.body,
      signal: AbortSignal.timeout(15_000),
    });
    return response.ok ? { ok: true, marker }
      : { ok: false, marker: null,
          reason: `application provenance action returned HTTP ${response.status}` };
  } catch (error) {
    return { ok: false, marker: null,
      reason: `application provenance action failed: ${error instanceof Error ? error.message : String(error)}` };
  }
}

export function databaseProvenanceFailure(error: unknown): { kind: string; phase: string; reason: string } {
  return { kind: 'harness_failure', phase: 'database-provenance',
    reason: `runtime database provenance failed: ${error instanceof Error ? error.message : String(error)}` };
}

export function checkRuntimeDatabaseProvenance(args: Pick<RunArguments, 'backend' | 'databaseLease'>,
  marker: string | null = null): RuntimeProvenance {
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  if (!('proveUse' in adapter.database)) {
    return { ok: null, verified: false,
      reason: 'exact runtime database marker proof is not implemented for this stack' };
  }
  if (!args.databaseLease) {
    return { ok: null, verified: false,
      reason: 'standalone grading has no authenticated database lease' };
  }
  if (typeof marker !== 'string' || !marker) {
    return { ok: null, verified: false,
      reason: 'the application action did not produce a database marker' };
  }
  if (args.backend === 'spacetime') {
    return STACK_ADAPTER_REGISTRY.get('spacetime').database.proveUse(
      { lease: requireLeasedSpacetime(args.databaseLease), marker });
  }
  const lease = requireLeasedDatabase(args.databaseLease);
  return args.backend === 'mongodb'
    ? STACK_ADAPTER_REGISTRY.get('mongodb').database.proveUse({ lease, marker })
    : STACK_ADAPTER_REGISTRY.get('postgres').database.proveUse({ lease, marker });
}

function isGradePayload(value: GradePayload | LintPayload | null | undefined): value is GradePayload {
  return value !== null && value !== undefined && 'total' in value && 'max' in value;
}

// Report the application size and direct runtime dependency count.
export function codeMetrics(args: Pick<RunArguments, 'app' | 'backend'>): { serverLoc: number; serverFiles: number;
  totalLoc: number; totalFiles: number; runtimeDeps: number } {
  // Minimal-guidance apps may place server code outside the conventional directory.
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const conventional = adapter.agent.serverDirectory;
  const SERVER_DIR = existsSync(join(args.app, conventional)) ? conventional : '.';
  const walk = (dir: string, out: string[] = []): string[] => {
    if (!existsSync(dir)) return out;
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (/^(node_modules|dist|\.vite|module_bindings|drizzle)$/.test(e.name)) continue;
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p, out);
      // Count every supported JavaScript and TypeScript source extension.
      else if (/\.(ts|tsx|js|jsx|mjs|cjs)$/.test(e.name)) out.push(p);
    }
    return out;
  };
  const count = (files: string[]): number => files.reduce((n, f) => n + readFileSync(f, 'utf8').split('\n').length, 0);
  // With no conventional server directory, "server" is everything that is not
  // the client — otherwise the fallback counts the client twice and serverLoc
  // equals totalLoc, which reads as a much larger backend than was written.
  const allFiles = walk(args.app);
  const serverFiles = SERVER_DIR === '.'
    ? allFiles.filter(f => !/[\\/]client[\\/]/.test(f))
    : walk(join(args.app, SERVER_DIR));

  let deps = 0;
  const packageFiles = new Set([
    resolve(args.app, 'package.json'),
    resolve(args.app, SERVER_DIR, 'package.json'),
    resolve(args.app, 'client/package.json'),
  ]);
  for (const p of packageFiles) {
    if (!existsSync(p)) continue;
    try { deps += Object.keys(JSON.parse(readFileSync(p, 'utf8')).dependencies ?? {}).length; } catch { /* ignore */ }
  }

  return {
    serverLoc: count(serverFiles), serverFiles: serverFiles.length,
    totalLoc: count(allFiles), totalFiles: allFiles.length,
    runtimeDeps: deps,
  };
}

export function findMutationBackups(app: string, { readDir = readdirSync }:
  { readDir?: MutationDirectoryReader } = {}): string[] {
  const backups: string[] = [];
  const walk = (dir: string): void => {
    let entries;
    try {
      entries = readDir(dir, { withFileTypes: true });
    } catch (error) {
      // Vite atomically replaces transient dependency directories while the
      // app runs. They are not source and may vanish between parent and child
      // reads; a missing directory cannot contain a mutation backup.
      if (isRecord(error) && error.code === 'ENOENT') return;
      throw error;
    }
    for (const entry of entries) {
      if (/^(node_modules|dist|\.vite|\.git|module_bindings)$/.test(entry.name)) continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && entry.name.endsWith('.mutation-backup')) backups.push(path);
    }
  };
  walk(app);
  return backups;
}

function resetDatabase(args: RunArguments): { ok: boolean; detail: string | null;
  outcome: { kind: string; phase: string; appFailures?: string[] } | null } {
  process.stdout.write('  reset database ... ');
  try {
    run(process.execPath, [RESET, args.backend, args.app]);
    console.log('ok');
  } catch (err) {
    console.log('FAILED');
    const failure: Failure = err instanceof Error ? err : new Error(String(err));
    const detail = childFailureDetail(failure, failure.stdout);
    console.log(`    ${detail}`);
    return { ok: false, detail, outcome: resetFailureOutcome(failure) };
  }
  return { ok: true, detail: null, outcome: null };
}

export function contractLintArgv(args: ContractLintArguments,
  selectedTask: BoundRecipeTaskRequestResult | null = null): string[] {
  const controls = selectedTask ? contractControlIds(selectedTask.task.contractText) : [];
  const out = join(args.out, ARTIFACT_FILE.contractLint);
  return [compiledEntrypoint('linter', 'lint.js'), '--url', args.url, '--level', args.level,
      '--track', args.track, '--label', args.label, '--out', out,
      '--parent-attempt-id', args.bundleArtifactId,
      ...(args.credentialAliases
        ? ['--credential-aliases-json', JSON.stringify(args.credentialAliases)] : []),
      ...controls.flatMap(id => ['--hook', id])];
}

function lint(args: RunArguments, selectedTask: BoundRecipeTaskRequestResult | null = null): LintPayload | null {
  process.stdout.write('  contract lint ... ');
  const out = join(args.out, ARTIFACT_FILE.contractLint);
  rmSync(out, { force: true });
  let failure: unknown = null;
  try {
    run('node', contractLintArgv(args, selectedTask));
  } catch (error) { failure = error; /* hook failures still write a report */ }
  if (!existsSync(out)) {
    const output = failure && typeof failure === 'object' && 'stdout' in failure
      ? String(failure.stdout ?? '') : undefined;
    const detail = failure instanceof Error
      ? childFailureDetail(failure, output) : null;
    throw new Error(`contract lint produced no report${detail ? `: ${detail}` : ''}`);
  }
  const r = readArtifactPayload<LintPayload>(out, { expectedKind: 'contract_lint' });
  console.log(r.pass
    ? `PASS (${r.counts.pass} hooks)`
    : `FAIL (${r.counts.fail} failed, ${r.counts.blocked} blocked)`);
  return r;
}

// Named write actions let concurrency checks issue authenticated operations
// without prescribing one transport. Missing actions are reported explicitly.
function checkActions(args: RunArguments): ActionsPayload | null {
  process.stdout.write(`  ${'actions'.padEnd(10)} ... `);
  const out = join(args.out, ARTIFACT_FILE.actions);
  rmSync(out, { force: true });
  try {
    run('node', [compiledEntrypoint('commands', 'check-actions.js'), '--backend', args.backend,
      '--url', args.url, '--app', args.app ?? '.', '--track', args.track, '--out', out, '--quiet',
      '--parent-attempt-id', args.bundleArtifactId]);
  } catch { /* non-zero exit means something is missing; the report still lands */ }
  if (!existsSync(out)) { console.log('NO REPORT'); return null; }
  const r = readArtifactPayload<ActionsPayload>(out, { expectedKind: 'action_check' });
  if (!r.missing.length) { console.log(`all ${r.results.length} present`); return r; }
  console.log(`${r.missing.length} MISSING — ${r.missing.join(', ')}`);
  return r;
}

function gradeSuite(args: RunArguments, suite: DeclaredSuite, track: Track,
  recipeBinding: RecipeBinding | null, bundleArtifactId: string, selectedChecks: RecipeCheck[] = [],
  { recordSelection = true, captureMedia = true, outputDirectory = args.out }: {
    recordSelection?: boolean; captureMedia?: boolean; outputDirectory?: string;
  } = {}): GradePayload {
  process.stdout.write(`  ${suite.id.padEnd(10)} ... `);
  mkdirSync(outputDirectory, { recursive: true });
  const out = join(outputDirectory, `grading-${suite.id}.json`);
  rmSync(out, { force: true });
  const argv = [compiledEntrypoint('grader', 'grade.js'), '--url', args.url, '--level', args.level,
    '--label', `${args.label}-${suite.id}`, '--out', out];
  if (suite.spec) argv.push('--spec', suite.spec);
  argv.push('--backend', args.backend, '--track', args.track);
  if (recipeBinding) argv.push('--expected-recipe-sha256', recipeBinding.release.contentSha256);
  const requestedRecipe = args.recipe ?? (args.recipeTask
    ? `${args.recipeTask.recipe.id}@${args.recipeTask.recipe.version}` : null);
  if (requestedRecipe) argv.push('--recipe', requestedRecipe);
  for (const check of selectedChecks) argv.push('--selected-check', check.stableKey);
  if (args.credentialAliases) {
    argv.push('--credential-aliases-json', JSON.stringify(args.credentialAliases));
  }
  if (recordSelection && args.selection?.sha256) {
    argv.push('--selection-sha256', args.selection.evaluationSha256 ?? args.selection.sha256);
  }
  argv.push('--parent-attempt-id', bundleArtifactId);
  // The out-of-band write goes straight to this run's database, with no
  // app code in the loop; only the harness knows which one that is.
  argv.push('--db-name', databaseNameForGrading(track, args.runIndex ?? 0,
    args.databaseLease?.resources.database ? args.databaseLease : null));
  if (args.restartSpec) argv.push('--restart-spec', JSON.stringify(args.restartSpec));
  // The systems criteria run scripts the app itself ships (back-office writes),
  // so the grader has to know where the app lives.
  if (args.app) argv.push('--app', args.app);
  if (captureMedia && args.media) argv.push('--media', join(outputDirectory, 'media'), '--trace');
  else if (captureMedia) argv.push('--failure-media', join(outputDirectory, 'failure-media'));
  const child = runGraderChild(argv, outputDirectory, suite.id);
  const { stdout, failure } = child;
  if (!existsSync(out)) {
    console.log('NO REPORT');
    const detail = childFailureDetail(failure, stdout);
    throw new Error(`grader produced no report for ${suite.id}${detail ? `: ${detail}` : ''}; `
      + `full diagnostics: ${child.stdoutName}, ${child.stderrName}`);
  }
  const r = readArtifactPayload<GradePayload>(out, { expectedKind: 'grade' });
  if (selectedChecks.length) {
    const expected = selectedChecks.map(check => check.stableKey).sort();
    const reported = (r.selection?.checks ?? []).map(check => check.stableKey).sort();
    if (JSON.stringify(reported) !== JSON.stringify(expected)) {
      throw new Error(`grader report scope differs from requested suite scope for ${suite.id}`);
    }
  }
  console.log(`${r.total}/${r.max}`);
  for (const f of r.features) {
    for (const c of f.criteria.filter(c => !evidencePassed(criterionEvidence(c)))) {
      console.log(`      ${renderEvidenceConsoleLine(criterionEvidence(c), `${f.name} / ${c.id}`, {
        includeSummary: false,
      })}`);
    }
  }
  // Disclose passes that lack server-side confirmation.
  const uiOnly = r.features.flatMap(f =>
    f.criteria.filter(c => evidencePassed(criterionEvidence(c)) && c.serverCheck === 'unverified')
      .map(c => `${f.name}/${c.id}`));
  if (uiOnly.length) {
    console.log(`      note: ${uiOnly.length} criterion/criteria passed on interface behaviour only`);
    for (const u of uiOnly) console.log(`            ${u} — server-side check not runnable on this backend`);
  }
  return r;
}

async function main() {
  const startedAt = new Date().toISOString();
  const args = parseArgs(process.argv);
  args.databaseLease = databaseLeaseForGrading(args.backend);
  const track = loadTrack(args.track);
  const recipeBinding = resolveRecipeRelease(track, Number(args.level), args.recipeTask?.recipe ?? args.recipe);
  if (!recipeBinding && (args.packIds.length || args.checkKeys.length)) {
    throw new Error('--pack and --check require a recipe-bound level');
  }
  const selectedTask = recipeBinding
    ? (args.recipeTask
        ? resolveBoundRecipeTaskRequest(recipeBinding, args.recipeTask)
        : createBoundRecipeTaskRequest(recipeBinding, args))
    : null;
  let selection = selectObservationScope(selectedTask, args.observation);
  if (args.sourceSha256) {
    const source = hashAppSource(args.app);
    if (source.sha256 !== args.sourceSha256) {
      throw new Error('live application source differs from the source selected for grading');
    }
  }
  const declaredSuites = recipeBinding
    ? suitesForRecipe(track, recipeBinding)
    : suitesFor(track, Number(args.level));
  if (args.observation === 'scored') {
    selection = attachRegressionScope(selection, recipeBinding, declaredSuites,
      args.regressionChecks);
  } else if (args.regressionChecks.length) {
    throw new Error('observed grading cannot include regression checks');
  }
  args.selection = selection;
  if (selection) {
    const suiteIds = new Set(declaredSuites.map(suite => suite.id));
    const unmapped = selection.checks.filter(check => !suiteIds.has(check.executionId));
    if (unmapped.length) {
      throw new Error(`selected recipe checks do not map to a declared suite: ${
        unmapped.map(check => check.stableKey).join(', ')}`);
    }
  }
  const calibration = resolveCalibrationForRelease(recipeBinding?.release ?? null, {
    trackRoot: track.dir,
    stackBenchRoot: ROOT,
  });
  const observationSuffix = args.observation === 'observed' ? '-observed' : '';
  const bundleArtifactId = `${args.parentAttemptId ?? args.label}-grade-bundle-l${args.level}${observationSuffix}`;
  args.bundleArtifactId = bundleArtifactId;
  mkdirSync(args.out, { recursive: true });
  // Remove all prior grade output before writing cumulative evidence.
  clearPreviousGradeOutputs(args.out);

  console.log(`\n=== ${args.label} (${args.backend}) ===`);
  console.log(`  app: ${args.app}`);
  console.log(`  url: ${args.url}`);
  if (recipeBinding && selection) {
    console.log(`  recipe: ${recipeBinding.alias} -> ${recipeBinding.release.id}@${recipeBinding.release.version} ` +
      `(${recipeBinding.status}, ${recipeBinding.release.contentSha256.slice(0, 12)})`);
    console.log(args.observation === 'observed'
      ? `  scope: ${selection.checks.length} observed check(s), ${selection.observedPoints} observed point(s), 0 score contribution`
      : `  scope: ${selection.checks.length} check(s), ${selection.scoredPoints} point(s)`);
    if (selection.requested.packs?.length) console.log(`    packs: ${selection.requested.packs.join(', ')}`);
    if (selection.requested.features?.length) {
      console.log(`    features: ${selection.requested.features.join(', ')}`);
    }
    if (selection.requested.checks.length) console.log(`    extra checks: ${selection.requested.checks.join(', ')}`);
  }

  const bundle: Bundle = {
    definitionSchemaVersion: track.schemaVersion,
    recipeRelease: bundleRecipeRelease(recipeBinding),
    calibration: calibration ? { id: calibration.id, version: calibration.version,
      state: calibration.state, contentSha256: calibration.contentSha256 } : null,
    label: args.label, track: args.track, backend: args.backend, url: args.url, app: args.app,
    level: Number(args.level), observation: args.observation,
    ...(args.sourceSha256 ? { source: { sha256: args.sourceSha256 } } : {}),
    suites: {}, totals: {},
    selection: selection ? { ...selection, attemptedChecks: [], reportedChecks: [], notRun: [] } : null,
  };
  const selectedPackIds = new Set(selection?.checks.map(check => check.packId) ?? []);
  const selectedPackDefinitions = recipeBinding?.plan.packs
    .filter(pack => selectedPackIds.has(pack.id)) ?? [];
  const writeBundle = () => {
    if (args.sourceSha256) {
      const current = hashAppSource(args.app);
      if (current.sha256 !== args.sourceSha256) {
        bundle.error = 'application source changed while grading was in progress';
        bundle.outcome = { kind: 'harness_failure', phase: 'source-provenance',
          reason: bundle.error };
      }
    }
    return writeArtifact(join(args.out, ARTIFACT_FILE.gradeBundle), {
      kind: 'grade_bundle',
      id: bundleArtifactId,
      attempt: { id: bundleArtifactId, parentId: args.parentAttemptId ?? null },
      timestamps: { startedAt, completedAt: new Date().toISOString() },
      identities: recipeArtifactIdentities(recipeBinding?.release ?? null, {
        calibration: calibration ? { id: calibration.id, version: calibration.version,
          sha256: calibration.contentSha256, state: calibration.state } : null,
        stackAdapter: { id: args.backend },
      }),
      payload: bundle,
    });
  };
  const recordApplicationAbort = () => {
    bundle.totals = applicationFailureTotals(selection, declaredSuites);
  };
  const freshenFailureMessage = () => lastResetOutcome?.phase === 'application-readiness'
    ? `application did not become ready after database reset${lastResetFailure ? `: ${lastResetFailure}` : ''}`
    : `database reset failed — scores would not be comparable${lastResetFailure ? `: ${lastResetFailure}` : ''}`;
  const markRemainingNotRun = (reason: string): void => {
    if (!bundle.selection) return;
    const accounted = new Set([
      ...bundle.selection.attemptedChecks,
      ...bundle.selection.notRun.map(check => check.stableKey),
    ]);
    bundle.selection.notRun.push(...bundle.selection.checks
      .filter(check => !accounted.has(check.stableKey))
      .map(check => ({ stableKey: check.stableKey, reason })));
  };

  if (args.applicationFailure) {
    bundle.error = args.applicationFailure.reason;
    bundle.outcome = args.applicationFailure;
    recordApplicationAbort();
    markRemainingNotRun(`run aborted: ${bundle.error}`);
    writeBundle();
    console.log(`\nABORTED: ${bundle.error}`);
    process.exit(1);
  }

  // Reset before each stateful check.
  let lastResetFailure: string | null = null;
  let lastResetOutcome: ResetOutcome = { kind: 'harness_failure', phase: 'database-reset' };
  const freshen = async () => {
    if (!args.reset) return true;
    const requiresReseed = STACK_ADAPTER_REGISTRY.get(args.backend).reset.requiresReseed;
    const restartSpec = args.restartSpec;
    if (track.reseedOnReset && restartSpec && requiresReseed) {
      process.stdout.write('  stop application ... ');
      try {
        await controlBackendRuntime(restartSpec, 'stop');
        console.log('ok');
      } catch (error) {
        const failure: Failure = error instanceof Error ? error : new Error(String(error));
        lastResetFailure = childFailureDetail(failure);
        lastResetOutcome = { kind: 'harness_failure', phase: 'application-reset-control' };
        console.log(`FAILED (${lastResetFailure})`);
        return false;
      }
    }
    const reset = resetDatabase(args);
    lastResetFailure = reset.detail;
    lastResetOutcome = reset.outcome ?? { kind: 'harness_failure', phase: 'database-reset' };
    if (!reset.ok) return false;
    // Do not grade until the reset application is reachable.
    const waitUntilReady = async () => {
      const ready = await waitForApplicationProbe(args.url);
      if (!ready.ok) {
        lastResetFailure = ready.detail;
        lastResetOutcome = { kind: 'app_failure', phase: 'application-readiness',
          appFailures: ['application-readiness'] };
        console.log(`FAILED (${ready.detail})`);
        return false;
      }
      console.log('ok');
      return true;
    };
    if (track.reseedOnReset && restartSpec && requiresReseed) {
      process.stdout.write('  restart     ... ');
      // Judge restart success with the readiness probe. The restart command can
      // leave a long-running server process behind, so the command also needs a deadline.
      try {
        // Do not give a background server an inherited pipe that keeps the
        // synchronous restart command open.
        await controlBackendRuntime(restartSpec, 'start');
      } catch (err) {
        const failure: Failure = err instanceof Error ? err : new Error(String(err));
        lastResetOutcome = resetFailureOutcome(failure);
        const detail = ((failure.stderr || '') + (failure.stdout || '') + (failure.message || ''))
          .toString().trim().split('\n').slice(-3).join(' | ').slice(0, 300);
        lastResetFailure = detail || null;
        console.log('FAILED (application did not restart)');
        console.log(`    control: ${JSON.stringify(restartSpec)}`);
        console.log(`    ${detail}`);
        return false;
      }
      return await waitUntilReady();
    }
    process.stdout.write('  ready       ... ');
    return await waitUntilReady();
  };

  bundle.code = codeMetrics(args);
  console.log(`  code        ... ${bundle.code.serverLoc} server LOC in ${bundle.code.serverFiles} files, ` +
    `${bundle.code.totalLoc} total LOC, ${bundle.code.runtimeDeps} runtime deps`);

  // Refuse source left modified by an interrupted mutation run.
  const mutated = findMutationBackups(args.app);
  if (mutated.length) {
    bundle.error = `app still carries mutation backups (${mutated.join(', ')}) — its source is mutated, not the build under test`;
    bundle.outcome = { kind: 'harness_failure', phase: 'mutation-cleanup', reason: bundle.error };
    markRemainingNotRun('run aborted because application source is still mutated');
    writeBundle();
    console.log(`\nABORTED: ${bundle.error}`);
    process.exit(1);
  }

  const prov = checkDatabaseProvenance(args);
  bundle.provenance = prov;
  console.log(`  database    ... ${prov.ok ? 'benchmark-owned' : `WRONG DATABASE — ${prov.reason}`}`);
  if (!prov.ok) {
    bundle.error = `app is not using the benchmark database: ${prov.reason}`;
    bundle.outcome = { kind: 'app_failure', phase: 'database-provenance', reason: bundle.error,
      appFailures: ['database-provenance'] };
    recordApplicationAbort();
    markRemainingNotRun('run aborted because database provenance was invalid');
    writeBundle();
    console.log('\nABORTED: results would not describe the benchmark environment.');
    process.exit(1);
  }

  if (args.observation === 'scored') {
    if (!(await freshen())) {
      bundle.error = freshenFailureMessage();
      bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
      if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
      markRemainingNotRun(`run aborted: ${bundle.error}`);
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    let runtime = checkRuntimeDatabaseProvenance(args);
    let proofError = null;
    let actionFailure: string | null = null;
    const proof = track.databaseProvenance;
    const supportsRuntimeProof = 'proveUse' in STACK_ADAPTER_REGISTRY.get(args.backend).database;
    const requiresRuntimeProof = supportsRuntimeProof && args.databaseLease && args.reset;
    if (requiresRuntimeProof && !proof) {
      proofError = new Error(`${args.track} does not define a runtime database provenance check`);
    } else if (requiresRuntimeProof && proof) {
      try {
        const write = await writeApplicationDatabaseMarker(args, track, proof);
        if (write.ok) runtime = checkRuntimeDatabaseProvenance(args, write.marker);
        else actionFailure = write.reason;
      } catch (error) {
        proofError = error;
      }

      // The proof writes unique data through the application. Remove it before
      // linting and scored grading so the proof cannot change the result.
      if (!(await freshen())) {
        bundle.error = freshenFailureMessage();
        bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
        if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
        markRemainingNotRun(`run aborted: ${bundle.error}`);
        writeBundle();
        console.log(`\nABORTED: ${bundle.error}`);
        process.exit(1);
      }
    } else if (supportsRuntimeProof && !args.reset) {
      runtime = { ok: null, verified: false,
        reason: 'runtime marker proof requires database reset to isolate its write' };
    }

    if (proofError) {
      bundle.outcome = databaseProvenanceFailure(proofError);
      bundle.error = bundle.outcome.reason;
      markRemainingNotRun('run aborted because runtime database provenance could not be verified');
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }

    if (actionFailure) {
      bundle.error = actionFailure;
      bundle.outcome = { kind: 'app_failure', phase: 'database-provenance-action',
        reason: actionFailure, appFailures: ['database-provenance-action'] };
      recordApplicationAbort();
      markRemainingNotRun('run aborted because the application database write failed');
      writeBundle();
      console.log(`\nABORTED: ${actionFailure}`);
      process.exit(1);
    }

    bundle.provenance.runtime = runtime;
    console.log(`  db runtime  ... ${runtime.verified
      ? runtime.ok ? runtime.reason : `WRONG DATABASE — ${runtime.reason}`
      : runtime.reason}`);
    if (runtime.ok === false) {
      bundle.error = `app did not write its marker to the benchmark database: ${runtime.reason}`;
      bundle.outcome = { kind: 'app_failure', phase: 'database-provenance', reason: bundle.error,
        appFailures: ['database-provenance'] };
      recordApplicationAbort();
      markRemainingNotRun('run aborted because runtime database provenance failed');
      writeBundle();
      console.log('\nABORTED: application data came from outside the benchmark database.');
      process.exit(1);
    }
    try {
      bundle.suites.lint = lint(args, selectedTask);
    } catch (error) {
      markRemainingNotRun('run aborted after contract lint failed to produce evidence');
      bundle.error = error instanceof Error ? error.message : String(error);
      bundle.outcome = { kind: 'harness_failure', phase: 'contract-lint', reason: bundle.error };
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    bundle.actions = checkActions(args);
  }

  // Keep current-level score separate from earlier guarantee regressions.
  let total = 0, max = 0, regTotal = 0, regMax = 0;
  const dirty = false;
  for (const suite of declaredSuites) {
    const selectedChecks = selection?.checks.filter(check => check.executionId === suite.id) ?? [];
    if (selection && selectedChecks.length === 0) {
      console.log(`  ${suite.id.padEnd(10)} ... not selected`);
      continue;
    }
    if (!(await freshen())) {
      bundle.error = freshenFailureMessage();
      console.log(`  ${suite.id}: SKIPPED (${bundle.error})`);
      markRemainingNotRun(`run aborted: ${bundle.error}`);
      bundle.outcome = { ...lastResetOutcome, reason: bundle.error };
      if (bundle.outcome.kind === 'app_failure') recordApplicationAbort();
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    if (bundle.selection) {
      bundle.selection.attemptedChecks.push(...selectedChecks.map(check => check.stableKey));
    }
    let r;
    try {
      r = gradeSuite(args, suite, track, recipeBinding, bundleArtifactId, selectedChecks);
    } catch (error) {
      markRemainingNotRun(`run aborted after ${suite.id} grader failure`);
      bundle.error = error instanceof Error ? error.message : String(error);
      bundle.outcome = { kind: 'harness_failure', phase: `grade:${suite.id}`, reason: bundle.error };
      writeBundle();
      console.log(`\nABORTED: ${bundle.error}`);
      process.exit(1);
    }
    bundle.suites[suite.id] = r;
    if (bundle.selection) bundle.selection.reportedChecks.push(...selectedChecks.map(check => check.stableKey));
    if (selection) {
      bundle.packRuntime = aggregatePackRuntime(
        Object.values(bundle.suites).filter(isGradePayload),
        selectedPackDefinitions);
      const exceeded = exceededPackBudgets(bundle.packRuntime);
      if (exceeded.length) {
        // Runtime budgets qualify references; generated apps still receive a complete grade.
        console.log(`  runtime    ... ${exceeded.map(pack =>
          `${pack.id} ${pack.measuredRuntimeMs}ms > ${pack.budget.maxRuntimeMs}ms`)
          .join(', ')} [recorded; grading continues]`);
      }
    }
    if (suite.inherited) { regTotal += r.total; regMax += r.max; }
    else { total += r.total; max += r.max; }
  }

  bundle.totals = {
    score: total, max, dirty, contractPass: isGradePayload(bundle.suites.lint)
      ? null : bundle.suites.lint?.pass ?? null,
    // null rather than 0/0 at L1, where there is nothing earlier to regress.
    regression: regMax ? { score: regTotal, max: regMax } : null,
  };
  writeBundle();

  console.log(`  ${'TOTAL'.padEnd(10)} ... ${total}/${max}${dirty ? '  [DIRTY]' : ''}`);
  if (regMax) {
    const kept = regTotal === regMax ? 'all earlier guarantees still hold' : `${regMax - regTotal} EARLIER GUARANTEE(S) LOST`;
    console.log(`  ${'REGRESSION'.padEnd(10)} ... ${regTotal}/${regMax}  — ${kept}`);
  }
  console.log(`  bundle: ${join(args.out, ARTIFACT_FILE.gradeBundle)}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
