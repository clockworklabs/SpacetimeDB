import type { CompiledRecipePlan } from '../composition/composition-compiler.js';

export const GRADER_SOURCE_TIMEOUT_MS = 15 * 60_000;

// The fixed allowance covers setup and bundle creation. Each selected scenario
// then receives the same allowance as its child grader, up to a worker-safe cap.
const MIN_GRADING_RUN_TIMEOUT_MS = 20 * 60_000;
const MAX_GRADING_RUN_TIMEOUT_MS = 120 * 60_000;

interface GradingCheck {
  executionId?: string;
  source?: string;
  packId?: string;
}

type GradingPacks = ReadonlyArray<Pick<CompiledRecipePlan['packs'][number], 'id' | 'budget'>>;

// A pack budget measures reference work, not a per-check latency requirement.
// Do not divide it by check count: selected checks can have very different costs.
export function gradingSourceTimeoutMs(packs: GradingPacks, checks: readonly GradingCheck[]): number {
  const selected = new Set(checks.map(check => check.packId));
  const budgetMs = packs.filter(pack => selected.has(pack.id))
    .reduce((total, pack) => total + (pack.budget.maxRuntimeMs ?? 0), 0);
  return Math.max(GRADER_SOURCE_TIMEOUT_MS, budgetMs + 60_000);
}

export function selectedGradingSourceCount(
  ...checkLists: ReadonlyArray<ReadonlyArray<GradingCheck> | null | undefined>
): number {
  const sources = new Set();
  for (const checks of checkLists) {
    for (const check of checks ?? []) {
      const source = check.source ?? check.executionId;
      if (source) sources.add(source);
    }
  }
  return sources.size;
}

export function gradingRunTimeoutMs(sourceCount: number, packs: GradingPacks = [],
  checks: readonly GradingCheck[] = []): number {
  if (!Number.isSafeInteger(sourceCount) || sourceCount < 0) {
    throw new Error('grading source count must be a non-negative safe integer');
  }
  const sources = new Set(checks.map(check => check.source ?? check.executionId).filter(Boolean));
  let extraMs = 0;
  for (const source of sources) {
    extraMs += gradingSourceTimeoutMs(packs,
      checks.filter(check => (check.source ?? check.executionId) === source)) - GRADER_SOURCE_TIMEOUT_MS;
  }
  const scaled = MIN_GRADING_RUN_TIMEOUT_MS + sourceCount * GRADER_SOURCE_TIMEOUT_MS + extraMs;
  return Math.min(scaled, MAX_GRADING_RUN_TIMEOUT_MS);
}
