import { evidenceDisposition, evidenceIsRepairable } from './check-evidence.mjs';
import { humaniseDiagnostic, sanitiseDiagnostic } from './diagnostic-sanitizer.mjs';

export function evidenceStatusLabel(evidence) {
  return evidenceDisposition(evidence).label;
}

export function renderEvidenceConsoleLine(evidence, subject, { includeSummary = true } = {}) {
  const label = evidenceStatusLabel(evidence);
  const summary = includeSummary ? sanitiseDiagnostic(evidence?.summary, 600) : '';
  return `${label} ${subject}${summary ? ` — ${summary}` : ''}`;
}

export function renderRepairDiagnostic(evidence) {
  if (!evidenceIsRepairable(evidence)) {
    throw new Error(`cannot render a repair diagnostic for ${evidenceStatusLabel(evidence)} evidence`);
  }
  return humaniseDiagnostic(evidence.summary);
}
