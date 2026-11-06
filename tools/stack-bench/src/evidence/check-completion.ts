export type CheckStatus = 'passed' | 'failed' | 'blocked' | 'unmeasured';

export interface CheckCompletion {
  selected: number;
  passed: number;
  failed: number;
  blocked: number;
  unmeasured: number;
  rate: number | null;
}

/** Counts checks, not weighted points. Missing observations retain the full scope. */
export function checkCompletion(checks: readonly { id: string; points: number }[],
  outcomes: ReadonlyMap<string, CheckStatus>): CheckCompletion {
  const selected = new Set(checks.filter(check => check.points > 0).map(check => check.id));
  const result: CheckCompletion = { selected: selected.size, passed: 0, failed: 0,
    blocked: 0, unmeasured: 0, rate: null };
  for (const id of selected) result[outcomes.get(id) ?? 'unmeasured'] += 1;
  result.rate = selected.size ? Number((result.passed / selected.size).toFixed(6)) : null;
  return result;
}
