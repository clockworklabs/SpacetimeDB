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

export function durableCostLedger(run: CostRun): CostLedger;
