import {
  compareCriterionEvidence,
  type CriterionEvidenceComparison,
  type EvidenceBundle,
} from './scoring.js';

const APPLICATION_SETUP_PHASES = new Set([
  'database-provenance', 'application-layout', 'application-restart', 'application-readiness',
]);

interface RepairEvidenceBundle extends EvidenceBundle {
  outcome?: { kind?: string; phase?: string } | null;
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
  const evidenceRegressed = shared.lostEvidence.length > 0 || shared.definitionChanges.length > 0;
  return {
    action: evidenceRegressed || shared.after < shared.before ? 'rollback-regression' : 'keep',
    shared,
  };
}
