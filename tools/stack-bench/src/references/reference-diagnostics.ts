// Model-free diagnostic case groups. Grades remain zero-point raw evidence.
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { parseArgs } from 'node:util';
import { z } from 'zod';
import { compileScenarioDefinition } from '../composition/definition-compiler.js';
import type { CompiledScenarioDefinition } from '../composition/definition-compiler.js';
import { dbName, loadTrack, moduleName, portsFor } from '../composition/tracks.js';
import { readArtifact, readGradeArtifactPayload, validateGradePayload } from '../evidence/artifacts.js';
import { compiledEntrypoint, STACK_BENCH_ROOT } from '../package-root.js';
import { backendResourceLockKeys, claimBackendResources, createBackendLease,
  existingResourceLockKeys, readBackendLease, releaseResourceLocks, resourceLockScope,
  writeBackendLease } from '../runtime/backend-lease.js';
import { captureApplicationDiagnostics, controlAppServer } from '../runtime/backend-control.js';
import { releaseBackendLease } from '../runtime/backend-teardown.js';
import { runBounded } from '../runtime/bounded-process.js';
import { probeLoopbackPort } from '../runtime/preflight.js';
import { selectRunResources } from '../runtime/run-resource-selection.js';
import { controllerRunner } from '../runtime/runner-environment.js';
import { hashAppSource, restoreAppSource, snapshotAppSource } from '../runtime/source-snapshot.js';
import { inspectSavedDiagnostic, savedDiagnosticSchema } from '../runtime/saved-diagnostic.js';
import { materializeAcceptedSource } from '../runtime/source-materialization.js';
import { activateAttemptBackend } from '../stacks/hosted-lifecycle.js';
import { resetMutationDatabase } from '../../grader/mutation-test.js';
import { inspectImportedReference, loadReferenceRegistry } from './reference-fixtures.js';
import { assertReferenceAuthentication, resolveReferenceSelection } from './reference-selection.js';

const hash = (value: string) => createHash('sha256').update(value).digest('hex');
const read = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
// Dispatch records point to raw grade artifacts; they never manufacture grade results.
function writeJson(path: string, value: unknown): void {
  const temporary = `${path}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`);
  renameSync(temporary, path);
}
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const positive = z.number().int().positive().safe();
const sourceSchema = z.object({ path: z.string().min(1), sha256: z.string().regex(/^[a-f0-9]{64}$/) }).strict();
const planSchema = z.object({ schemaVersion: z.literal(1), groups: z.array(z.object({
  backend: z.enum(['postgres', 'mongodb', 'spacetime']), track: z.string().min(1),
  level: positive, recipe: z.string().min(1), scenario: z.string().min(1),
  features: z.array(positive).nonempty(), repetitions: positive,
  source: sourceSchema.optional(), expectedFailures: z.array(z.string().min(1)).default([]),
  saved: savedDiagnosticSchema.optional(),
}).strict()).nonempty() }).strict();

export interface DiagnosticTrial { key: string; feature: number; repetition: number }
interface DiagnosticGroup {
  backend: string; track: string; level: number; recipe: string;
  fixture: string; referenceSha256: string; source?: z.infer<typeof sourceSchema>;
  saved?: ReturnType<typeof inspectSavedDiagnostic>;
  scenario: CompiledScenarioDefinition; scenarioSha256: string;
  expectedFailures: string[]; trials: DiagnosticTrial[];
}
interface FrozenPlan { schemaVersion: 1; groups: DiagnosticGroup[]; sha256: string }
interface TrialResult extends DiagnosticTrial {
  executionId: string; state: 'started' | 'collected' | 'interrupted';
  startedAt: string; grade?: string; resetMs?: number; gradeMs?: number; error?: string;
  checkOutcomes?: Record<string, number>;
}
interface GroupAudit {
  id: string; planSha256: string; groupIndex: number; backend: string;
  sourceSha256: string; scenarioSha256: string; controllerImage: string;
  plannedTrials: DiagnosticTrial[]; trials: TrialResult[]; startedAt: string;
  finishedAt?: string; runIndex?: number; error?: string; released?: boolean;
  phases: Record<string, number>; sourceAfter?: string; applicationDiagnostics?: unknown;
}

export function validateDiagnosticGroup(plan: FrozenPlan, index: number, audit: GroupAudit,
  image: string, directory: string, complete: boolean): void {
  const group = plan.groups[index];
  if (!group || audit.planSha256 !== plan.sha256 || audit.groupIndex !== index || audit.backend !== group.backend
    || audit.controllerImage !== image || audit.sourceSha256 !== (group.source?.sha256 ?? group.referenceSha256)
    || audit.scenarioSha256 !== group.scenarioSha256) throw new Error('worker identity mismatch');
  auditDiagnosticTrials(group.trials, audit.trials);
  if (!complete) return;
  const population = diagnosticPopulation(group.trials, audit.trials);
  if (!audit.finishedAt || audit.error || !audit.released || audit.sourceAfter !== audit.sourceSha256
    || population.collected !== population.planned || population.executions !== population.planned) {
    throw new Error('collected worker has incomplete or invalid evidence');
  }
  const gradeIds = new Set<string>();
  for (const trial of audit.trials) {
    if (!trial.grade || resolve(trial.grade) !== resolve(directory, `${trial.executionId}.json`)) {
      throw new Error('grade path does not match its execution');
    }
    const artifact = readArtifact(trial.grade, { expectedKind: 'grade' });
    if (gradeIds.has(artifact.id)) throw new Error('duplicate grade artifact');
    gradeIds.add(artifact.id);
    const payload = validateGradePayload(artifact.payload);
    auditDiagnosticGrade({ payload }, trial.feature,
      group.scenario.features.find(feature => feature.id === trial.feature)!.criteria.map(check => check.id), group.expectedFailures,
      Boolean(group.saved));
    // Counts are derived from the raw artifact, including inconclusive checks.
    trial.checkOutcomes = {};
    for (const feature of payload.features) for (const check of feature.criteria) {
      const status = check.evidence.status;
      trial.checkOutcomes[status] = (trial.checkOutcomes[status] ?? 0) + 1;
    }
  }
}

export function diagnosticTrials(identity: string, features: readonly number[], repetitions: number): DiagnosticTrial[] {
  if (!Number.isSafeInteger(repetitions) || repetitions < 1 || !features.length
    || features.some(id => !Number.isSafeInteger(id) || id < 1)
    || new Set(features).size !== features.length) throw new Error('invalid diagnostic trial selection');
  return features.flatMap(feature => Array.from({ length: repetitions }, (_, index) => ({
    key: hash(`${identity}:${feature}:${index + 1}`), feature, repetition: index + 1,
  })));
}

export function freezeDiagnosticPlan(input: unknown, base: string): FrozenPlan {
  const request = planSchema.parse(input);
  const registry = loadReferenceRegistry();
  const keys = new Set<string>();
  const groups = request.groups.flatMap(entry => {
    if (entry.saved && entry.source) throw new Error('saved diagnostics cannot also replace a reference source');
    const saved = entry.saved ? inspectSavedDiagnostic(entry.saved, base) : undefined;
    if (saved && (entry.backend !== saved.backend || entry.track !== saved.track || entry.level !== 3)) {
      throw new Error('saved diagnostic assignment differs from its accepted run');
    }
    const fixture = saved ? undefined : resolveReferenceSelection(registry, entry).fixture;
    const inspection = fixture ? inspectImportedReference(fixture) : undefined;
    if (fixture && (!inspection?.ok || !inspection.sourceSha256)) throw new Error(`invalid reference ${fixture.id}`);
    if (fixture) assertReferenceAuthentication(fixture.id, inspection?.requiredEnvironment ?? []);
    const scenarioText = readFileSync(resolve(base, entry.scenario), 'utf8');
    const scenario = compileScenarioDefinition(JSON.parse(scenarioText), { source: entry.scenario });
    const selected = scenario.features.filter(feature => entry.features.includes(feature.id));
    if (selected.length !== entry.features.length
      || selected.some(feature => feature.criteria.some(check => check.points !== 0))) {
      throw new Error('diagnostic selection requires unique existing zero-point features');
    }
    const checks = new Set(selected.flatMap(feature => feature.criteria.map(check => check.id)));
    if (new Set(entry.expectedFailures).size !== entry.expectedFailures.length
      || entry.expectedFailures.some(id => !checks.has(id))) throw new Error('unknown or duplicate expected failure');
    const source = saved ? { path: saved.source, sha256: saved.sourceSha256 }
      : entry.source ? { ...entry.source, path: resolve(base, entry.source.path) } : undefined;
    if (source && hashAppSource(source.path).sha256 !== source.sha256) throw new Error('candidate source hash mismatch');
    const scenarioSha256 = hash(scenarioText);
    const identity = [entry.backend, entry.track, entry.level, entry.recipe, fixture?.id ?? saved?.runSha256, source?.sha256 ?? inspection?.sourceSha256,
      saved?.reader.sha256 ?? '',
      scenarioSha256, ...entry.expectedFailures.slice().sort()].join(':');
    // Each case owns fresh storage. Repetitions share only their own built app.
    return selected.map(feature => {
      const trials = diagnosticTrials(identity, [feature.id], entry.repetitions);
      for (const trial of trials) {
        if (keys.has(trial.key)) throw new Error('duplicate logical diagnostic trial');
        keys.add(trial.key);
      }
      return { backend: entry.backend, track: entry.track, level: entry.level, recipe: entry.recipe,
        fixture: fixture?.id ?? 'saved-checkpoint', referenceSha256: inspection?.sourceSha256 ?? saved!.sourceSha256, source, saved,
        scenario, scenarioSha256, expectedFailures: entry.expectedFailures.filter(id =>
          feature.criteria.some(check => check.id === id)), trials };
    });
  });
  return { schemaVersion: 1, groups, sha256: hash(JSON.stringify(groups)) };
}

// Validate the actual selection and each expected control, including nonzero exit grades.
export function auditDiagnosticGrade(value: unknown, feature: number, criteria: readonly string[],
  expectedFailures: readonly string[], savedApp = false): void {
  const grade = z.object({ payload: z.object({ features: z.array(z.object({ id: z.number(),
    criteria: z.array(z.object({ id: z.string(), evidence: z.object({ status: z.string() }).passthrough() })) })) }) }).parse(value);
  const features = grade.payload.features;
  const checks = features[0]?.criteria ?? [];
  if (features.length !== 1 || features[0]?.id !== feature || checks.length !== criteria.length
    || new Set(checks.map(check => check.id)).size !== criteria.length
    || checks.some(check => !criteria.includes(check.id))) throw new Error('grade does not match assigned checks');
  for (const check of checks) {
    const status = check.evidence.status;
    // A saved app has no predetermined result. A measured application failure
    // is evidence, while a harness failure still stops further dispatch.
    if (expectedFailures.includes(check.id) ? status !== 'failed'
      : !['passed', 'inconclusive', ...(savedApp ? ['failed'] : [])].includes(status)) {
      throw new Error(`unexpected ${status} for ${check.id}`);
    }
  }
}

export function auditDiagnosticTrials(planned: readonly DiagnosticTrial[], results: readonly TrialResult[]): void {
  const byKey = new Map(planned.map(trial => [trial.key, trial]));
  const executions = new Set<string>();
  for (const trial of results) {
    const assignment = byKey.get(trial.key);
    if (!assignment || assignment.feature !== trial.feature || assignment.repetition !== trial.repetition
      || executions.has(trial.executionId)) throw new Error('unassigned or duplicate diagnostic execution');
    executions.add(trial.executionId);
  }
}

export function diagnosticPopulation(planned: readonly DiagnosticTrial[], results: readonly TrialResult[]) {
  auditDiagnosticTrials(planned, results);
  const started = new Set(results.map(trial => trial.key));
  const collected = new Set(results.filter(trial => trial.state === 'collected').map(trial => trial.key));
  const checkOutcomes: Record<string, number> = {};
  for (const trial of results) for (const [status, count] of Object.entries(trial.checkOutcomes ?? {})) {
    checkOutcomes[status] = (checkOutcomes[status] ?? 0) + count;
  }
  return { planned: planned.length, started: started.size, collected: collected.size,
    executions: results.length, interrupted: results.filter(trial => trial.state === 'interrupted').length,
    unstarted: planned.filter(trial => !started.has(trial.key)).map(trial => trial.key), checkOutcomes };
}

export function assertDiagnosticDependencies(reference: string, candidate: string): void {
  const files = (path: string) => hashAppSource(path).files.filter(file =>
    /^(package\.json|package-lock\.json|npm-shrinkwrap\.json|yarn\.lock|pnpm-lock\.yaml|bun\.lockb?|\.npmrc|reference\.json)$/.test(basename(file)));
  const before = files(reference), after = files(candidate);
  if (JSON.stringify(before) !== JSON.stringify(after)
    || before.some(file => !readFileSync(join(reference, file)).equals(readFileSync(join(candidate, file))))) {
    throw new Error('diagnostic candidates must retain reference dependency and deployment metadata');
  }
}

async function claimGroup(group: DiagnosticGroup, directory: string, id: string,
  signal: AbortSignal, stopPath: string) {
  const track = loadTrack(group.track), scope = resourceLockScope();
  const path = join(directory, 'lease.json');
  let waitReason = '';
  while (true) {
    signal.throwIfAborted();
    if (existsSync(stopPath)) throw new Error('dispatch stopped before admission');
    const serverUri = (index: number) => {
      if (group.saved) return group.saved.serverUri;
      if (18000 + index > 65535) throw new RangeError('database listener exceeds TCP port range');
      return group.backend === 'spacetime' ? `http://127.0.0.1:${18000 + index}` : null;
    };
    const runIndex = group.saved?.runIndex ?? (await selectRunResources({ track, backends: [group.backend], count: 1,
      serverUri, probePort: probeLoopbackPort, signal })).runIndices[0];
    if (runIndex === undefined) throw new Error('no free diagnostic run resources');
    const ports = portsFor(track, group.backend, runIndex);
    if (group.saved && ![ports.vite, ports.express, ...(group.saved.serverUri ? [Number(new URL(group.saved.serverUri).port)] : [])].filter((port): port is number => typeof port === 'number')
      .every(port => probeLoopbackPort(port).free)) {
      await delay(1000, undefined, { signal }); continue;
    }
      const lease = createBackendLease({ runId: id, backend: group.backend, track: group.track, runIndex,
        ...(group.backend === 'spacetime' ? { serverUri: serverUri(runIndex),
          module: group.saved?.module ?? moduleName(track, runIndex), dataDir: join(directory, 'database') }
          : { database: group.saved?.database ?? dbName(track, runIndex) }) });
      const keys = backendResourceLockKeys(lease, ports, [`workspace:${join(directory, 'source')}`]);
      try { claimBackendResources(path, lease, { ...scope, keys }); return { lease, path, ports }; }
      catch (error) {
        releaseResourceLocks(lease);
        lease.state = 'released'; writeBackendLease(path, lease);
        if (message(error).includes('host capacity unavailable:')) {
          if (waitReason !== message(error)) console.error(waitReason = message(error));
        } else if (!existingResourceLockKeys({ ...scope, keys }).length) throw error;
      }
    await delay(1000, undefined, { signal });
  }
}

async function runGroup(plan: FrozenPlan, groupIndex: number, directory: string, stopPath: string): Promise<void> {
  const group = plan.groups[groupIndex];
  if (!group) throw new Error('unknown diagnostic group');
  mkdirSync(directory, { recursive: true });
  const app = join(directory, 'source'), path = join(directory, 'audit.json');
  const audit: GroupAudit = { id: randomUUID(), planSha256: plan.sha256, groupIndex, backend: group.backend,
    sourceSha256: group.source?.sha256 ?? group.referenceSha256, scenarioSha256: group.scenarioSha256,
    controllerImage: process.env.STACK_BENCH_CONTROLLER_IMAGE_ID ?? '',
    plannedTrials: group.trials, trials: [], startedAt: new Date().toISOString(), phases: {} };
  const save = () => writeJson(path, audit);
  const cancellation = new AbortController();
  const cancel = () => cancellation.abort();
  process.once('SIGTERM', cancel); process.once('SIGINT', cancel);
  const started = Date.now();
  save();
  try {
    const { lease, path: leasePath, ports } = await claimGroup(group, directory, audit.id, cancellation.signal, stopPath);
    audit.runIndex = lease.runIndex; audit.phases.admissionMs = Date.now() - started; save();
    process.env.STACK_BENCH_LEASE = leasePath;
    process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
    if (group.backend === 'spacetime') process.env.STACK_BENCH_STDB_URI = lease.resources.serverUri!;
    const phase = async (name: string, operation: () => Promise<void>) => {
      const start = Date.now();
      try { await operation(); } finally { audit.phases[name] = Date.now() - start; save(); }
    };
    const command = (name: string, args: string[]) => runBounded(process.execPath, args, {
      cwd: STACK_BENCH_ROOT, timeoutMs: 20 * 60_000, signal: cancellation.signal,
      logs: { stdout: join(directory, `${name}.stdout.log`), stderr: join(directory, `${name}.stderr.log`) },
    });
    const restartSpec = { backend: group.backend, app, port: ports.vite, probe: '/' };
    await phase('deploymentMs', async () => {
      if (group.saved) {
        const verified = inspectSavedDiagnostic(savedDiagnosticSchema.strip().parse(group.saved), STACK_BENCH_ROOT);
        if (JSON.stringify(verified) !== JSON.stringify(group.saved)
          || verified.buildImage !== process.env.STACK_BENCH_IMAGE) throw new Error('saved diagnostic provenance or runtime changed');
        if (group.backend === 'spacetime' && !process.env.STACK_BENCH_RELEASE_DEPS_VOLUME?.trim()) {
          throw new Error('saved SpacetimeDB requires its original backend image release dependency volume');
        }
        if (group.backend === 'spacetime') {
          const name = `stack-bench-dependencies-${audit.id}`;
          let failure: unknown;
          try {
            // The original backend image owns the manifest; the slim build
            // image does not. App dependencies remain installed by start.sh.
            const verified = await runBounded('docker', ['run', '--rm', '--name', name, '--network', 'none', '--read-only',
              '--mount', `type=volume,source=${process.env.STACK_BENCH_RELEASE_DEPS_VOLUME!.trim()},target=/release-deps,readonly`,
              '--entrypoint', 'node', group.saved.backendImage!, '/opt/stack-bench/dist/appliance/dependency-volume.js',
              'verify', '--target', '/release-deps'], {
              cwd: STACK_BENCH_ROOT, timeoutMs: 60_000, signal: cancellation.signal,
              logs: { stdout: join(directory, 'dependencies.stdout.log'), stderr: join(directory, 'dependencies.stderr.log') },
            });
            if (!verified.ok) failure = new Error(`saved SpacetimeDB dependency provenance failed: ${verified.stderrTail}`);
          } catch (error) { failure = error; }
          finally {
            const removed = await runBounded('docker', ['rm', '-f', name], { cwd: STACK_BENCH_ROOT, timeoutMs: 10_000 });
            if (!removed.ok && !removed.stderrTail.includes('No such container')) {
              failure = new Error(`${failure ? `${message(failure)}; ` : ''}dependency verifier cleanup failed: ${removed.stderrTail}`);
            }
          }
          if (failure) throw failure;
        }
      }
      activateAttemptBackend({ leasePath, lease, ports });
      if (group.saved) {
        snapshotAppSource(group.saved.source, app);
        writeFileSync(join(directory, '.stack-bench-isolation'), 'container');
        writeFileSync(join(directory, '.stack-bench-backend'), group.backend);
        const result = await command('prepare', [compiledEntrypoint('container', 'run-build.js'),
          '--app', app, '--backend', group.backend, '--image', group.saved.buildImage,
          '--ports', [ports.vite, ports.express].filter(Boolean).join(','), '--prepare-only']);
        if (!result.ok) throw new Error(`saved application preparation failed: ${result.stderrTail}`);
        await materializeAcceptedSource(group.saved.source, app, restartSpec);
        if (hashAppSource(app).sha256 !== audit.sourceSha256) throw new Error('saved app changed on startup');
        return;
      }
      const result = await command('deploy', [compiledEntrypoint('src', 'references', 'reference-agent.js'),
        '--backend', group.backend, '--track', group.track, '--level', String(group.level),
        '--recipe', group.recipe, '--run-index', String(lease.runIndex), '--app', app, '--mode', 'build']);
      if (!result.ok) throw new Error(`reference deployment failed: ${result.stderrTail}`);
      if (hashAppSource(app).sha256 !== group.referenceSha256) throw new Error('deployed reference changed');
      if (group.source) {
        if (hashAppSource(group.source.path).sha256 !== group.source.sha256) throw new Error('candidate source changed');
        assertDiagnosticDependencies(app, group.source.path);
        const candidate = join(directory, 'candidate');
        snapshotAppSource(group.source.path, candidate);
        if (hashAppSource(candidate).sha256 !== group.source.sha256) throw new Error('candidate changed while copying');
        await controlAppServer(restartSpec, 'stop');
        restoreAppSource(candidate, app);
        await resetMutationDatabase({ backend: group.backend, app, track: group.track,
          reseedOnReset: true, restartSpec }, null);
      }
      if (hashAppSource(app).sha256 !== audit.sourceSha256) throw new Error('worker source hash mismatch');
    });
    const spec = join(directory, 'scenario.json'); writeJson(spec, group.scenario);
    for (const trial of group.trials) {
      cancellation.signal.throwIfAborted();
      if (existsSync(stopPath)) break;
      const entry: TrialResult = { ...trial, executionId: randomUUID(), state: 'started', startedAt: new Date().toISOString() };
      audit.trials.push(entry); save();
      const resetStarted = Date.now();
      if (audit.trials.length > 1) await resetMutationDatabase({ backend: group.backend, app, track: group.track,
        reseedOnReset: true, restartSpec }, null);
      entry.resetMs = Date.now() - resetStarted; save();
      const gradeStarted = Date.now(), gradePath = join(directory, `${entry.executionId}.json`);
      const result = await command(entry.executionId, [compiledEntrypoint('grader', 'grade.js'),
        '--diagnostic', '--backend', group.backend, '--track', group.track, '--app', app, '--url', `http://127.0.0.1:${ports.vite}`,
        '--level', String(group.scenario.level), '--restart-spec', JSON.stringify(restartSpec),
        '--spec', spec, '--feature', String(trial.feature), '--out', gradePath,
        ...(group.saved ? ['--saved-diagnostic', JSON.stringify(savedDiagnosticSchema.strip().parse(group.saved)),
          '--credential-aliases-json', JSON.stringify(group.saved.credentialAliases)] : [])]);
      entry.gradeMs = Date.now() - gradeStarted;
      if (existsSync(gradePath)) entry.grade = gradePath;
      if (result.timedOut || result.cancelled || result.error || ![0, 1].includes(result.code ?? -1)
        || !entry.grade) throw new Error('grade process did not complete');
      const payload = readGradeArtifactPayload(gradePath);
      entry.checkOutcomes = {};
      for (const feature of payload.features) for (const check of feature.criteria) {
        const status = check.evidence.status;
        entry.checkOutcomes[status] = (entry.checkOutcomes[status] ?? 0) + 1;
      }
      entry.state = 'collected'; save();
      auditDiagnosticGrade({ payload }, trial.feature,
        group.scenario.features.find(feature => feature.id === trial.feature)!.criteria.map(check => check.id),
        group.expectedFailures, Boolean(group.saved));
    }
  } catch (error) {
    audit.error = message(error);
    const last = audit.trials.at(-1);
    if (last?.state === 'started') { last.state = 'interrupted'; last.error = message(error); }
    // Other workers finish their active trial, then retain any unstarted work.
    writeFileSync(stopPath, audit.error); process.exitCode = 1;
  } finally {
    const cleanupStarted = Date.now();
    try {
      if (existsSync(app)) {
        audit.sourceAfter = hashAppSource(app).sha256;
        if (audit.phases.deploymentMs !== undefined && audit.sourceAfter !== audit.sourceSha256) {
          audit.error ??= 'source changed during diagnostic';
        }
      }
      audit.applicationDiagnostics = captureApplicationDiagnostics(join(directory, 'application.log'));
    } catch (error) { audit.error ??= message(error); }
    try { audit.released = rescueGroup(directory); }
    catch (error) { audit.released = false; audit.error ??= message(error); }
    audit.phases.cleanupMs = Date.now() - cleanupStarted;
    audit.finishedAt = new Date().toISOString(); save();
    if (audit.error || !audit.released) { process.exitCode = 1; writeFileSync(stopPath, audit.error ?? 'cleanup failed'); }
    process.removeListener('SIGTERM', cancel); process.removeListener('SIGINT', cancel);
  }
}

function rescueGroup(directory: string): boolean {
  const path = join(directory, 'lease.json');
  if (!existsSync(path)) return true;
  const lease = readBackendLease(path);
  return releaseBackendLease(path, lease.ownershipToken);
}

export async function runReferenceDiagnostics(argv: string[]): Promise<void> {
  const { values } = parseArgs({ args: argv, options: {
    'diagnostic-plan': { type: 'string' }, 'diagnostic-workers': { type: 'string' },
    'diagnostic-group': { type: 'string' }, out: { type: 'string' },
    'diagnostic-resume': { type: 'string' },
  } });
  if (!values['diagnostic-plan'] || !values.out) throw new Error('diagnostics require --diagnostic-plan and --out');
  if (process.platform !== 'linux' || !/^sha256:[a-f0-9]{64}$/.test(process.env.STACK_BENCH_CONTROLLER_IMAGE_ID ?? '')
    || !/^sha256:[a-f0-9]{64}$/.test(process.env.STACK_BENCH_IMAGE ?? '')) {
    throw new Error('reference diagnostics require the Linux appliance and immutable controller/build image IDs');
  }
  const inputPath = resolve(values['diagnostic-plan']), output = resolve(values.out);
  if (values['diagnostic-group'] !== undefined) {
    const plan = read(inputPath) as FrozenPlan;
    if (hash(JSON.stringify(plan.groups)) !== plan.sha256) throw new Error('frozen diagnostic plan changed');
    const index = Number(values['diagnostic-group']);
    if (!Number.isSafeInteger(index) || index < 0) throw new Error('invalid group index');
    await runGroup(plan, index, output, join(dirname(inputPath), 'stop'));
    return;
  }
  const plan = freezeDiagnosticPlan(read(inputPath), dirname(inputPath));
  const workers = values['diagnostic-workers'] === undefined ? plan.groups.length : Number(values['diagnostic-workers']);
  if (!Number.isSafeInteger(workers) || workers < 1) throw new Error('diagnostic workers must be a positive integer');
  if (existsSync(output) || existsSync(`${output}.runs`)) throw new Error('diagnostic output already exists');
  const root = `${output}.runs`;
  mkdirSync(dirname(root), { recursive: true });
  mkdirSync(root);
  const frozenPath = join(root, 'plan.json'); writeJson(frozenPath, plan);
  const cancellation = new AbortController(), cancel = () => cancellation.abort();
  process.once('SIGINT', cancel); process.once('SIGTERM', cancel);
  const report = { schemaVersion: 1, diagnostic: true, paidCalls: 0, id: randomUUID(),
    planSha256: plan.sha256, controllerImage: process.env.STACK_BENCH_CONTROLLER_IMAGE_ID ?? '',
    buildImage: process.env.STACK_BENCH_IMAGE!, resumedFrom: values['diagnostic-resume'] ? resolve(values['diagnostic-resume']) : null,
    runner: controllerRunner(), workers: Math.min(workers, plan.groups.length), startedAt: new Date().toISOString(),
    finishedAt: '', plannedTrials: plan.groups.flatMap(group => group.trials),
    population: diagnosticPopulation(plan.groups.flatMap(group => group.trials), []),
    groups: plan.groups.map((group, index) => ({ backend: group.backend, groupIndex: index,
      directory: join(root, `group-${index}`), state: 'unstarted', error: '', audit: null as GroupAudit | null })) };
  if (report.resumedFrom) {
    const prior = read(report.resumedFrom) as typeof report;
    if (!prior.finishedAt || prior.planSha256 !== report.planSha256 || prior.controllerImage !== report.controllerImage
      || prior.buildImage !== report.buildImage || prior.groups.length !== report.groups.length) {
      throw new Error('resume requires a finished controller with the same frozen plan and images');
    }
    // Resume whole groups with no dispatch only. Interrupted trials need an explicit new study;
    // their original execution and grades are never overwritten or chosen by score.
    for (const [index, entry] of prior.groups.entries()) {
      if (entry.groupIndex !== index) throw new Error('resume group identity mismatch');
      if (entry.state === 'unstarted') {
        if (entry.audit || existsSync(entry.directory)) throw new Error('group has evidence of prior dispatch');
      } else {
        const leasePath = join(entry.directory, 'lease.json');
        if (existsSync(leasePath) && readBackendLease(leasePath).state !== 'released') {
          throw new Error('resume requires released prior workers');
        }
        if (!entry.audit && entry.state === 'collected') throw new Error('collected group has no audit');
        if (entry.audit) validateDiagnosticGroup(plan, index, entry.audit, report.controllerImage, entry.directory, entry.state === 'collected');
        report.groups[index] = entry;
      }
    }
  }
  const save = () => {
    report.population = diagnosticPopulation(report.plannedTrials, report.groups.flatMap(group => group.audit?.trials ?? []));
    writeJson(output, report);
  };
  let cursor = 0;
  save();
  try {
    await Promise.all(Array.from({ length: report.workers }, async () => {
      while (!cancellation.signal.aborted && !existsSync(join(root, 'stop'))) {
        const entry = report.groups[cursor++];
        if (!entry) return;
        if (entry.state !== 'unstarted') continue;
        entry.state = 'started'; mkdirSync(entry.directory); save();
        console.log(`Diagnostic group ${entry.groupIndex + 1}/${report.groups.length}: dispatched (${entry.backend})`);
        try {
          const result = await runBounded(process.execPath, [compiledEntrypoint('src', 'references', 'reference-live.js'),
            '--diagnostic-plan', frozenPath, '--diagnostic-group', String(entry.groupIndex), '--out', entry.directory], {
            cwd: STACK_BENCH_ROOT, timeoutMs: null, signal: cancellation.signal, gracefulCancellationMs: 10_000,
            logs: { stdout: join(entry.directory, 'worker.stdout.log'), stderr: join(entry.directory, 'worker.stderr.log') },
          });
          const auditPath = join(entry.directory, 'audit.json');
          if (existsSync(auditPath)) entry.audit = read(auditPath) as GroupAudit;
          if (entry.audit) {
            validateDiagnosticGroup(plan, entry.groupIndex, entry.audit, report.controllerImage, entry.directory, false);
          }
          if (!result.ok || !entry.audit?.finishedAt || entry.audit.error) throw new Error(entry.audit?.error ?? 'worker interrupted');
          const population = diagnosticPopulation(plan.groups[entry.groupIndex]!.trials, entry.audit.trials);
          entry.state = population.collected === population.planned && population.executions === population.planned
            ? 'collected' : 'incomplete';
          if (entry.state === 'collected') validateDiagnosticGroup(plan, entry.groupIndex, entry.audit,
            report.controllerImage, entry.directory, true);
        } catch (error) {
          entry.state = 'interrupted'; entry.error = message(error); writeFileSync(join(root, 'stop'), entry.error);
          for (const trial of entry.audit?.trials ?? []) {
            if (trial.state === 'started') { trial.state = 'interrupted'; trial.error = entry.error; }
          }
        } finally {
          try { if (!rescueGroup(entry.directory)) entry.error = 'worker cleanup incomplete'; }
          catch (error) { entry.error = message(error); }
          if (entry.error) { entry.state = 'interrupted'; writeFileSync(join(root, 'stop'), entry.error); }
          save();
          console.log(`Diagnostic group ${entry.groupIndex + 1}: ${entry.state}${entry.error ? `: ${entry.error}` : ''}`);
        }
      }
    }));
  } finally {
    report.finishedAt = new Date().toISOString(); save();
    process.removeListener('SIGINT', cancel); process.removeListener('SIGTERM', cancel);
  }
  if (report.groups.some(group => group.state !== 'collected')) process.exitCode = 1;
  console.log(JSON.stringify({ output, groups: report.groups.map(group => ({ backend: group.backend, state: group.state })) }));
}
