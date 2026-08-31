import { criterionEvidence, validateCheckEvidence } from '../evidence/check-evidence.js';
import type { CompletedGradeReport } from '../evidence/grade-report.js';
import type { CompiledPackBudget } from './composition-compiler.js';

export const PACK_RUNTIME_METRIC = 'pack-check-wall-clock-sum-v1' as const;

export interface PackRuntimeMeasurement {
  id: string;
  checkCount: number;
  setupRuntimeMs: number;
  criterionRuntimeMs: number;
  measuredRuntimeMs: number;
}

export interface PackRuntimeEvidence {
  schemaVersion: 1;
  metric: typeof PACK_RUNTIME_METRIC;
  packs: PackRuntimeMeasurement[];
}

interface PackDefinition {
  id: string;
  budget: CompiledPackBudget;
}

export interface AggregatedPackRuntimeMeasurement extends PackRuntimeMeasurement {
  budget: CompiledPackBudget;
  exceeded: boolean | null;
}

export interface AggregatedPackRuntimeEvidence {
  schemaVersion: 1;
  metric: typeof PACK_RUNTIME_METRIC;
  packs: AggregatedPackRuntimeMeasurement[];
}

function positiveInteger(value: unknown, at: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw new Error(`${at} must be a non-negative integer`);
  }
  return value;
}

export function measureGradePackRuntime(report: CompletedGradeReport): PackRuntimeEvidence {
  const selected = report.selection?.checks;
  if (!Array.isArray(selected) || selected.length === 0) {
    throw new Error('pack runtime measurement requires a non-empty recipe selection');
  }
  const byKey = new Map<string, string>();
  for (const check of selected) {
    if (typeof check.stableKey !== 'string' || !check.stableKey
      || typeof check.packId !== 'string' || !check.packId) {
      throw new Error('pack runtime selection requires stableKey and packId');
    }
    if (byKey.has(check.stableKey)) {
      throw new Error(`pack runtime selection repeats ${check.stableKey}`);
    }
    byKey.set(check.stableKey, check.packId);
  }

  const totals = new Map<string, PackRuntimeMeasurement>();
  const seen = new Set<string>();
  for (const feature of report.features) {
    const featurePacks = new Set<string>();
    for (const criterion of feature.criteria ?? []) {
      const stableKey = criterion.stableKey;
      if (typeof stableKey !== 'string') {
        throw new Error(`graded criterion has unknown stable key ${String(stableKey ?? '<missing>')}`);
      }
      const packId = byKey.get(stableKey);
      if (!packId) throw new Error(`graded criterion has unknown stable key ${stableKey}`);
      if (seen.has(stableKey)) throw new Error(`graded criterion repeats stable key ${stableKey}`);
      seen.add(stableKey);
      featurePacks.add(packId);
      const evidence = criterionEvidence(criterion);
      const durationMs = positiveInteger(evidence.timing.durationMs, `${stableKey} durationMs`);
      const current = totals.get(packId) ?? {
        id: packId,
        checkCount: 0,
        setupRuntimeMs: 0,
        criterionRuntimeMs: 0,
        measuredRuntimeMs: 0,
      };
      current.checkCount += 1;
      // A setup failure projects one setup observation onto every affected
      // criterion. Count that shared setup once per pack below.
      if (evidence.phase === 'assertion') current.criterionRuntimeMs += durationMs;
      totals.set(packId, current);
    }
    const setupEvidence = validateCheckEvidence(feature.setupEvidence, {
      at: `feature ${String(feature.id ?? '<unknown>')}.setupEvidence`,
    });
    if (setupEvidence.phase !== 'setup') {
      throw new Error(`feature ${String(feature.id ?? '<unknown>')}.setupEvidence must use setup phase`);
    }
    const setupRuntimeMs = positiveInteger(
      setupEvidence.timing.durationMs,
      `feature ${String(feature.id ?? '<unknown>')} setup durationMs`,
    );
    // If two packs share a feature, each is charged the complete setup it would
    // require when selected alone. This deliberately avoids timing-order or
    // scoring-weight allocation rules.
    for (const packId of featurePacks) {
      const total = totals.get(packId);
      if (total) total.setupRuntimeMs += setupRuntimeMs;
    }
  }
  const missing = [...byKey.keys()].filter((key) => !seen.has(key));
  if (missing.length) throw new Error(`pack runtime evidence is missing ${missing.join(', ')}`);
  for (const measured of totals.values()) {
    measured.measuredRuntimeMs = measured.setupRuntimeMs + measured.criterionRuntimeMs;
  }
  return {
    schemaVersion: 1,
    metric: PACK_RUNTIME_METRIC,
    packs: [...totals.values()].sort((left, right) => left.id.localeCompare(right.id)),
  };
}

export function aggregatePackRuntime(
  suiteReports: Array<{ packRuntime?: PackRuntimeEvidence }>,
  packDefinitions: PackDefinition[],
): AggregatedPackRuntimeEvidence {
  if (!Array.isArray(suiteReports)) throw new Error('suiteReports must be an array');
  if (!Array.isArray(packDefinitions)) throw new Error('packDefinitions must be an array');
  const definitions = new Map<string, PackDefinition>();
  for (const pack of packDefinitions) {
    if (definitions.has(pack.id)) throw new Error(`packDefinitions repeats ${pack.id}`);
    definitions.set(pack.id, pack);
  }
  const totals = new Map<string, PackRuntimeMeasurement>();
  for (const report of suiteReports) {
    if (!report?.packRuntime) continue;
    if (report.packRuntime.schemaVersion !== 1 || report.packRuntime.metric !== PACK_RUNTIME_METRIC
      || !Array.isArray(report.packRuntime.packs)) {
      throw new Error('suite pack runtime uses an unsupported measurement format');
    }
    for (const measured of report.packRuntime.packs) {
      const definition = definitions.get(measured.id);
      if (!definition) throw new Error(`suite measured unselected pack ${measured.id}`);
      const current = totals.get(measured.id) ?? {
        id: measured.id,
        checkCount: 0,
        setupRuntimeMs: 0,
        criterionRuntimeMs: 0,
        measuredRuntimeMs: 0,
      };
      current.checkCount += positiveInteger(measured.checkCount, `${measured.id} checkCount`);
      current.setupRuntimeMs += positiveInteger(measured.setupRuntimeMs, `${measured.id} setupRuntimeMs`);
      current.criterionRuntimeMs += positiveInteger(
        measured.criterionRuntimeMs,
        `${measured.id} criterionRuntimeMs`,
      );
      current.measuredRuntimeMs += positiveInteger(
        measured.measuredRuntimeMs,
        `${measured.id} measuredRuntimeMs`,
      );
      totals.set(measured.id, current);
    }
  }
  return {
    schemaVersion: 1,
    metric: PACK_RUNTIME_METRIC,
    packs: [...totals.values()]
      .sort((left, right) => left.id.localeCompare(right.id))
      .map((measured) => {
        const definition = definitions.get(measured.id);
        if (!definition) throw new Error(`pack definition is missing ${measured.id}`);
        const budget = structuredClone(definition.budget);
        const maxRuntimeMs = budget.status === 'bounded' ? budget.maxRuntimeMs : null;
        return {
          ...measured,
          budget,
          exceeded: maxRuntimeMs === null ? null : measured.measuredRuntimeMs > maxRuntimeMs,
        };
      }),
  };
}

export function exceededPackBudgets(
  runtime: { packs?: AggregatedPackRuntimeMeasurement[] } | null | undefined,
): AggregatedPackRuntimeMeasurement[] {
  return (runtime?.packs ?? []).filter((pack) => pack.exceeded === true);
}
