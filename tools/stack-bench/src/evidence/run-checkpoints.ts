import { z } from 'zod';
import type { GradeBundlePayload } from './benchmark-run.js';
import { criterionEvidence, evidenceDisposition } from './check-evidence.js';
import { checkCompletion, type CheckCompletion, type CheckStatus } from './check-completion.js';
import { sessionCostEvidence, sumCostEvidence, type CostEvidence, type CostLevel, type CostSession } from './cost-proof.js';

const hash = z.string().regex(/^[a-f0-9]{64}$/);
export const costEvidenceSchema = z.discriminatedUnion('status', [
  z.strictObject({ status: z.literal('unknown'), costUsd: z.null() }),
  z.strictObject({ status: z.enum(['exact', 'upper-bound']), costUsd: z.number().finite().nonnegative() }),
]);
export const completionSchema = z.strictObject({
  selected: z.number().int().nonnegative(), passed: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(), blocked: z.number().int().nonnegative(),
  unmeasured: z.number().int().nonnegative(), rate: z.number().min(0).max(1).nullable(),
}).refine(value => value.selected === value.passed + value.failed + value.blocked + value.unmeasured
  && value.rate === (value.selected ? Number((value.passed / value.selected).toFixed(6)) : null));
export const checkpointSchema = z.strictObject({
  sequence: z.number().int().positive(), phase: z.enum(['first-build', 'repair', 'final']),
  level: z.number().int().positive(), accepted: z.boolean(),
  excluded: z.boolean().optional(),
  workNodeIds: z.array(z.string().min(1)),
  sourceSha256: hash, selectionSha256: hash,
  evidence: z.strictObject({ path: z.string().min(1).refine(path => !path.startsWith('/')
    && !path.includes('\\') && !path.split('/').includes('..') && !path.includes(':')), sha256: hash }),
  cost: costEvidenceSchema, executionCost: costEvidenceSchema, completion: completionSchema,
  checks: z.array(z.strictObject({ id: z.string().min(1), status: z.enum(['passed', 'failed', 'blocked', 'unmeasured']) })),
});
export type RunCheckpoint = z.infer<typeof checkpointSchema>;

export interface CheckpointRun {
  checkpoints?: RunCheckpoint[];
  levels?: CostLevel[];
  progressionResume?: { inheritedLevels: number[] };
  condition?: { requested?: { levels?: Array<{ selection?: {
    scoredChecks?: Array<{ stableKey: string; points: number }>;
  } }> } };
}

/** Called at a measured grade, while its cumulative session list is known. */
export function recordRunCheckpoint(run: CheckpointRun, input: {
  phase: RunCheckpoint['phase']; level: number; bundle: GradeBundlePayload;
  sourceSha256: string; evidence: RunCheckpoint['evidence'];
  extraSessions?: readonly CostSession[]; priorCost?: CostEvidence; accepted?: boolean; workNodeIds?: string[];
  initialChecks?: RunCheckpoint['checks'];
}): RunCheckpoint {
  const selected = (run.condition?.requested?.levels ?? []).flatMap(level =>
    (level.selection?.scoredChecks ?? []).map(check => ({ id: check.stableKey, points: check.points })));
  if (!selected.length) throw new Error('a grading checkpoint requires a declared check scope');
  const checkpoints = run.checkpoints ??= [];
  const previous = checkpoints.findLast(checkpoint => checkpoint.accepted);
  const checks = checkpointChecks(selected, previous?.checks ?? input.initialChecks ?? [], input.bundle);
  const outcomes = new Map(checks.map(check => [check.id, check.status]));
  const checkpoint = checkpointSchema.parse({ sequence: checkpoints.length + 1,
    phase: input.phase, level: input.level, accepted: input.accepted ?? true,
    workNodeIds: input.workNodeIds ?? [],
    sourceSha256: input.sourceSha256, selectionSha256: input.bundle.selection?.sha256,
    evidence: input.evidence, cost: sumCostEvidence([
      input.priorCost ?? (run.progressionResume ? { status: 'unknown', costUsd: null }
        : { status: 'exact', costUsd: 0 }),
      sessionCostEvidence(checkpointSessions(run, input.extraSessions ?? [])),
    ]),
    executionCost: sessionCostEvidence(checkpointSessions(run, input.extraSessions ?? [])),
    completion: checkCompletion(selected, outcomes),
    checks });
  checkpoints.push(checkpoint);
  return checkpoint;
}

/** Restrict observations to the declared scored scope; missing checks stay unmeasured. */
export function checkpointChecks(selected: Array<{ id: string; points: number }>,
  previous: RunCheckpoint['checks'], bundle: GradeBundlePayload): RunCheckpoint['checks'] {
  const outcomes = new Map<string, CheckStatus>(previous.map(check => [check.id, check.status]));
  const measured = new Set(bundle.selection?.reportedChecks ?? []);
  for (const suite of Object.values(bundle.suites ?? {})) {
    for (const feature of suite.features ?? []) for (const check of feature.criteria ?? []) {
      if (!check.stableKey || !measured.has(check.stableKey)) continue;
      const evidence = criterionEvidence(check);
      const disposition = evidenceDisposition(evidence);
      outcomes.set(check.stableKey, evidence.status === 'blocked' ? 'blocked' : disposition.passed ? 'passed'
        : disposition.outcomeKind === 'app_failure' ? 'failed' : 'unmeasured');
    }
  }
  return [...new Set(selected.filter(check => check.points > 0).map(check => check.id))]
    .map(id => ({ id, status: outcomes.get(id) ?? 'unmeasured' }));
}

/** Extra sessions are the active build/repairs not yet appended to run.levels. */
export function checkpointSessions(run: Pick<CheckpointRun, 'levels' | 'progressionResume'>,
  extraSessions: readonly CostSession[]): CostSession[] {
  const inherited = new Set(run.progressionResume?.inheritedLevels ?? []);
  const persisted = (run.levels ?? []).filter(level => !inherited.has(level.level)).flatMap(level =>
    [...(level.buildSessions ?? []), ...(level.resumeSession ? [level.resumeSession] : []),
      ...(level.repairSessions ?? [])]);
  // Repeated references are already owned by the level. Different paid sessions
  // can share a conversation id, cost, and token count; never deduplicate those.
  return [...new Set([...persisted, ...extraSessions])];
}

export interface CompletionCurve {
  checkpoints: RunCheckpoint[];
  completionAtSpend: Array<{ budgetUsd: number; status: 'measured' | 'unmeasured';
    completion: CheckCompletion | null; sequence: number | null }>;
  costToCompletion: Array<{ targetRate: number; status: 'reached' | 'not-reached' | 'unmeasured';
    cost: CostEvidence; sequence: number | null }>;
}

export function completionCurve(checkpoints: RunCheckpoint[], thresholds: number[], targets: number[]): CompletionCurve {
  const accepted = checkpoints.filter(checkpoint => checkpoint.accepted && !checkpoint.excluded);
  return { checkpoints,
    completionAtSpend: thresholds.map(budgetUsd => {
      const checkpoint = accepted.findLast(item => item.cost.status !== 'unknown' && item.cost.costUsd <= budgetUsd);
      return { budgetUsd, status: checkpoint ? 'measured' : 'unmeasured',
        completion: checkpoint?.completion ?? null, sequence: checkpoint?.sequence ?? null };
    }),
    costToCompletion: targets.map(targetRate => {
      const checkpoint = accepted.find(item => item.completion.rate !== null && item.completion.rate >= targetRate);
      return { targetRate, status: checkpoint ? 'reached' : accepted.length ? 'not-reached' : 'unmeasured',
        cost: checkpoint?.cost ?? { status: 'unknown', costUsd: null }, sequence: checkpoint?.sequence ?? null };
    }) };
}
