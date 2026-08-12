import assert from 'node:assert/strict';
import test from 'node:test';
import { summarizeSessions } from '../session-metrics.mjs';

test('session totals include build and every fix without treating missing metrics as data', () => {
  const totals = summarizeSessions([
    { costUsd: 1.11111, tokens: 100, outputTokens: 10, turns: 2, durationMs: 50,
      promptBytes: 20, usage: { input: 10, output: 20, cacheWrite: 30, cacheRead: 40 },
      thinking: { blocks: 2, signatureBytes: 200 } },
    { costUsd: 0.22222, tokens: 50, outputTokens: 5, turns: 1, durationMs: 25,
      promptBytes: 10, usage: { input: 5, output: 10, cacheWrite: 15, cacheRead: 20 } },
    { costUsd: 0.1 },
  ]);

  assert.deepEqual(totals, {
    sessions: 3,
    costUsd: 1.4333,
    tokens: 150,
    outputTokens: 15,
    turns: 3,
    durationMs: 75,
    promptBytes: 30,
    usage: { input: 15, output: 30, cacheWrite: 45, cacheRead: 60 },
    thinking: { blocks: 2, signatureBytes: 200, sessions: 1 },
  });
});

test('empty session totals are explicit zeroes', () => {
  assert.deepEqual(summarizeSessions([]), {
    sessions: 0, costUsd: 0, tokens: 0, outputTokens: 0, turns: 0,
    durationMs: 0, promptBytes: 0,
    usage: { input: 0, output: 0, cacheWrite: 0, cacheRead: 0 },
    thinking: null,
  });
});
