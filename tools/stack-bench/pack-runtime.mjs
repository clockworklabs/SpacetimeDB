import { criterionEvidence, validateCheckEvidence } from './check-evidence.mjs';

export const PACK_RUNTIME_METRIC = 'pack-check-wall-clock-sum-v1';

function positiveInteger(value, at) {
  if (!Number.isInteger(value) || value < 0) throw new Error(`${at} must be a non-negative integer`);
  return value;
}

export function measureGradePackRuntime(report) {
  const selected = report?.selection?.checks;
  if (!Array.isArray(selected) || selected.length === 0) {
    throw new Error('pack runtime measurement requires a non-empty recipe selection');
  }
  const byKey = new Map();
  for (const check of selected) {
    if (typeof check.stableKey !== 'string' || !check.stableKey
      || typeof check.packId !== 'string' || !check.packId) {
      throw new Error('pack runtime selection requires stableKey and packId');
    }
    if (byKey.has(check.stableKey)) throw new Error(`pack runtime selection repeats ${check.stableKey}`);
    byKey.set(check.stableKey, check.packId);
  }

  const totals = new Map();
  const seen = new Set();
  for (const feature of report.features ?? []) {
    const featurePacks = new Set();
    for (const criterion of feature.criteria ?? []) {
      const stableKey = criterion.stableKey;
      const packId = byKey.get(stableKey);
      if (!packId) throw new Error(`graded criterion has unknown stable key ${stableKey ?? '<missing>'}`);
      if (seen.has(stableKey)) throw new Error(`graded criterion repeats stable key ${stableKey}`);
      seen.add(stableKey);
      featurePacks.add(packId);
      const evidence = criterionEvidence(criterion);
      const durationMs = positiveInteger(evidence.timing.durationMs,
        `${stableKey} durationMs`);
      const current = totals.get(packId) ?? { id: packId, checkCount: 0,
        setupRuntimeMs: 0, criterionRuntimeMs: 0, measuredRuntimeMs: 0 };
      current.checkCount += 1;
      // A setup failure projects the one setup observation onto every affected
      // criterion. Count that shared setup once below, never once per criterion.
      if (evidence.phase === 'assertion') current.criterionRuntimeMs += durationMs;
      totals.set(packId, current);
    }
    validateCheckEvidence(feature.setupEvidence, { at: `feature ${feature.id ?? '<unknown>'}.setupEvidence` });
    if (feature.setupEvidence.phase !== 'setup') {
      throw new Error(`feature ${feature.id ?? '<unknown>'}.setupEvidence must use setup phase`);
    }
    const setupRuntimeMs = positiveInteger(feature.setupEvidence.timing.durationMs,
      `feature ${feature.id ?? '<unknown>'} setup durationMs`);
    // If two packs share a feature, each is charged the complete setup it would
    // require when selected alone. This deliberately avoids timing-order or
    // scoring-weight allocation rules.
    for (const packId of featurePacks) totals.get(packId).setupRuntimeMs += setupRuntimeMs;
  }
  const missing = [...byKey.keys()].filter(key => !seen.has(key));
  if (missing.length) throw new Error(`pack runtime evidence is missing ${missing.join(', ')}`);
  for (const measured of totals.values()) {
    measured.measuredRuntimeMs = measured.setupRuntimeMs + measured.criterionRuntimeMs;
  }
  return {
    schemaVersion: 1,
    metric: PACK_RUNTIME_METRIC,
    packs: [...totals.values()].sort((a, b) => a.id.localeCompare(b.id)),
  };
}

export function aggregatePackRuntime(suiteReports, packDefinitions) {
  if (!Array.isArray(suiteReports)) throw new Error('suiteReports must be an array');
  if (!Array.isArray(packDefinitions)) throw new Error('packDefinitions must be an array');
  const definitions = new Map();
  for (const pack of packDefinitions) {
    if (definitions.has(pack.id)) throw new Error(`packDefinitions repeats ${pack.id}`);
    definitions.set(pack.id, pack);
  }
  const totals = new Map();
  for (const report of suiteReports) {
    if (!report?.packRuntime) continue;
    if (report.packRuntime.schemaVersion !== 1 || report.packRuntime.metric !== PACK_RUNTIME_METRIC
      || !Array.isArray(report.packRuntime.packs)) {
      throw new Error('suite pack runtime uses an unsupported measurement format');
    }
    for (const measured of report.packRuntime.packs) {
      const definition = definitions.get(measured.id);
      if (!definition) throw new Error(`suite measured unselected pack ${measured.id}`);
      const current = totals.get(measured.id) ?? { id: measured.id, checkCount: 0,
        setupRuntimeMs: 0, criterionRuntimeMs: 0, measuredRuntimeMs: 0 };
      current.checkCount += positiveInteger(measured.checkCount, `${measured.id} checkCount`);
      current.setupRuntimeMs += positiveInteger(measured.setupRuntimeMs,
        `${measured.id} setupRuntimeMs`);
      current.criterionRuntimeMs += positiveInteger(measured.criterionRuntimeMs,
        `${measured.id} criterionRuntimeMs`);
      current.measuredRuntimeMs += positiveInteger(measured.measuredRuntimeMs,
        `${measured.id} measuredRuntimeMs`);
      totals.set(measured.id, current);
    }
  }
  return {
    schemaVersion: 1,
    metric: PACK_RUNTIME_METRIC,
    packs: [...totals.values()].sort((a, b) => a.id.localeCompare(b.id)).map(measured => {
      const budget = structuredClone(definitions.get(measured.id).budget);
      const maxRuntimeMs = budget.status === 'bounded' ? budget.maxRuntimeMs : null;
      return { ...measured, budget,
        exceeded: maxRuntimeMs === null ? null : measured.measuredRuntimeMs > maxRuntimeMs };
    }),
  };
}

export function exceededPackBudgets(runtime) {
  return (runtime?.packs ?? []).filter(pack => pack.exceeded === true);
}
