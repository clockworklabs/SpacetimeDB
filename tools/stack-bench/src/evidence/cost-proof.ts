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
  costReceipts?: CostReceiptEntry[];
  [key: string]: unknown;
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
  totals?: {
    costComplete?: boolean;
    costUsd?: number;
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
          receipts: session.costReceipts ?? [],
        });
      }
    }
  }
  return rows;
}

export function durableCostLedger(run: CostRun): CostLedger {
  const rows = sessionRows(run).map(row => {
    const receiptCostUsd = roundUsd(row.receipts.reduce(
      (sum, entry, index) => sum + cost(entry?.receipt?.costUsd,
        `level ${row.level} ${row.kind} receipt[${index}].costUsd`),
      0,
    ));
    const receiptsComplete = (row.receipts.length === 0 && row.sessionCostUsd === 0)
      || (row.receipts.length > 0 && row.receipts.every(entry => entry?.receipt?.complete === true
        && entry.receipt.reconciled === true && entry.receipt.error === null));
    const differenceUsd = roundUsd(row.sessionCostUsd - receiptCostUsd);
    return {
      ...row,
      receiptCostUsd,
      differenceUsd,
      complete: row.costComplete && receiptsComplete && Math.abs(differenceUsd) <= 0.0001,
      exact: row.receipts.every(entry => entry?.receipt?.exact !== false),
    };
  });
  const reportedCostUsd = roundUsd(run.totals?.costUsd === undefined
    ? 0 : cost(run.totals.costUsd, 'totals.costUsd'));
  const receiptCostUsd = roundUsd(rows.reduce((sum, row) => sum + row.receiptCostUsd, 0));
  const differenceUsd = roundUsd(reportedCostUsd - receiptCostUsd);
  return {
    runId: run.id ?? null,
    pricing: run.pricing ?? null,
    reportedCostUsd,
    receiptCostUsd,
    differenceUsd,
    complete: run.totals?.costComplete === true && rows.every(row => row.complete)
      && Math.abs(differenceUsd) <= 0.0001,
    exact: rows.every(row => row.exact),
    rows,
  };
}
