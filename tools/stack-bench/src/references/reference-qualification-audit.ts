import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { criterionEvidence, evidencePassed } from '../evidence/check-evidence.js';
import { readArtifactPayload } from '../evidence/artifacts.js';

import type { ReferenceFixture } from './reference-fixtures.js';

type UnknownRecord = Record<string, unknown>;

interface RunLevelResult {
  level: number;
  score?: { earned?: number; possible?: number };
  criteria?: UnknownRecord[];
  [key: string]: unknown;
}

interface LeasePayload {
  runId?: string;
  state?: string;
  resources?: {
    buildContainer?: { running?: boolean; [key: string]: unknown } | null;
    locks?: Array<{ releasedAt?: string | null; [key: string]: unknown }>;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

interface RunPayload {
  setup?: { isolation?: { mode?: string; image?: string; imageId?: string } };
  outcome?: { kind?: string };
  levels?: RunLevelResult[];
  mutationControl?: { ok?: boolean; [key: string]: unknown };
  backendLease?: LeasePayload | null;
  [key: string]: unknown;
}

interface BundlePayload {
  selection?: {
    recipe?: UnknownRecord;
    checks?: UnknownRecord[];
    reportedChecks?: string[];
    [key: string]: unknown;
  };
  recipeRelease?: { contentSha256?: string; [key: string]: unknown };
  [key: string]: unknown;
}

interface BundleSuite {
  features?: Array<{ id?: string; criteria?: UnknownRecord[]; [key: string]: unknown }>;
  [key: string]: unknown;
}

interface MutationControlPayload {
  ok?: boolean;
  fixtureSha256?: string;
  mutations?: UnknownRecord[];
  [key: string]: unknown;
}

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object';

export function auditReferenceRun(output: string, fixture: ReferenceFixture,
  { requireMutationControl = false, release = null, level = fixture.level,
    selectedCheckKeys = null }: {
      requireMutationControl?: boolean; release?: UnknownRecord | null;
      level?: number; selectedCheckKeys?: string[] | null;
    } = {}): { ok: boolean; failures: string[]; [key: string]: unknown } {
  const runPath = join(output, 'run.json');
  const bundlePath = join(output, 'grading', 'bundle.json');
  if (!existsSync(runPath) || !existsSync(bundlePath)) {
    return { ok: false, failures: ['run.json or grading/bundle.json is missing'] };
  }
  const run = readArtifactPayload<RunPayload>(runPath, { expectedKind: 'benchmark_run' });
  const bundle = readArtifactPayload<BundlePayload>(bundlePath, { expectedKind: 'grade_bundle' });
  const failures = [];
  if (!release?.contentSha256 || !Array.isArray(release.checkCatalog)
      || release.checkCatalog.length === 0) {
    failures.push('exact recipe release was not supplied to the qualification audit');
  }
  if (run.backend !== fixture.backend || run.track !== fixture.track) failures.push('run identity does not match fixture');
  if (run.setup?.isolation?.mode !== 'container') failures.push('run was not isolated in Docker');
  if (run.outcome?.kind !== 'passed') failures.push(`run outcome is ${run.outcome?.kind ?? 'missing'}`);
  const levelResult = run.levels?.find(candidate => candidate.level === level);
  if (!levelResult) failures.push(`L${level} result is missing`);
  else {
    if (!levelResult.graded) failures.push(`L${level} was not graded`);
    if (!levelResult.contractPass) failures.push(`L${level} contract lint failed`);
    if (levelResult.score !== levelResult.max) {
      failures.push(`L${level} score is ${levelResult.score}/${levelResult.max}`);
    }
  }

  const criteria = [];
  const suites: Record<string, BundleSuite> = record(bundle.suites)
    ? bundle.suites as Record<string, BundleSuite> : {};
  for (const [suiteId, suite] of Object.entries(suites)) {
    if (suiteId === 'lint') continue;
    for (const feature of suite.features ?? []) {
      const setupFailure = (feature.criteria ?? []).map(criterionEvidence)
        .find(evidence => evidence.phase === 'setup' && !evidencePassed(evidence));
      if (setupFailure) failures.push(`${suiteId}/${feature.id} setup failed: ${setupFailure.summary}`);
      for (const criterion of feature.criteria ?? []) {
        const key = `${suiteId}/${feature.id}/${criterion.id}`;
        const evidence = criterionEvidence(criterion);
        criteria.push({ key, stableKey: criterion.stableKey ?? null,
          points: criterion.points ?? 0, passed: evidencePassed(evidence), status: evidence.status });
        if (!evidencePassed(evidence)) failures.push(`${key} did not pass`);
      }
    }
  }
  if (!criteria.length) failures.push('no scenario criteria were recorded');
  if (release) {
    const expectedRecipe = { id: release.id, version: release.version,
      contentSha256: release.contentSha256 };
    const actualRecipe = bundle.selection?.recipe;
    if (actualRecipe?.id !== expectedRecipe.id || actualRecipe?.version !== expectedRecipe.version
        || actualRecipe?.contentSha256 !== expectedRecipe.contentSha256) {
      failures.push('grading bundle recipe identity does not match the requested release');
    }
    if (bundle.recipeRelease?.contentSha256 !== release.contentSha256) {
      failures.push('grading bundle release document does not match the requested release');
    }
    const fields = ['stableKey', 'points', 'source', 'executionId', 'featureId',
      'criterionId', 'packId', 'checkGroupId'];
    const shape = (check: unknown): { stableKey: string; [field: string]: unknown } => {
      const value = record(check) ? check : {};
      const shaped = Object.fromEntries(fields.map(field => [field, value[field] ?? null]));
      return { ...shaped, stableKey: String(value.stableKey ?? '') };
    };
    const order = (checks: readonly unknown[]): Array<{ stableKey: string }> =>
      checks.map(shape).sort((a, b) => a.stableKey.localeCompare(b.stableKey));
    const catalog: Array<{ stableKey: string }> = Array.isArray(release.checkCatalog)
      ? release.checkCatalog : [];
    const selected = selectedCheckKeys === null ? catalog : (() => {
      const requested = new Set(selectedCheckKeys);
      const checks = catalog.filter(check => requested.delete(check.stableKey));
      if (requested.size) failures.push(`selected checks are absent from the recipe: ${
        [...requested].sort().join(', ')}`);
      return checks;
    })();
    const expectedChecks = order(selected);
    const selectedChecks = Array.isArray(bundle.selection?.checks)
      ? order(bundle.selection.checks) : null;
    if (!selectedChecks || JSON.stringify(selectedChecks) !== JSON.stringify(expectedChecks)) {
      failures.push('graded check catalog does not match the requested release');
    }
    const expectedKeys = expectedChecks.map(check => check.stableKey);
    const reportedKeys = [...(bundle.selection?.reportedChecks ?? [])].sort();
    const evidenceKeys = criteria.map(check => check.stableKey).sort();
    if (JSON.stringify(reportedKeys) !== JSON.stringify(expectedKeys)
        || JSON.stringify(evidenceKeys) !== JSON.stringify(expectedKeys)) {
      failures.push('graded check evidence does not cover the exact requested release');
    }
  }
  const lease = run.backendLease;
  if (lease?.state !== 'released') failures.push('backend lease was not released');
  if (lease?.resources?.buildContainer?.running !== false) failures.push('leased build container was not recorded as removed');
  if (!lease?.resources?.locks?.length) failures.push('run recorded no resource lock');
  for (const lock of lease?.resources?.locks ?? []) {
    if (!lock.releasedAt) failures.push(`resource lock ${lock.key ?? lock.path} was not released`);
  }
  let mutationControl = null;
  if (requireMutationControl) {
    const mutationPath = join(output, 'mutation-control.json');
    if (!existsSync(mutationPath)) failures.push('mutation-control.json is missing');
    else {
      mutationControl = readArtifactPayload(mutationPath, { expectedKind: 'mutation_control' });
      if (run.mutationControl?.ok !== true || mutationControl.ok !== true) {
        failures.push('mutation control did not pass');
      }
      if (mutationControl.fixtureSha256 !== fixture.imported?.sourceSha256) {
        failures.push('mutation control targets a different fixture hash');
      }
      if (!Array.isArray(mutationControl.results) || mutationControl.results.length === 0) {
        failures.push('mutation control recorded no mutants');
      } else {
        for (const mutant of mutationControl.results) {
          if (mutant.status !== 'CAUGHT') failures.push(`${mutant.id ?? '<unnamed mutant>'} is ${mutant.status ?? 'missing a status'}`);
        }
      }
      if (Number(mutationControl.baseline?.total) !== Number(mutationControl.baseline?.max)) {
        failures.push('mutation baseline was not fully passing');
      }
    }
  }
  const fingerprint = createHash('sha256').update(JSON.stringify(criteria)).digest('hex');
  return { ok: failures.length === 0, failures, runId: run.id,
    score: levelResult ? `${levelResult.score}/${levelResult.max}` : null,
    imageId: run.setup?.isolation?.imageId ?? null, criteria: criteria.length,
    zeroPointCriteria: criteria.filter(criterion => criterion.points === 0).length, fingerprint,
    outcome: run.outcome?.kind ?? null, packRuntime: bundle.packRuntime ?? null,
    mutations: mutationControl?.summary ?? null };
}

export function auditMutationWorkerRun(output: string, fixture: ReferenceFixture):
  { ok: boolean; failures: string[]; [key: string]: unknown } {
  const runPath = join(output, 'run.json');
  const controlPath = join(output, 'mutation-control.json');
  if (!existsSync(runPath) || !existsSync(controlPath)) {
    return { ok: false, failures: ['run.json or mutation-control.json is missing'] };
  }
  const run = readArtifactPayload<RunPayload>(runPath, { expectedKind: 'benchmark_run' });
  const control = readArtifactPayload<MutationControlPayload>(controlPath,
    { expectedKind: 'mutation_control' });
  const failures = [];
  if (run.backend !== fixture.backend || run.track !== fixture.track) {
    failures.push('run identity does not match fixture');
  }
  if (run.setup?.isolation?.mode !== 'container') failures.push('run was not isolated in Docker');
  if (run.outcome?.kind !== 'passed') failures.push(`run outcome is ${run.outcome?.kind ?? 'missing'}`);
  if (run.mutationControl?.ok !== true || control.ok !== true) {
    failures.push('mutation control did not pass');
  }
  if (control.fixtureSha256 !== fixture.imported?.sourceSha256) {
    failures.push('mutation control targets a different fixture hash');
  }
  if (!Array.isArray(control.results) || control.results.length === 0) {
    failures.push('mutation control recorded no mutants');
  } else {
    for (const mutant of control.results) {
      if (mutant.status !== 'CAUGHT') {
        failures.push(`${mutant.id ?? '<unnamed mutant>'} is ${mutant.status ?? 'missing a status'}`);
      }
    }
  }
  const baseline = record(control.baseline) ? control.baseline : {};
  if (Number(baseline.total) !== Number(baseline.max)) {
    failures.push('mutation baseline was not fully passing');
  }
  const lease = run.backendLease;
  if (lease?.state !== 'released') failures.push('backend lease was not released');
  if (lease?.resources?.buildContainer?.running !== false) {
    failures.push('leased build container was not recorded as removed');
  }
  if (!lease?.resources?.locks?.length
      || lease.resources.locks.some(lock => !lock.releasedAt)) {
    failures.push('resource lock release evidence is incomplete');
  }
  return { ok: failures.length === 0, failures, runId: run.id,
    imageId: run.setup?.isolation?.imageId ?? null, outcome: run.outcome?.kind ?? null,
    mutations: control.summary ?? null, score: null, criteria: null,
    zeroPointCriteria: null, fingerprint: null, packRuntime: null };
}
