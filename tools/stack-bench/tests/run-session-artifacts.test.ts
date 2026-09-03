import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runSessionRecord } from '../src/evidence/benchmark-run.js';
import { readRunJson, writeRunJson } from '../src/evidence/artifacts.js';
import type { AgentCostReceiptEntry, ValidatedAgentResult }
  from '../src/agents/agent-result-contract.js';
import type { RunSessionRecord } from '../src/evidence/benchmark-run.js';

function receipt(): AgentCostReceiptEntry[] {
  return [{ invocation: 1, receipt: { schemaVersion: 2, source: 'credential-broker',
    model: 'test-model', maxBudgetUsd: 10, costUsd: 1, cliCostUsd: 1,
    calculatedCostUsd: 1, usage: { input: 1, output: 1, cacheWrite5m: 0,
      cacheWrite1h: 0, cacheRead: 0 }, pricingRates: { input: 1, output: 1,
      cacheWrite5m: 1, cacheWrite1h: 1, cacheRead: 1 }, complete: true,
    reconciled: true, error: null } }];
}

function session(marker: string,
  { billable = true, costComplete = true }: { billable?: boolean; costComplete?: boolean } = {},
): ValidatedAgentResult {
  return { appDir: '/app', mode: 'build', level: 1, backend: 'stub', track: 'loop',
    model: 'test-model', guidance: null, ok: true, sessionId: marker,
    costUsd: billable ? 1 : 0, durationMs: 10,
    usage: { input: 1, output: 1, cacheWrite: 0, cacheRead: 0 },
    costReceipts: billable ? receipt() : [],
    setup: { resources: { buildContainerMemory: {
      currentBytes: 100, peakBytes: 200, limitBytes: 400,
    }, memoryProbeError: null } }, tokens: 2, outputTokens: 1, turns: 1,
    promptBytes: 20, thinking: null, transcript: { kind: 'test', id: marker },
    provenance: null, providerMetadata: null, costComplete };
}

type SessionLocation = 'buildSessions' | 'repairSessions';

interface SessionArtifactLevel {
  buildSessions?: RunSessionRecord[];
  repairSessions?: RunSessionRecord[];
  resumedRepair?: Record<string, unknown>;
  outcome?: { kind: string; phase?: string };
}

interface SessionCase {
  name: string;
  marker: string;
  billable?: boolean;
  costComplete?: boolean;
  round?: number;
  level: (source: ValidatedAgentResult) => SessionArtifactLevel;
  location: SessionLocation;
}

const cases: readonly SessionCase[] = [
  { name: 'successful build', marker: 'build',
    level: source => ({ buildSessions: [runSessionRecord(source)] }),
    location: 'buildSessions' },
  { name: 'failed build', marker: 'failed-build', costComplete: false,
    level: source => ({ outcome: { kind: 'harness_failure' },
      buildSessions: [runSessionRecord(source)] }),
    location: 'buildSessions' },
  { name: 'normal repair', marker: 'repair', round: 1,
    level: source => ({ repairSessions: [runSessionRecord(source, 1)] }),
    location: 'repairSessions' },
  { name: 'resumed repair', marker: 'resumed-repair', round: 4,
    level: source => ({ resumedRepair: {}, repairSessions: [runSessionRecord(source, 4)] }),
    location: 'repairSessions' },
  { name: 'model-free early exit', marker: 'model-free', billable: false,
    level: source => ({ outcome: { kind: 'ungraded', phase: 'reference-mutation-only' },
      buildSessions: [runSessionRecord(source)] }),
    location: 'buildSessions' },
];

interface StoredSession {
  costReceipts: unknown[];
  costComplete: boolean;
  round?: number;
  resources: unknown;
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function storedSession(run: unknown, location: SessionLocation): StoredSession {
  if (!object(run) || !Array.isArray(run.levels) || !object(run.levels[0])) {
    throw new Error('run artifact is missing its first level');
  }
  const level = run.levels[0];
  const sessions = level[location];
  const candidate = Array.isArray(sessions) ? sessions[0] : undefined;
  if (!object(candidate) || !Array.isArray(candidate.costReceipts)
    || typeof candidate.costComplete !== 'boolean') {
    throw new Error(`run artifact is missing ${location} cost data`);
  }
  const round = typeof candidate.round === 'number' ? candidate.round : undefined;
  return { costReceipts: candidate.costReceipts, costComplete: candidate.costComplete,
    resources: candidate.resources,
    ...(round === undefined ? {} : { round }) };
}

for (const item of cases) {
  test(`${item.name} keeps its cost receipts in run.json`, () => {
    const root = mkdtempSync(join(tmpdir(), 'stack-bench-session-artifact-'));
    try {
      const path = join(root, 'run.json');
      const source = session(item.marker, { billable: item.billable !== false,
        costComplete: item.costComplete !== false });
      writeRunJson(path, { id: `run-${item.marker}`, levels: [item.level(source)] });
      const stored = storedSession(readRunJson(path, `run-${item.marker}`), item.location);
      assert.deepEqual(stored.costReceipts, source.costReceipts);
      assert.equal(Object.hasOwn(stored, 'costReceipts'), true);
      assert.equal(stored.costComplete, source.costComplete);
      assert.deepEqual(stored.resources, source.setup.resources);
      if (item.round !== undefined) assert.equal(stored.round, item.round);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
