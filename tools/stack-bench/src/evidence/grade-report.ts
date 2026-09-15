import type { CheckEvidence, CheckEvidencePhase, CheckEvidenceStatus }
  from './check-evidence.js';
import { evidencePassed, validateCheckEvidence } from './check-evidence.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import type { CompiledScenarioDefinition } from '../composition/definition-compiler.js';
import type { ActionEvidence } from '../actions/action-contract.js';
import { checkoutStateSchema } from '../stacks/checkout-state.js';
import { z } from 'zod';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { validateArtifact } from './artifact-schema.js';

export interface GradeCleanupFailure {
  actor: string | null;
  stage: string;
  reason: string;
}

export interface GradeCriterionResult {
  id: string;
  desc: string;
  points: number;
  evidence: CheckEvidence;
  stableKey?: string;
  serverCheck?: string;
}

export interface CompletedGradeFeatureResult {
  id: number;
  name: string;
  score: number;
  max: number;
  criteria: GradeCriterionResult[];
  consoleErrors: string[];
  setupEvidence: CheckEvidence;
  cleanupEvidence?: { status: 'harness_failure'; failures: GradeCleanupFailure[] };
  inconclusive?: Array<{
    id: string;
    points: number;
    status: CheckEvidenceStatus;
    code: string;
    phase: CheckEvidencePhase;
    summary: string | null;
  }>;
  screenshots?: string[];
  videos?: string[];
  unverified?: string[];
  verified?: string[];
}

export interface GradeSelectionCheck {
  stableKey: string;
  packId: string;
}

export interface CompletedGradeReport {
  selection: { checks: GradeSelectionCheck[] } | null;
  features: CompletedGradeFeatureResult[];
  total: number;
  max: number;
}

export function verifyPopulatedPreparation(scenario: CompiledScenarioDefinition,
  report: CompletedGradeReport & { cleanupEvidence?: unknown }): void {
  const expected = scenario.features.flatMap(feature => feature.criteria.map(check => `${feature.id}/${check.id}`)).sort();
  const actual = report.features.flatMap(feature => feature.criteria.map(check => `${feature.id}/${check.id}`)).sort();
  if (!expected.length || canonicalDefinitionJson(expected) !== canonicalDefinitionJson(actual)
    || report.cleanupEvidence || report.total !== 0 || report.max !== 0
    || report.features.some(feature => !evidencePassed(feature.setupEvidence) || feature.cleanupEvidence
      || feature.criteria.some(check => check.points !== 0 || !evidencePassed(check.evidence)))) {
    throw new Error('populated starting preparation did not produce complete passing evidence');
  }
}

const checkoutObservation = z.strictObject({
  key: z.string().min(1), account: z.string().min(1), item: z.string().min(1),
  schemaSha256: z.record(z.string(), z.string().regex(/^[a-f0-9]{64}$/)), state: checkoutStateSchema,
});
export type PreparedCheckout = z.infer<typeof checkoutObservation>;

export function readPreparedCheckouts(scenario: CompiledScenarioDefinition, path: string, sha256: string): PreparedCheckout[] {
  const bytes = readFileSync(path);
  if (createHash('sha256').update(bytes).digest('hex') !== sha256) {
    throw new Error('populated preparation evidence changed after checkpoint capture');
  }
  const artifact = validateArtifact<CompletedGradeReport>(JSON.parse(bytes.toString('utf8')), { source: path });
  if (artifact.kind !== 'grade') throw new Error('populated preparation requires a grade artifact');
  return preparedCheckouts(scenario, artifact.payload);
}

// Read only the successful observations produced by the compiled preparation.
// Candidate grading cannot replace these with its own dbRecordCheckout actions.
export function preparedCheckouts(scenario: CompiledScenarioDefinition, report: CompletedGradeReport): PreparedCheckout[] {
  verifyPopulatedPreparation(scenario, report);
  const snapshots: PreparedCheckout[] = [];
  for (const feature of scenario.features) {
    const measured = report.features.find(value => value.id === feature.id)!;
    const groups = [{ steps: feature.setup, evidence: measured.setupEvidence },
      ...feature.criteria.map(check => ({ steps: check.steps,
        evidence: measured.criteria.find(value => value.id === check.id)!.evidence }))];
    for (const { steps, evidence } of groups) {
      const expected = steps.filter(step => step.do === 'dbRecordCheckout');
      const observed = validateCheckEvidence(evidence).actions
        .map(entry => entry.evidence as ActionEvidence).filter(entry => entry.action.id === 'dbRecordCheckout');
      if (observed.length !== expected.length) throw new Error('preparation checkout observations are missing or duplicated');
      observed.forEach((entry, index) => {
        const snapshot = checkoutObservation.parse(entry.observation);
        if (entry.status !== 'passed' || snapshot.key !== expected[index]!.as
          || snapshots.some(prior => prior.key === snapshot.key)) {
          throw new Error('preparation checkout observation differs from its compiled step');
        }
        snapshots.push(snapshot);
      });
    }
  }
  return snapshots;
}
