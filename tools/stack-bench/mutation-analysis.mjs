import { resolve, sep } from 'node:path';
import { criterionEvidence, evidenceDisposition } from './check-evidence.mjs';

function featureKey(id) {
  return String(id);
}

function criterionKey(feature, criterion) {
  return `${featureKey(feature)}:${String(criterion)}`;
}

export function mutationEdits(mutation) {
  return mutation.edits ?? (mutation.find != null || mutation.replace != null
    ? [{ find: mutation.find, replace: mutation.replace }] : []);
}

export function mutationTargetKeys(mutation) {
  if (Array.isArray(mutation.targets)) {
    return mutation.targets.map(target => criterionKey(target.feature, target.criterion));
  }
  return (mutation.kills ?? []).map(criterion => criterionKey(mutation.breaks, criterion));
}

export function mutationScenario(manifest, mutation) {
  const scenario = mutation?.scenario ?? manifest?.scenario;
  return typeof scenario === 'string' && scenario.trim() ? scenario : null;
}

export function groupMutationsByScenario(manifest) {
  const groups = new Map();
  for (const mutation of manifest?.mutations ?? []) {
    const scenario = mutationScenario(manifest, mutation);
    if (!scenario) throw new Error(`mutation ${mutation?.id ?? '<unnamed>'} has no scenario`);
    if (!groups.has(scenario)) groups.set(scenario, []);
    groups.get(scenario).push(mutation);
  }
  return groups;
}

export function resolveMutationFile(app, file) {
  const root = resolve(app);
  const target = resolve(root, file);
  if (target === root || !target.startsWith(`${root}${sep}`)) {
    throw new Error(`mutation file escapes the app directory: ${file}`);
  }
  return target;
}

export function validateMutationDefinitions(mutations,
  { defaultScenario = null, requireScenario = false } = {}) {
  const issues = [];
  const ids = new Set();
  for (const mutation of mutations ?? []) {
    if (typeof mutation.id !== 'string' || !mutation.id) issues.push({ kind: 'bad_id', mutation: mutation.id });
    else if (ids.has(mutation.id)) issues.push({ kind: 'duplicate_id', mutation: mutation.id });
    else ids.add(mutation.id);
    if (typeof mutation.file !== 'string' || !mutation.file) issues.push({ kind: 'bad_file', mutation: mutation.id });
    if (mutation.scenario !== undefined
      && (typeof mutation.scenario !== 'string' || !mutation.scenario.trim())) {
      issues.push({ kind: 'bad_scenario', mutation: mutation.id });
    }
    if (requireScenario && !mutationScenario({ scenario: defaultScenario }, mutation)) {
      issues.push({ kind: 'missing_scenario', mutation: mutation.id });
    }
    const hasExactTargets = Array.isArray(mutation.targets);
    if (hasExactTargets) {
      const keys = mutation.targets.map(target => target && typeof target === 'object'
        ? criterionKey(target.feature, target.criterion) : '');
      if (mutation.targets.length === 0 || mutation.targets.some(target => !target
        || typeof target !== 'object' || typeof target.feature !== 'number'
        || !Number.isFinite(target.feature) || typeof target.criterion !== 'string'
        || !target.criterion) || new Set(keys).size !== keys.length) {
        issues.push({ kind: 'bad_targets', mutation: mutation.id });
      }
      if (mutation.breaks != null || mutation.kills != null) {
        issues.push({ kind: 'ambiguous_targets', mutation: mutation.id });
      }
    } else {
      if (typeof mutation.breaks !== 'number' || !Number.isFinite(mutation.breaks)) {
        issues.push({ kind: 'bad_feature', mutation: mutation.id });
      }
      if (!Array.isArray(mutation.kills) || mutation.kills.length === 0
        || mutation.kills.some(id => typeof id !== 'string' || !id)
        || new Set(mutation.kills).size !== mutation.kills.length) {
        issues.push({ kind: 'bad_kills', mutation: mutation.id });
      }
    }
    const edits = mutationEdits(mutation);
    if (edits.length === 0) issues.push({ kind: 'missing_edits', mutation: mutation.id });
    for (const edit of edits) {
      if (typeof edit.find !== 'string' || !edit.find || typeof edit.replace !== 'string' ||
        edit.find === edit.replace) issues.push({ kind: 'bad_edit', mutation: mutation.id });
    }
  }
  return { ok: issues.length === 0, issues };
}

export function indexMutationReport(report) {
  const criteria = new Map();
  const setupFailures = new Map();
  for (const feature of report?.features ?? []) {
    for (const criterion of feature.criteria ?? []) {
      const evidence = criterionEvidence(criterion);
      const disposition = evidenceDisposition(evidence);
      if (evidence.phase === 'setup' && !disposition.passed) {
        setupFailures.set(featureKey(feature.id), { feature: feature.id, detail: evidence.summary,
          status: evidence.status, outcomeKind: disposition.outcomeKind, code: evidence.code });
      }
      criteria.set(criterionKey(feature.id, criterion.id), {
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

export function validateMutationBaseline(report, mutations) {
  const indexed = indexMutationReport(report);
  const issues = [];
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

export function classifyMutationResult(baselineReport, mutantReport, mutation) {
  const baseline = indexMutationReport(baselineReport);
  const mutant = indexMutationReport(mutantReport);
  const targetKeys = new Set(mutationTargetKeys(mutation));
  const missing = [...baseline.criteria.keys()].filter(key => !mutant.criteria.has(key));
  const setupFailures = mutant.setupFailures.filter(item =>
    !baseline.setupFailures.some(base => featureKey(base.feature) === featureKey(item.feature)));
  const regressions = [];
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

  let status = 'CAUGHT';
  if (targetKeys.size === 0 || missing.length || targetMissing.length) status = 'INVALID_REPORT';
  else if (setupFailures.length) status = 'INVALID_SETUP';
  else if (targetHarnessFailures.length) status = 'INVALID_HARNESS_FAILURE';
  else if (targetInconclusive.length) status = 'INVALID_INCONCLUSIVE';
  else if (targetSurvived.length && collateral.length) status = 'WRONG_CRITERION';
  else if (targetSurvived.length) status = 'SURVIVED';
  else if (collateral.length) status = 'CAUGHT_COLLATERAL';

  return { status, targetKeys: [...targetKeys], targetMissing, targetHarnessFailures, targetInconclusive,
    targetSurvived, collateral, setupFailures, missing, regressions };
}
