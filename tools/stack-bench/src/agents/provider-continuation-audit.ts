import { runCostEvidence, sessionCostEvidence } from '../evidence/cost-proof.js';
import type { CostEvidence } from '../evidence/cost-proof.js';

export interface HistoricalProviderContinuation {
  eligible: false;
  work: 'paid' | 'zero-usage-candidate' | 'unknown';
  sessionId: string | null;
  reason: string;
  sessionCost: CostEvidence;
  runCost: CostEvidence;
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function sessions(run: unknown): Record<string, unknown>[] {
  if (!record(run) || !Array.isArray(run.levels)) return [];
  const inherited = record(run.progressionResume) && Array.isArray(run.progressionResume.inheritedLevels)
    ? run.progressionResume.inheritedLevels : [];
  return run.levels.filter(record).filter(level => !inherited.includes(level.level)).flatMap(level => [
    ...(Array.isArray(level.buildSessions) ? level.buildSessions : []),
    ...(level.resumeSession ? [level.resumeSession] : []),
    ...(Array.isArray(level.repairSessions) ? level.repairSessions : []),
  ]).filter(record);
}

const zeroUsage = (value: unknown, keys: string[]): boolean =>
  record(value) && keys.every(key => value[key] === 0);

/** Historical diagnostics are never authorization to resume a stopped process. */
export function assessStoppedProviderContinuation(run: unknown): HistoricalProviderContinuation | null {
  const session = sessions(run).at(-1);
  const metadata = session?.providerMetadata;
  if (!session || !record(metadata) || metadata.failureCode !== 'provider-throttle-exhausted') return null;
  const usage = session.usage;
  const receipts = Array.isArray(session.costReceipts) ? session.costReceipts : [];
  const entries = receipts.filter(record);
  const paid = (typeof session.costUsd === 'number' && session.costUsd > 0)
    || (record(usage) && Object.values(usage).some(value => typeof value === 'number' && value > 0))
    || entries.some(entry => record(entry.receipt) && (Number(entry.receipt.costUsd) > 0
      || (record(entry.receipt.usage) && Object.values(entry.receipt.usage)
        .some(value => typeof value === 'number' && value > 0))));
  const sessionCost = sessionCostEvidence([{
    costUsd: typeof session.costUsd === 'number' ? session.costUsd : NaN,
    costComplete: session.costComplete === true, costReceipts: receipts,
  }]);
  const zero = sessionCost.status === 'exact' && sessionCost.costUsd === 0
    && zeroUsage(usage, ['input', 'output', 'cacheWrite', 'cacheRead'])
    && Number.isSafeInteger(metadata.invocations) && Number(metadata.invocations) > 0
    && entries.length === receipts.length && entries.length === metadata.invocations
    && entries.every((entry, index) => entry.invocation === index + 1 && record(entry.receipt)
      && entry.receipt.exact === true && entry.receipt.costUsd === 0
      && zeroUsage(entry.receipt.usage,
        ['input', 'output', 'cacheRead', 'cacheWrite5m', 'cacheWrite1h']));
  return {
    eligible: false,
    work: paid ? 'paid' : zero ? 'zero-usage-candidate' : 'unknown',
    sessionId: typeof session.sessionId === 'string' ? session.sessionId : null,
    reason: paid
      ? 'The stopped action did model work. Its original native session and live runtime are not proved recoverable.'
      : zero
        ? 'Every recorded invocation has exact zero usage. This is only an audit candidate: tool-work, original runtime, native session and compatible executable proofs are missing. Stopped executions cannot continue.'
        : 'The stopped action lacks complete zero-work evidence. Stopped executions cannot continue.',
    sessionCost, runCost: runCostEvidence(run),
  };
}

export interface ProviderWaitSummary {
  waits: number;
  waitedMs: number;
  continued: number;
  stopped: number;
  waiting?: number;
  durationKind?: 'exact' | 'lower-bound';
  details?: Array<{ generation: string; disposition: string; waitedMs: number;
    durationKind: 'exact' | 'lower-bound' }>;
}

/** Keep operator waits separate from automatic provider backoff. */
export function providerWaitSummary(run: unknown): ProviderWaitSummary | null {
  const waits = sessions(run).flatMap(session => record(session.providerMetadata)
    && Array.isArray(session.providerMetadata.providerWaits) ? session.providerMetadata.providerWaits : []);
  if (!waits.length) return null;
  if (!waits.every(wait => record(wait) && Number.isSafeInteger(wait.waitedMs)
    && Number(wait.waitedMs) >= 0 && ['continued', 'stopped'].includes(String(wait.disposition)))) {
    throw new Error('Invalid provider wait history');
  }
  return { waits: waits.length,
    waitedMs: waits.reduce((sum, wait) => sum + Number(wait.waitedMs), 0),
    continued: waits.filter(wait => wait.disposition === 'continued').length,
    stopped: waits.filter(wait => wait.disposition === 'stopped').length };
}
