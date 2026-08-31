export interface CostReceiptEntry {
  receipt?: {
    complete?: boolean;
    reconciled?: boolean;
    error?: unknown;
    costUsd?: number;
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
  buildSession?: CostSession;
  resumeSession?: CostSession;
  fixSessions?: CostSession[];
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
}

export interface CostLedger {
  complete: boolean;
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

const roundUsd = (value: number): number => Number(value.toFixed(6));

function sessionRows(run: CostRun): SessionCostRow[] {
  const rows: SessionCostRow[] = [];
  for (const level of run.levels ?? []) {
    const groups: Array<[CostLedgerRow['kind'], CostSession[]]> = [
      ['build', level.buildSession ? [level.buildSession] : []],
      ['resume', level.resumeSession ? [level.resumeSession] : []],
      ['repair', level.fixSessions ?? []],
    ];
    for (const [kind, sessions] of groups) {
      for (const [index, session] of sessions.entries()) {
        rows.push({
          level: level.level,
          kind,
          index: index + 1,
          sessionCostUsd: session.costUsd,
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
      (sum, entry) => {
        const costUsd = entry?.receipt?.costUsd;
        return sum + (typeof costUsd === 'number' && Number.isFinite(costUsd) ? costUsd : 0);
      },
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
    };
  });
  const reportedCostUsd = roundUsd(run.totals?.costUsd ?? 0);
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
    rows,
  };
}
