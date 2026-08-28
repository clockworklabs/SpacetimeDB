import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runSessionRecord } from '../commands/bench.mjs';
import { readRunJson, writeRunJson } from '../src/evidence/artifacts.mjs';

function receipt() {
  return [{ invocation: 1, receipt: { schemaVersion: 2, source: 'credential-broker',
    model: 'test-model', maxBudgetUsd: 10, costUsd: 1, cliCostUsd: 1,
    calculatedCostUsd: 1, usage: { input: 1, output: 1, cacheWrite5m: 0,
      cacheWrite1h: 0, cacheRead: 0 }, pricingRates: { input: 1, output: 1,
      cacheWrite5m: 1, cacheWrite1h: 1, cacheRead: 1 }, complete: true,
    reconciled: true, error: null } }];
}

function session(marker, { billable = true, costComplete = true } = {}) {
  return { sessionId: marker, costUsd: billable ? 1 : 0, durationMs: 10,
    usage: { input: 1, output: 1, cacheWrite: 0, cacheRead: 0 },
    costReceipts: billable ? receipt() : [],
    setup: { providerThrottle: null }, tokens: 2, outputTokens: 1, turns: 1,
    promptBytes: 20, thinking: null, transcript: { kind: 'test', id: marker },
    provenance: null, providerMetadata: null, costComplete };
}

const cases = [
  { name: 'successful build', marker: 'build',
    level: source => ({ buildSession: runSessionRecord(source) }),
    select: level => level.buildSession },
  { name: 'failed build', marker: 'failed-build', costComplete: false,
    level: source => ({ outcome: { kind: 'harness_failure' },
      buildSession: runSessionRecord(source) }),
    select: level => level.buildSession },
  { name: 'normal repair', marker: 'repair', round: 1,
    level: source => ({ fixSessions: [runSessionRecord(source, 1)] }),
    select: level => level.fixSessions[0] },
  { name: 'resumed repair', marker: 'resumed-repair', round: 4,
    level: source => ({ resumedRepair: {}, fixSessions: [runSessionRecord(source, 4)] }),
    select: level => level.fixSessions[0] },
  { name: 'model-free early exit', marker: 'model-free', billable: false,
    level: source => ({ outcome: { kind: 'ungraded', phase: 'reference-mutation-only' },
      buildSession: runSessionRecord(source) }),
    select: level => level.buildSession },
];

for (const item of cases) {
  test(`${item.name} keeps its cost receipts in run.json`, () => {
    const root = mkdtempSync(join(tmpdir(), 'stack-bench-session-artifact-'));
    try {
      const path = join(root, 'run.json');
      const source = session(item.marker, { billable: item.billable !== false,
        costComplete: item.costComplete !== false });
      writeRunJson(path, { id: `run-${item.marker}`, levels: [item.level(source)] });
      const stored = item.select(readRunJson(path, `run-${item.marker}`).levels[0]);
      assert.deepEqual(stored.costReceipts, source.costReceipts);
      assert.equal(Object.hasOwn(stored, 'costReceipts'), true);
      assert.equal(stored.costComplete, source.costComplete);
      if (item.round !== undefined) assert.equal(stored.round, item.round);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
