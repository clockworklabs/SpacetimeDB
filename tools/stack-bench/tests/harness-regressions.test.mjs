import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { finalizeRunTotals } from '../commands/bench.mjs';
import { checkDatabaseProvenance } from '../commands/run-suite.mjs';
import { AGENT_PROCESS_TIMEOUT_MS, CODING_SESSION_TIMEOUT_MS }
  from '../src/agents/coding-session-timeouts.mjs';

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
    buildCostUsd: 10, fixCostUsd: 2.5, fixRounds: 1,
    sessionTotals: { sessions: 2, tokens: 100, outputTokens: 20, turns: 8, durationMs: 900 } }] };
  assert.deepEqual(finalizeRunTotals(run, 1_000, { now: 11_000, costComplete: false }), {
    score: 58, max: 58, costUsd: 12.5, costComplete: false, fixRounds: 1,
    sessions: 2, tokens: 100, outputTokens: 20, turns: 8, modelDurationMs: 900,
    durationSec: 10, ungraded: [],
  });
});
