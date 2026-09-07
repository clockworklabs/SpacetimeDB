export interface CostReceiptEntry {
  receipt?: {
    complete?: boolean;
    reconciled?: boolean;
    error?: unknown;
    costUsd?: number;
    exact?: boolean;
  };
  [key: string]: unknown;
}

export interface CostSession {
  costUsd: number;
  costComplete?: boolean;
  costReceipts?: unknown[];
  usage?: unknown;
}

export interface CostLevel {
  level: number;
  buildSessions?: CostSession[];
  resumeSession?: CostSession;
  repairSessions?: CostSession[];
}

export interface CostRun {
  id?: string | null;
  levels?: CostLevel[];
  pricing?: unknown;
  progressionResume?: { inheritedLevels: number[] };
  totals?: {
    costComplete?: boolean;
    costUsd?: number | null;
    currentExecutionCostUsd?: number;
  };
}

export interface CostLedgerRow {
  level: number;
  kind: 'build' | 'resume' | 'repair';
  index: number;
  sessionCostUsd: number;
  costComplete: boolean;
  receipts: CostReceiptEntry[];
  receiptCostUsd: number;
  differenceUsd: number;
  complete: boolean;
  // False when a receipt charged a request its cost ceiling instead of exact
  // provider usage; the row's cost is then an upper bound.
  exact: boolean;
}

export interface CostLedger {
  complete: boolean;
  exact: boolean;
  differenceUsd: number;
  receiptCostUsd: number;
  reportedCostUsd: number;
  rows: CostLedgerRow[];
  runId: string | null;
  pricing: unknown;
}

export type CostEvidence = { status: 'unknown'; costUsd: null }
  | { status: 'exact' | 'upper-bound'; costUsd: number };

export function sessionCostEvidence(sessions: readonly CostSession[]): CostEvidence {
  if (sessions.some(session => typeof session.costUsd !== 'number'
    || !Number.isFinite(session.costUsd) || session.costUsd < 0)) {
    return { status: 'unknown', costUsd: null };
  }
  return runCostEvidence({ levels: [{ level: 1, buildSessions: sessions }],
    totals: { costComplete: true, costUsd: roundUsd(sessions.reduce((sum, session) => sum + session.costUsd, 0)) } });
}

export function sumCostEvidence(costs: readonly CostEvidence[]): CostEvidence {
  if (costs.some(item => item.status === 'unknown')) return { status: 'unknown', costUsd: null };
  return { status: costs.some(item => item.status === 'upper-bound') ? 'upper-bound' : 'exact',
    costUsd: roundUsd(costs.reduce((sum, item) => sum + item.costUsd!, 0)) };
}

/** Read retained usage proof without treating a cost ceiling as exact usage. */
export function runCostEvidence(input: unknown, scope: 'run' | 'execution' = 'run'): CostEvidence {
  try {
    if (!input || typeof input !== 'object') return { status: 'unknown', costUsd: null };
    const ledger = durableCostLedger(input as CostRun, scope);
    if (!ledger.complete) return { status: 'unknown', costUsd: null };
    return { status: ledger.exact ? 'exact' : 'upper-bound', costUsd: ledger.reportedCostUsd };
  } catch {
    // Incomplete or malformed run artifacts do not establish a spend total.
    return { status: 'unknown', costUsd: null };
  }
}

interface SessionCostRow {
  level: number;
  kind: CostLedgerRow['kind'];
  index: number;
  sessionCostUsd: number;
  costComplete: boolean;
  receipts: CostReceiptEntry[];
}

function cost(value: unknown, at: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${at} must be a non-negative number`);
  }
  return value;
}

const roundUsd = (value: number): number => Number(value.toFixed(6));

function sessionRows(run: CostRun): SessionCostRow[] {
  const rows: SessionCostRow[] = [];
  for (const [levelIndex, level] of (run.levels ?? []).entries()) {
    if (!Number.isSafeInteger(level.level) || level.level < 1) {
      throw new Error(`levels[${levelIndex}].level must be a positive integer`);
    }
    const groups: Array<[CostLedgerRow['kind'], CostSession[]]> = [
      ['build', level.buildSessions ?? []],
      ['resume', level.resumeSession ? [level.resumeSession] : []],
      ['repair', level.repairSessions ?? []],
    ];
    for (const [kind, sessions] of groups) {
      for (const [index, session] of sessions.entries()) {
        rows.push({
          level: level.level,
          kind,
          index: index + 1,
          sessionCostUsd: cost(session.costUsd,
            `levels[${levelIndex}].${kind}[${index}].costUsd`),
          costComplete: session.costComplete === true,
          receipts: (session.costReceipts ?? []) as CostReceiptEntry[],
        });
      }
    }
  }
  return rows;
}

export function durableCostLedger(run: CostRun, scope: 'run' | 'execution' = 'run'): CostLedger {
  const inherited = scope === 'execution' ? new Set(run.progressionResume?.inheritedLevels ?? []) : new Set();
  const rows = sessionRows({ ...run, levels: (run.levels ?? []).filter(level => !inherited.has(level.level)) }).map(row => {
    const receiptCostUsd = roundUsd(row.receipts.reduce(
      (sum, entry, index) => sum + cost(entry?.receipt?.costUsd,
        `level ${row.level} ${row.kind} receipt[${index}].costUsd`),
      0,
    ));
    const receiptsComplete = (row.receipts.length === 0 && row.sessionCostUsd === 0)
      || (row.receipts.length > 0 && row.receipts.every(entry => entry?.receipt?.complete === true
        && entry.receipt.reconciled === true && entry.receipt.error === null
        && typeof entry.receipt.exact === 'boolean'));
    const differenceUsd = roundUsd(row.sessionCostUsd - receiptCostUsd);
    return {
      ...row,
      receiptCostUsd,
      differenceUsd,
      complete: row.costComplete && receiptsComplete && Math.abs(differenceUsd) <= 0.0001,
      exact: row.receipts.every(entry => entry?.receipt?.exact === true),
    };
  });
  const reportedCostUsd = roundUsd(cost(scope === 'execution' && run.progressionResume
    ? run.totals?.currentExecutionCostUsd : run.totals?.costUsd,
  scope === 'execution' && run.progressionResume ? 'totals.currentExecutionCostUsd' : 'totals.costUsd'));
  const receiptCostUsd = roundUsd(rows.reduce((sum, row) => sum + row.receiptCostUsd, 0));
  const differenceUsd = roundUsd(reportedCostUsd - receiptCostUsd);
  return {
    runId: run.id ?? null,
    pricing: run.pricing ?? null,
    reportedCostUsd,
    receiptCostUsd,
    differenceUsd,
    complete: (scope === 'execution' && run.progressionResume !== undefined
      ? true : run.totals?.costComplete === true) && rows.every(row => row.complete)
      && Math.abs(differenceUsd) <= 0.0001,
    exact: rows.every(row => row.exact),
    rows,
  };
}
