import { compareCriterionEvidence } from './scoring.mjs';

const APPLICATION_SETUP_PHASES = new Set([
  'database-provenance', 'application-layout', 'application-restart',
]);

export function repairEvidenceDecision(beforeBundle, afterBundle) {
  const shared = compareCriterionEvidence(beforeBundle, afterBundle);
  const startedFromApplicationSetup = beforeBundle?.outcome?.kind === 'app_failure'
    && APPLICATION_SETUP_PHASES.has(beforeBundle.outcome.phase);
  if (shared.count === 0 && shared.lostEvidence.length === 0
    && shared.definitionChanges.length === 0) {
    return { action: startedFromApplicationSetup ? 'keep-setup-repair' : 'rollback-no-comparison',
      shared };
  }
  const evidenceRegressed = shared.lostEvidence.length > 0 || shared.definitionChanges.length > 0;
  return { action: evidenceRegressed || shared.after < shared.before ? 'rollback-regression' : 'keep',
    shared };
}
