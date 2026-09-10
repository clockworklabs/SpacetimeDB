import assert from 'node:assert/strict';
import test from 'node:test';
import { responseCosts, cumulativeResponseCosts, liveCostTotal } from '../../dashboard/dashboard-live-cost.js';

test('live cost uses pinned response usage once, excludes inherited history, and rejects conflicting or unpriced usage', () => {
  const rates = { input: 2, output: 10, cacheRead: 0.2, cacheWrite5m: 2.5, cacheWrite1h: 4 };
  const start = '2026-09-10T10:00:00Z';
  const row = (id: string, timestamp: string, output = 100, model = 'claude-sonnet-5') => JSON.stringify({
    type: 'assistant', requestId: id, timestamp, message: { model, stop_reason: 'tool_use', usage: {
      input_tokens: 1000, output_tokens: output, cache_read_input_tokens: 0, cache_creation_input_tokens: 0,
    } },
  });
  const response = row('one', '2026-09-10T10:01:00Z');
  const points = responseCosts([row('old', '2026-09-10T09:00:00Z'), response, response,
    row('two', '2026-09-10T10:02:00Z')].join('\n'), rates, 'claude-sonnet-5', start);
  assert.deepEqual(cumulativeResponseCosts(points).map(point => point.costUsd), [0.003, 0.006]);
  assert.throws(() => cumulativeResponseCosts([...points,
    ...responseCosts(row('one', '2026-09-10T10:01:00Z', 200), rates, 'claude-sonnet-5', start)]), /Conflicting/);
  assert.throws(() => responseCosts(row('other', start, 100, 'other-model'), rates, 'claude-sonnet-5', start), /pinned price/);
  assert.throws(() => responseCosts('{broken', rates, 'claude-sonnet-5', start), /Incomplete/);
  assert.equal(liveCostTotal('running', 12, 10), 12);
  assert.equal(liveCostTotal('running', 8, 10), undefined);
  assert.equal(liveCostTotal('completed', 12, 11), undefined, 'final receipt replaces estimate, never adds it');
  assert.equal(liveCostTotal('invalid', 12, 11), undefined);
});
