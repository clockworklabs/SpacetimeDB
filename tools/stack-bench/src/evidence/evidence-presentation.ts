import {
  evidenceDisposition,
  evidenceIsRepairable,
  type CheckEvidence,
} from './check-evidence.mjs';
import { humaniseDiagnostic, sanitiseDiagnostic } from './diagnostic-sanitizer.js';

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
  return `${label} ${subject}${summary ? ` — ${summary}` : ''}`;
}

export function renderRepairDiagnostic(evidence: CheckEvidence): string {
  if (!evidenceIsRepairable(evidence)) {
    throw new Error(`cannot render a repair diagnostic for ${evidenceStatusLabel(evidence)} evidence`);
  }
  return humaniseDiagnostic(evidence.summary);
}
