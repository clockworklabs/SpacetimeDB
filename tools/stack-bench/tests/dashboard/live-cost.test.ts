import assert from 'node:assert/strict';
import test from 'node:test';
import { responseCosts, codexResponseCosts, cumulativeResponseCosts, liveCostTotal } from '../../dashboard/dashboard-live-cost.js';

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

test('Codex live usage counts cumulative deltas across sessions without counting cached or reasoning tokens twice', () => {
  const rates = { input: 2, output: 10, cacheRead: 0.2, cacheWrite5m: 0, cacheWrite1h: 0 };
  const start = '2026-09-12T01:00:00Z';
  const header = (id: string, model = 'gpt-6-astra') => [
    JSON.stringify({ type: 'session_meta', payload: { id } }),
    JSON.stringify({ type: 'turn_context', payload: { model } }),
  ].join('\n') + '\n';
  const row = (input: number, cached: number, output: number, timestamp = start) => JSON.stringify({
    type: 'event_msg', timestamp, payload: { type: 'token_count', info: { total_token_usage: {
      input_tokens: input, cached_input_tokens: cached, output_tokens: output, reasoning_output_tokens: output,
    } } },
  });
  const parse = (text: string) => codexResponseCosts(text, rates, 'gpt-6-astra', start);
  const prefix = header('one') + row(1000, 800, 100, '2026-09-12T00:59:00Z') + '\n';
  assert.deepEqual(parse(prefix), []);
  const first = parse(prefix + row(2000, 1600, 200));
  const next = parse(prefix + [row(2000, 1600, 200), row(2000, 1600, 200), row(3000, 2400, 300)].join('\n'));
  const other = codexResponseCosts(header('two') + row(1000, 800, 100), rates, 'gpt-6-astra', start);
  assert.deepEqual(cumulativeResponseCosts([...first, ...first, ...next, ...other]).map(p => p.costUsd),
    [0.00156, 0.00312, 0.00468]);
  assert.throws(() => parse(prefix + row(999, 800, 100)), /decreased/);
  assert.throws(() => codexResponseCosts(header('x', 'other') + row(1000, 0, 1), rates, 'gpt-6-astra', start), /pinned price/);
  assert.throws(() => codexResponseCosts(header('x') + row(1, 2, 1), rates, 'gpt-6-astra', start), /Invalid Codex/);
});
