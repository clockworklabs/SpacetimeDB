import { resolve, sep } from 'node:path';
import { criterionEvidence, evidenceDisposition } from './check-evidence.js';
import type { CheckEvidenceStatus, CheckOutcomeKind } from './check-evidence.js';
import type { RecipeRelease } from '../composition/recipe-release.mjs';

type UnknownRecord = Record<string, unknown>;

export interface MutationEditDefinition {
  file?: unknown;
  find?: unknown;
  replace?: unknown;
}

export interface MutationDefinition {
  id?: unknown;
  file?: unknown;
  find?: unknown;
  replace?: unknown;
  edits?: unknown;
  targets?: unknown;
  scenario?: unknown;
  breaks?: unknown;
  kills?: unknown;
  [key: string]: unknown;
}

export interface MutationFileEdit {
  file: string;
  find: string;
  replace: string;
}

export interface MutationValidationIssue {
  kind: string;
  mutation?: unknown;
}

export interface MutationManifest {
  scenario?: unknown;
  mutations?: MutationDefinition[];
}

interface CriterionReport {
  id?: string;
  stableKey?: unknown;
  evidence?: unknown;
}

interface FeatureReport {
  id?: unknown;
  criteria?: CriterionReport[];
}

interface SelectionCheck {
  stableKey?: unknown;
}

interface MutationReport {
  total?: unknown;
  max?: unknown;
  features?: FeatureReport[];
  selection?: { checks?: SelectionCheck[] };
}

interface IndexedCriterion {
  feature: unknown;
  criterion: string | number | undefined;
  passed: boolean;
  measured: boolean;
  applicationFailure: boolean;
  outcomeKind: CheckOutcomeKind;
  status: CheckEvidenceStatus;
  phase: 'setup' | 'assertion';
  detail: string | null;
}

interface SetupFailure {
  feature: unknown;
  detail: string | null;
  status: CheckEvidenceStatus;
  outcomeKind: CheckOutcomeKind;
  code: string;
}

interface IndexedMutationReport {
  criteria: Map<string, IndexedCriterion>;
  setupFailures: SetupFailure[];
}

interface ArtifactIdentityLike {
  id?: unknown;
  version?: unknown;
  sha256?: unknown;
  state?: unknown;
}

interface MutationSuite extends MutationReport {
  [key: string]: unknown;
}

interface MutationBundle {
  backend?: unknown;
  track?: unknown;
  level?: unknown;
  source?: { sha256?: unknown };
  recipeRelease?: { id?: unknown; version?: unknown; contentSha256?: unknown };
  artifactEnvelope?: { identities?: Record<string, ArtifactIdentityLike | undefined> };
  suites?: Record<string, MutationSuite>;
}

interface ExpectedBaseline {
  backend: unknown;
  track: unknown;
  level: unknown;
  fixtureSha256: unknown;
  recipe: { id: unknown; version: unknown; sha256: unknown };
  identities?: Record<string, ArtifactIdentityLike | undefined>;
  selectedCheckKeys: Iterable<string>;
}

export interface MutationAnalysisIssue extends UnknownRecord {
  kind: string;
}

export type MutationClassificationStatus =
  | 'CAUGHT'
  | 'INVALID_REPORT'
  | 'INVALID_SETUP'
  | 'INVALID_HARNESS_FAILURE'
  | 'INVALID_INCONCLUSIVE'
  | 'WRONG_CRITERION'
  | 'SURVIVED'
  | 'CAUGHT_COLLATERAL';

function featureKey(id: unknown): string {
  return String(id);
}

function criterionKey(feature: unknown, criterion: unknown): string {
  return `${featureKey(feature)}:${String(criterion)}`;
}

export function mutationEdits(mutation: MutationDefinition): MutationEditDefinition[] {
  const edits = mutation.edits ?? (mutation.find != null || mutation.replace != null
    ? [{ find: mutation.find, replace: mutation.replace }] : []);
  return edits as MutationEditDefinition[];
}

export function mutationFileEdits(mutation: MutationDefinition): MutationFileEdit[] {
  return mutationEdits(mutation).map(edit => ({
    file: edit.file ?? mutation.file,
    find: edit.find,
    replace: edit.replace,
  })) as MutationFileEdit[];
}

export function mutationTargetKeys(mutation: MutationDefinition): string[] {
  return Array.isArray(mutation.targets) ? mutation.targets as string[] : [];
}

export function mutationScenario(
  manifest: Pick<MutationManifest, 'scenario'> | null | undefined,
  mutation: MutationDefinition | null | undefined,
): string | null {
  const scenario = mutation?.scenario ?? manifest?.scenario;
  return typeof scenario === 'string' && scenario.trim() ? scenario : null;
}

export function groupMutationsByScenario(manifest: MutationManifest): Map<string, MutationDefinition[]> {
  const groups = new Map<string, MutationDefinition[]>();
  for (const mutation of manifest?.mutations ?? []) {
    const scenario = mutationScenario(manifest, mutation);
    if (!scenario) throw new Error(`mutation ${mutation?.id ?? '<unnamed>'} has no scenario`);
    const group = groups.get(scenario) ?? [];
    group.push(mutation);
    groups.set(scenario, group);
  }
  return groups;
}

export function releaseScenarioCheckKeys(
  release: RecipeRelease | null | undefined,
  trackDir: string,
  scenarioPath: string,
  selectedCheckKeys: Iterable<string> | null = null,
): string[] {
  if (!release || !Array.isArray(release.checkCatalog)) {
    throw new Error('recipe-bound mutation grading requires a compiled release check catalog');
  }
  const selectedScenario = resolve(scenarioPath);
  const selected = selectedCheckKeys === null ? null : new Set<string>(selectedCheckKeys);
  const keys = release.checkCatalog
    .filter(check => Number(check.points) > 0
      && (selected === null || selected.has(check.stableKey))
      && resolve(trackDir, check.source as string) === selectedScenario)
    .map(check => check.stableKey);
  if (keys.length === 0) {
    throw new Error(`mutation scenario ${scenarioPath} has no checks in the exact recipe release`);
  }
  return keys;
}

export function resolveMutationFile(app: string, file: string): string {
  const root = resolve(app);
  const target = resolve(root, file);
  if (target === root || !target.startsWith(`${root}${sep}`)) {
    throw new Error(`mutation file escapes the app directory: ${file}`);
  }
  return target;
}

export function validateMutationDefinitions(
  mutations: MutationDefinition[] | undefined,
  { defaultScenario = null, requireScenario = false }:
    { defaultScenario?: string | null; requireScenario?: boolean } = {},
): { ok: boolean; issues: MutationValidationIssue[] } {
  const issues: MutationValidationIssue[] = [];
  const ids = new Set<string>();
  for (const mutation of mutations ?? []) {
    if (typeof mutation.id !== 'string' || !mutation.id) issues.push({ kind: 'bad_id', mutation: mutation.id });
    else if (ids.has(mutation.id)) issues.push({ kind: 'duplicate_id', mutation: mutation.id });
    else ids.add(mutation.id);
    const edits = mutationEdits(mutation);
    const hasDefaultFile = typeof mutation.file === 'string' && Boolean(mutation.file);
    if (!hasDefaultFile && !edits.every(edit => typeof edit?.file === 'string' && edit.file)) {
      issues.push({ kind: 'bad_file', mutation: mutation.id });
    }
    if (mutation.file !== undefined && !hasDefaultFile) {
      issues.push({ kind: 'bad_file', mutation: mutation.id });
    }
    if (mutation.scenario !== undefined
      && (typeof mutation.scenario !== 'string' || !mutation.scenario.trim())) {
      issues.push({ kind: 'bad_scenario', mutation: mutation.id });
    }
    if (requireScenario && !mutationScenario({ scenario: defaultScenario }, mutation)) {
      issues.push({ kind: 'missing_scenario', mutation: mutation.id });
    }
    if (!Array.isArray(mutation.targets) || mutation.targets.length === 0
      || mutation.targets.some(target => typeof target !== 'string' || !target.trim())
      || new Set(mutation.targets).size !== mutation.targets.length) {
      issues.push({ kind: 'bad_targets', mutation: mutation.id });
    }
    if (mutation.breaks != null || mutation.kills != null) {
      issues.push({ kind: 'legacy_targets', mutation: mutation.id });
    }
    if (edits.length === 0) issues.push({ kind: 'missing_edits', mutation: mutation.id });
    for (const edit of edits) {
      if (edit.file !== undefined && (typeof edit.file !== 'string' || !edit.file)) {
        issues.push({ kind: 'bad_file', mutation: mutation.id });
      }
      if (typeof edit.find !== 'string' || !edit.find || typeof edit.replace !== 'string' ||
        edit.find === edit.replace) issues.push({ kind: 'bad_edit', mutation: mutation.id });
    }
  }
  return { ok: issues.length === 0, issues };
}

export function indexMutationReport(report: MutationReport | null | undefined): IndexedMutationReport {
  const criteria = new Map<string, IndexedCriterion>();
  const setupFailures = new Map<string, SetupFailure>();
  for (const feature of report?.features ?? []) {
    for (const criterion of feature.criteria ?? []) {
      const evidence = criterionEvidence(criterion);
      const disposition = evidenceDisposition(evidence);
      if (evidence.phase === 'setup' && !disposition.passed) {
        setupFailures.set(featureKey(feature.id), { feature: feature.id, detail: evidence.summary,
          status: evidence.status, outcomeKind: disposition.outcomeKind, code: evidence.code });
      }
      const key = typeof criterion.stableKey === 'string' && criterion.stableKey
        ? criterion.stableKey : criterionKey(feature.id, criterion.id);
      criteria.set(key, {
        feature: feature.id,
        criterion: criterion.id,
        passed: disposition.passed,
        measured: disposition.measured,
        applicationFailure: disposition.applicationFailure,
        outcomeKind: disposition.outcomeKind,
        status: evidence.status,
        phase: evidence.phase,
        detail: evidence.summary,
      });
    }
  }
  return { criteria, setupFailures: [...setupFailures.values()] };
}

function stableKeys(report: MutationReport | null | undefined): unknown[] {
  return (report?.selection?.checks ?? []).map(check => check.stableKey).sort();
}

function identityFields(identity: ArtifactIdentityLike | null | undefined): UnknownRecord {
  return {
    id: identity?.id ?? null,
    version: identity?.version ?? null,
    sha256: identity?.sha256 ?? null,
    state: identity?.state ?? null,
  };
}

export function reusableMutationBaseline(
  bundle: MutationBundle | null | undefined,
  expected: ExpectedBaseline,
): { ok: false; reason: string } | { ok: true; report: MutationSuite } {
  const release = bundle?.recipeRelease;
  const identities = bundle?.artifactEnvelope?.identities ?? {};
  const mismatches: string[] = [];
  if (bundle?.backend !== expected.backend) mismatches.push('backend');
  if (bundle?.track !== expected.track) mismatches.push('track');
  if (Number(bundle?.level) !== Number(expected.level)) mismatches.push('level');
  if (bundle?.source?.sha256 !== expected.fixtureSha256) mismatches.push('fixture');
  if (release?.id !== expected.recipe.id || release?.version !== expected.recipe.version
      || release?.contentSha256 !== expected.recipe.sha256) mismatches.push('recipe');
  for (const key of ['engine', 'calibration', 'stackAdapter']) {
    if (JSON.stringify(identityFields(identities[key]))
        !== JSON.stringify(identityFields(expected.identities?.[key]))) mismatches.push(key);
  }
  if (mismatches.length) {
    return { ok: false, reason: `clean baseline identity differs: ${mismatches.join(', ')}` };
  }

  const requested = [...expected.selectedCheckKeys].sort();
  const candidates = Object.values(bundle?.suites ?? {}).filter(suite =>
    JSON.stringify(stableKeys(suite)) === JSON.stringify(requested));
  if (candidates.length !== 1) {
    return { ok: false, reason: `clean baseline has ${candidates.length} exact scenario matches` };
  }
  return { ok: true, report: candidates[0]! };
}

export function validateMutationBaseline(
  report: MutationReport | null | undefined,
  mutations: MutationDefinition[],
): { ok: boolean; issues: MutationAnalysisIssue[] } {
  const indexed = indexMutationReport(report);
  const issues: MutationAnalysisIssue[] = [];
  if (indexed.criteria.size === 0) issues.push({ kind: 'empty_report' });
  if (Number(report?.total) !== Number(report?.max)) {
    issues.push({ kind: 'score_not_full', total: report?.total, max: report?.max });
  }
  for (const failure of indexed.setupFailures) issues.push({ kind: 'setup_failure', ...failure });
  for (const item of indexed.criteria.values()) {
    if (!item.passed) issues.push({ kind: item.outcomeKind === 'harness_failure' ? 'harness_failure'
      : item.outcomeKind === 'inconclusive' ? 'inconclusive' : 'criterion_failure', ...item });
  }
  for (const mutation of mutations) {
    for (const key of mutationTargetKeys(mutation)) {
      if (!indexed.criteria.has(key)) {
        const [feature, ...criterion] = key.split(':');
        issues.push({ kind: 'missing_target', mutation: mutation.id,
          feature, criterion: criterion.join(':') });
      }
    }
  }
  return { ok: issues.length === 0, issues };
}

export function isRetryableMutationBaseline(
  issues: ReadonlyArray<{ kind: string }> | null | undefined,
): boolean {
  const kinds = new Set((issues ?? []).map(issue => issue.kind));
  if (!kinds.size) return false;
  if (kinds.has('empty_report') || kinds.has('missing_target') || kinds.has('criterion_failure')) {
    return false;
  }
  return kinds.has('setup_failure') || kinds.has('harness_failure') || kinds.has('inconclusive');
}

export function classifyMutationResult(
  baselineReport: MutationReport,
  mutantReport: MutationReport,
  mutation: MutationDefinition,
): {
  status: MutationClassificationStatus;
  targetKeys: string[];
  targetMissing: string[];
  targetHarnessFailures: string[];
  targetInconclusive: string[];
  targetSurvived: string[];
  collateral: Array<IndexedCriterion & { key: string; expected: boolean }>;
  collateralHarnessFailures: string[];
  collateralInconclusive: string[];
  setupFailures: SetupFailure[];
  missing: string[];
  regressions: Array<IndexedCriterion & { key: string; expected: boolean }>;
} {
  const baseline = indexMutationReport(baselineReport);
  const mutant = indexMutationReport(mutantReport);
  const targetKeys = new Set(mutationTargetKeys(mutation));
  const missing = [...baseline.criteria.keys()].filter(key => !mutant.criteria.has(key));
  const setupFailures = mutant.setupFailures.filter(item =>
    !baseline.setupFailures.some(base => featureKey(base.feature) === featureKey(item.feature)));
  const regressions: Array<IndexedCriterion & { key: string; expected: boolean }> = [];
  for (const [key, before] of baseline.criteria) {
    const after = mutant.criteria.get(key);
    if (!before.passed || !after?.applicationFailure) continue;
    regressions.push({ ...after, key, expected: targetKeys.has(key) });
  }
  const targets = [...targetKeys].map(key => ({ key, result: mutant.criteria.get(key) }));
  const targetMissing = targets.filter(item => !item.result).map(item => item.key);
  const targetHarnessFailures = targets.filter(item => item.result?.outcomeKind === 'harness_failure')
    .map(item => item.key);
  const targetInconclusive = targets.filter(item => item.result?.outcomeKind === 'inconclusive')
    .map(item => item.key);
  const targetSurvived = targets.filter(item => item.result?.passed).map(item => item.key);
  const collateral = regressions.filter(item => !item.expected);
  const collateralHarnessFailures = [...baseline.criteria].filter(([key, before]) =>
    before.passed && !targetKeys.has(key)
      && mutant.criteria.get(key)?.outcomeKind === 'harness_failure').map(([key]) => key);
  const collateralInconclusive = [...baseline.criteria].filter(([key, before]) =>
    before.passed && !targetKeys.has(key)
      && mutant.criteria.get(key)?.outcomeKind === 'inconclusive').map(([key]) => key);

  let status: MutationClassificationStatus = 'CAUGHT';
  if (targetKeys.size === 0 || missing.length || targetMissing.length) status = 'INVALID_REPORT';
  else if (setupFailures.length) status = 'INVALID_SETUP';
  else if (targetHarnessFailures.length || collateralHarnessFailures.length) {
    status = 'INVALID_HARNESS_FAILURE';
  }
  else if (targetInconclusive.length || collateralInconclusive.length) status = 'INVALID_INCONCLUSIVE';
  else if (targetSurvived.length && collateral.length) status = 'WRONG_CRITERION';
  else if (targetSurvived.length) status = 'SURVIVED';
  else if (collateral.length) status = 'CAUGHT_COLLATERAL';

  return { status, targetKeys: [...targetKeys], targetMissing, targetHarnessFailures, targetInconclusive,
    targetSurvived, collateral, collateralHarnessFailures, collateralInconclusive,
    setupFailures, missing, regressions };
}
