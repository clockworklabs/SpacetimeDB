export const GRADER_SOURCE_TIMEOUT_MS = 15 * 60_000;

// The fixed allowance covers setup and bundle creation. Each selected scenario
// then receives the same allowance as its child grader, up to a worker-safe cap.
const MIN_GRADING_RUN_TIMEOUT_MS = 20 * 60_000;
const MAX_GRADING_RUN_TIMEOUT_MS = 120 * 60_000;

export function selectedGradingSourceCount(...checkLists) {
  const sources = new Set();
  for (const checks of checkLists) {
    for (const check of checks ?? []) {
      const source = check.source ?? check.executionId;
      if (source) sources.add(source);
    }
  }
  return sources.size;
}

export function gradingRunTimeoutMs(sourceCount) {
  if (!Number.isSafeInteger(sourceCount) || sourceCount < 0) {
    throw new Error('grading source count must be a non-negative safe integer');
  }
  const scaled = MIN_GRADING_RUN_TIMEOUT_MS + sourceCount * GRADER_SOURCE_TIMEOUT_MS;
  return Math.min(scaled, MAX_GRADING_RUN_TIMEOUT_MS);
}
