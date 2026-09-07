import { cpSync, existsSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import type { ProgressionAttempt } from '../progression/progression-state.js';
import type { GradeBundlePayload } from './benchmark-run.js';
import { classifyBundle, ladderMayContinue } from './outcomes.js';
import type { RunOutcome } from './outcomes.js';
import {
  compareCriterionEvidence,
  type CriterionEvidenceComparison,
  type EvidenceBundle,
} from './scoring.js';

export interface RepairProgress {
  score: number | null;
  fingerprint: string;
  stalledRounds: number;
}

export function clearPrivateGradingEvidence(appDir: string): void {
  rmSync(join(resolve(appDir), 'stack-bench'), { recursive: true, force: true });
}

export function restorePrivateGradingEvidence(appDir: string, snapshot: string): void {
  if (!existsSync(snapshot)) throw new Error('repair grading snapshot does not exist');
  clearPrivateGradingEvidence(appDir);
  cpSync(snapshot, join(resolve(appDir), 'stack-bench'), { recursive: true });
}

export function repairProgressState(previous: RepairProgress | null,
  bundle: GradeBundlePayload | null): RepairProgress {
  const outcome = classifyBundle(bundle);
  const score = bundle?.totals?.score ?? null;
  const fingerprint = canonicalDefinitionJson({
    kind: outcome.kind,
    phase: outcome.phase ?? null,
    appFailures: [...(outcome.appFailures ?? [])].sort(),
    inconclusive: [...(outcome.inconclusive ?? [])].sort(),
    harnessFailures: [...(outcome.harnessFailures ?? [])].sort(),
    contractFailures: (bundle?.suites?.lint?.results ?? [])
      .filter(result => result.status === 'FAIL')
      .map(result => ({ id: result.id, detail: result.detail ?? null })),
  });
  const stalledRounds = previous && score !== null && previous.score !== null
    && score <= previous.score && fingerprint === previous.fingerprint
    ? previous.stalledRounds + 1 : 0;
  return { score, fingerprint, stalledRounds };
}

export function repairHistoryEntry(round: number, before: GradeBundlePayload | null,
  after: GradeBundlePayload | null, result: string) {
  const failureKeys = (bundle: GradeBundlePayload | null): string[] => {
    const outcome = classifyBundle(bundle);
    const contract = (bundle?.suites?.lint?.results ?? [])
      .filter(item => item.status === 'FAIL').map(item => `testing-interface/${item.id}`);
    return [...new Set([...(outcome.appFailures ?? []).filter(key => key !== 'contract-lint'),
      ...contract])].sort();
  };
  return {
    round,
    beforeScore: before?.totals?.score ?? null,
    beforeMax: before?.totals?.max ?? null,
    afterScore: after?.totals?.score ?? null,
    afterMax: after?.totals?.max ?? null,
    result,
    remainingFailures: failureKeys(after),
  };
}

export function levelGradeIsUsable(bundleOutcome: RunOutcome,
  progressionAttempt: Pick<ProgressionAttempt, 'outcome'> | null = null): boolean {
  if (progressionAttempt) return progressionAttempt.outcome === 'conclusive';
  return ladderMayContinue(bundleOutcome);
}

const APPLICATION_SETUP_PHASES = new Set([
  'database-provenance', 'application-layout', 'application-restart', 'application-readiness',
]);

interface RepairEvidenceBundle extends EvidenceBundle {
  outcome?: { kind?: string; phase?: string } | null;
  selection?: { checks?: Array<{ stableKey?: string }> } | null;
}

export interface RepairEvidenceDecision {
  action: 'keep-setup-repair' | 'rollback-no-comparison' | 'rollback-regression' | 'keep';
  shared: CriterionEvidenceComparison;
}

export function repairEvidenceDecision(
  beforeBundle: RepairEvidenceBundle | null | undefined,
  afterBundle: RepairEvidenceBundle | null | undefined,
): RepairEvidenceDecision {
  const shared = compareCriterionEvidence(beforeBundle, afterBundle);
  const startedFromApplicationSetup = beforeBundle?.outcome?.kind === 'app_failure'
    && typeof beforeBundle.outcome.phase === 'string'
    && APPLICATION_SETUP_PHASES.has(beforeBundle.outcome.phase);
  if (shared.count === 0 && shared.lostEvidence.length === 0
    && shared.definitionChanges.length === 0) {
    return {
      action: startedFromApplicationSetup ? 'keep-setup-repair' : 'rollback-no-comparison',
      shared,
    };
  }
  const evidenceRegressed = shared.lostEvidence.length > 0
    || shared.definitionChanges.length > 0
    || shared.regressions.length > 0;
  return {
    action: evidenceRegressed || shared.after < shared.before ? 'rollback-regression' : 'keep',
    shared,
  };
}

export function repairRegressionDecision(
  acceptedBundle: RepairEvidenceBundle | null | undefined,
  repairedBundle: RepairEvidenceBundle | null | undefined,
): RepairEvidenceDecision {
  const selected = repairedBundle?.selection?.checks;
  const selectedKeys = Array.isArray(selected)
    ? new Set(selected.flatMap(check => typeof check.stableKey === 'string'
      ? [check.stableKey] : [])) : null;
  const shared = compareCriterionEvidence(acceptedBundle, repairedBundle,
    { onlyPreviousPasses: true, previousStableKeys: selectedKeys });
  const regressed = shared.lostEvidence.length > 0
    || shared.definitionChanges.length > 0
    || shared.regressions.length > 0;
  return { action: regressed ? 'rollback-regression' : 'keep', shared };
}
