import { existsSync, readdirSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { readCredentialBrokerLedger } from '../../container/credential-broker-accounting.js';
import { readBackendLease } from '../runtime/backend-lease.js';
import { sha256 } from './provenance.js';
import type { CostEvidence, CostRun } from './cost-proof.js';

// Recover accounting from the controller's retained ledgers, never from app files.
// This does not change grades, run.json, or campaign eligibility.
export function retainedRunCost(input: unknown, runtimeRoot = process.env.STACK_BENCH_RUNTIME_DIR): {
  cost: CostEvidence; recorded: CostEvidence;
} | null {
  if (!input || typeof input !== 'object') return null;
  const run = input as CostRun & { backend?: string; model?: string;
    backendLease?: { ownership?: { markerSha256?: string } } };
  if (typeof run.id !== 'string' || typeof run.model !== 'string' || typeof run.backend !== 'string') return null;
  if (!runtimeRoot || !isAbsolute(runtimeRoot) || !run.id || !/^[a-zA-Z0-9_-]+$/.test(run.id)) return null;
  const directory = join(runtimeRoot, run.id);
  if (!existsSync(join(directory, 'backend-lease.json'))) return null;
  try {
    const lease = readBackendLease(join(directory, 'backend-lease.json'),
      { runId: run.id, backend: run.backend });
    if (!run.backendLease?.ownership?.markerSha256
      || sha256(lease.ownershipToken) !== run.backendLease.ownership.markerSha256) return null;
    const names = readdirSync(directory).filter(name => name.startsWith('stack-bench-credential-broker-'));
    if (!names.length) return null;
    const ledgers = names.map(name => readCredentialBrokerLedger(join(directory, name, 'spend-ledger.json'),
      { model: run.model }));
    const total = Number(ledgers.reduce((sum, ledger) => sum + ledger.spentUsd, 0).toFixed(6));
    const checkpoint = run.checkpoints?.at(-1)?.executionCost;
    if (checkpoint?.status === 'exact' && total + 0.0001 < checkpoint.costUsd) return null;
    // Every saved receipt must still have a ledger. Equal-cost sessions are
    // separate charges; consume matches instead of deduplicating by cost.
    const remaining = [...ledgers];
    const inherited = new Set(run.progressionResume?.inheritedLevels ?? []);
    for (const level of run.levels ?? []) {
      if (inherited.has(level.level)) continue;
      const sessions = [...(level.buildSessions ?? []), ...(level.repairSessions ?? []),
        ...(level.resumeSession ? [level.resumeSession] : [])];
      for (const session of sessions) for (const entry of session.costReceipts ?? []) {
        const value = (entry as { receipt?: { costUsd?: number } }).receipt?.costUsd;
        const index = remaining.findIndex(ledger => typeof value === 'number' && Math.abs(ledger.spentUsd - value) <= 0.0001);
        if (index < 0) return null;
        remaining.splice(index, 1);
      }
    }
    const recorded: CostEvidence = {
      status: ledgers.some(ledger => ledger.estimatedBillableRequests > 0) ? 'upper-bound' : 'exact', costUsd: total,
    };
    return { recorded, cost: ledgers.every(ledger => ledger.complete) ? recorded : { status: 'unknown', costUsd: null } };
  } catch { return null; }
}
