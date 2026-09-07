import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { addCostUsd, finalizeRunTotals } from '../src/evidence/benchmark-run.js';
import { checkDatabaseProvenance } from '../commands/run-suite.js';
import { AGENT_PROCESS_TIMEOUT_MS, CODING_SESSION_TIMEOUT_MS }
  from '../src/agents/coding-session-timeouts.js';
import { summarizeSessions } from '../src/evidence/session-metrics.js';

test('database provenance accepts the leased environment and rejects an unrelated literal', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-database-provenance-'));
  try {
    const server = join(root, 'server');
    mkdirSync(server);
    writeFileSync(join(server, 'db.ts'),
      'export const connectionString = process.env.DATABASE_URL;\n');
    assert.deepEqual(checkDatabaseProvenance({ app: root, backend: 'postgres' }), {
      ok: true, url: 'process.env.DATABASE_URL',
      reason: 'app reads the database URL supplied by its authenticated backend lease',
    });
    writeFileSync(join(server, 'db.ts'),
      'export const connectionString = "postgresql://user:pass@localhost:5433/wrong";\n');
    const wrong = checkDatabaseProvenance({ app: root, backend: 'postgres' });
    assert.equal(wrong.ok, false);
    assert.match(wrong.reason, /benchmark database is on port 6532/);
    writeFileSync(join(server, 'db.ts'),
      'export const connectionString = "postgresql://user:pass@localhost:5433/wrong?note=:6532/";\n');
    assert.equal(checkDatabaseProvenance({ app: root, backend: 'postgres' }).ok, false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('coding sessions are bounded without the former 55-minute cutoff', () => {
  assert.equal(CODING_SESSION_TIMEOUT_MS, 120 * 60_000);
  assert.equal(AGENT_PROCESS_TIMEOUT_MS, CODING_SESSION_TIMEOUT_MS + 3 * 60_000);
});

test('an interrupted later level keeps completed totals and marks cost incomplete', () => {
  const run = { levels: [{ level: 1, graded: true, score: 58, max: 58,
    buildCostUsd: 10, repairCostUsd: 2.5, repairs: 1,
    sessionTotals: { sessions: 2, tokens: 100, outputTokens: 20, turns: 8, durationMs: 900 } }] };
  assert.deepEqual(finalizeRunTotals(run, 1_000, { now: 11_000, costComplete: false }), {
    score: 58, max: 58, costUsd: 12.5, costComplete: false, repairs: 1,
    sessions: 2, tokens: 100, outputTokens: 20, turns: 8, modelDurationMs: 900,
    durationSec: 10, ungraded: [],
  });
});

test('dependency totals use the cumulative progression score', () => {
  const run = {
    levels: [
      { level: 1, graded: true, score: 8, max: 12 },
      { level: 2, graded: true, score: 13, max: 15 },
    ],
    progressionStatus: { score: { uniqueChecks: { passedPoints: 17, availablePoints: 162 } } },
  };
  const totals = finalizeRunTotals(run, 1_000, { now: 2_000 });
  assert.equal(totals.score, 17);
  assert.equal(totals.max, 162);
});

test('session and run totals keep receipt precision', () => {
  const sessions = [{ costUsd: 0.123456 }, { costUsd: 0.234567 }, { costUsd: 0.345678 }];
  assert.equal(summarizeSessions(sessions).costUsd, 0.703701);
  const run = { levels: [{ level: 1, buildCostUsd: 0.123456, repairCostUsd: 0.234567 },
    { level: 2, buildCostUsd: 0.345678, repairCostUsd: 0 }] };
  assert.equal(finalizeRunTotals(run, 0, { now: 1 }).costUsd, 0.703701);
  assert.equal(addCostUsd(0.123456, 0.234567, 0.345678), 0.703701);
});
