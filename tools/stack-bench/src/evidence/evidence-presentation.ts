import {
  evidenceDisposition,
  type CheckEvidence,
} from './check-evidence.js';
import { sanitiseDiagnostic } from './diagnostic-sanitizer.js';
import type { RunLevelRecord } from './benchmark-run.js';

export function evidenceStatusLabel(evidence: CheckEvidence): string {
  return evidenceDisposition(evidence).label;
}

interface RenderEvidenceOptions {
  includeSummary?: boolean;
}

export function renderEvidenceConsoleLine(
  evidence: CheckEvidence,
  subject: string,
  { includeSummary = true }: RenderEvidenceOptions = {},
): string {
  const label = evidenceStatusLabel(evidence);
  const summary = includeSummary ? sanitiseDiagnostic(evidence.summary, 600) : '';
  return `${label} ${subject}${summary ? `: ${summary}` : ''}`;
}

interface LevelSummaryInput {
  level: number;
  graded: boolean;
  score?: number | null;
  max?: number | null;
  firstBuild?: RunLevelRecord['firstBuild'];
  baseline?: RunLevelRecord['baseline'];
  repairs?: number;
  buildCostUsd?: number;
  resumeCostUsd?: number;
  repairCostUsd?: number;
  durationSec?: number;
  durationMs?: number;
  error?: string;
  repair?: RunLevelRecord['repair'];
}

export function formatLevelSummary(level: LevelSummaryInput): string {
  const starting = level.firstBuild?.score != null
    ? `${level.firstBuild.score}/${level.firstBuild.max} unaided -> `
    : level.baseline?.score != null ? `${level.baseline.score}/${level.baseline.max} resumed -> ` : '';
  const score = level.graded ? `${starting}${level.score}/${level.max}` : 'NOT GRADED';
  const repairs = Number.isInteger(level.repairs) ? level.repairs : 0;
  const repairLabel = `${repairs} ${repairs === 1 ? 'repair' : 'repairs'}`;
  const totalCost = (level.buildCostUsd ?? level.resumeCostUsd ?? 0) + (level.repairCostUsd ?? 0);
  const durationSec = Number.isFinite(level.durationSec)
    ? level.durationSec : Math.round((level.durationMs ?? 0) / 1000);
  const status = level.error
    ? `stopped: ${level.error.replaceAll('-', ' ')}`
    : level.repair?.status?.replaceAll('-', ' ') ?? 'complete';
  return `L${level.level}: ${score} | ${repairLabel} | $${totalCost.toFixed(2)} total`
    + ` ($${(level.repairCostUsd ?? 0).toFixed(2)} repairs) | ${status} | ${durationSec}s`;
}
