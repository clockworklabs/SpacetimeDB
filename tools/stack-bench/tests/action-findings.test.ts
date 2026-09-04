import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

import { ActionApplicationFailure, ActionInconclusive } from '../src/actions/action-contract.js';
import { FAILED_FINDING_KINDS, FINDING_KINDS, INCONCLUSIVE_FINDING_KINDS, finding, findingStatus,
  isFinding, renderFinding } from '../src/actions/action-findings.js';
import type { Finding, FindingKind } from '../src/actions/action-findings.js';
import { fail, inconclusive } from '../src/actions/actor-action-runtime.js';
import { assertAgentVisibleText } from '../src/composition/agent-visible-contract.js';

// One sample per kind. The sample values are the shapes an executor has:
// contract names, actor labels, action ids, numbers, statuses. `detail` is
// the raw text a human may read and must never be rendered.
const DETAIL = 'RAW-DETAIL locator.click: Timeout 5000ms exceeded [data-testid="secret"]';
const SAMPLES: { [K in FindingKind]: Finding } = {
  'control-missing': finding('control-missing', { control: 'buy-now' }),
  'control-present': finding('control-present', { control: 'admin-link' }),
  'control-available': finding('control-available', { control: 'buy-now', actor: 'visitor' }),
  'control-not-ready': finding('control-not-ready', { control: 'buy-now', actors: ['a', 'b'] }),
  'control-blocked': finding('control-blocked', { control: 'buy-now', detail: DETAIL }),
  'control-empty': finding('control-empty', { control: 'order-total' }),
  'control-unreadable': finding('control-unreadable', { control: 'stock', actors: ['a'] }),
  'value-mismatch': finding('value-mismatch', { control: 'support-status' }),
  'text-unexpected': finding('text-unexpected', { control: 'support-ticket' }),
  'value-unstable': finding('value-unstable', { control: 'stock' }),
  'clients-disagree': finding('clients-disagree', { control: 'stock', actors: ['a', 'b'] }),
  'number-missing': finding('number-missing', { control: 'order-total' }),
  'number-mismatch': finding('number-mismatch', { control: 'order-total', observed: 9,
    expected: { equals: 12 } }),
  'count-mismatch': finding('count-mismatch', { control: 'order-item', observed: 2, expected: 1 }),
  'order-mismatch': finding('order-mismatch', { control: 'message-item', actors: ['a', 'b'] }),
  'entries-missing': finding('entries-missing', { expected: 10, missing: 2, duplicated: 1 }),
  'actors-with-control': finding('actors-with-control', { control: 'ticket', observed: 2, expected: 1 }),
  'too-many-per-actor': finding('too-many-per-actor', { control: 'ticket', maxEach: 1 }),
  'clicks-failed': finding('clicks-failed', { control: 'buy-now', failed: 1, total: 3, detail: DETAIL }),
  'choice-missing': finding('choice-missing', { control: 'frequency', detail: DETAIL }),
  'page-timeout': finding('page-timeout', { control: 'buy-now', detail: DETAIL }),
  'page-crashed': finding('page-crashed', { detail: DETAIL }),
  'page-error': finding('page-error', { detail: DETAIL }),
  'app-control-failed': finding('app-control-failed', { mode: 'start', target: 'app-server', detail: DETAIL }),
  'script-failed': finding('script-failed', { script: 'scripts/seed.js', detail: DETAIL }),
  'script-invalid': finding('script-invalid', { script: '../outside.js' }),
  'action-failed': finding('action-failed', { action: 'click' }),
  'call-refused': finding('call-refused', { action: 'buy', actor: 'buyer', status: 404,
    operation: { reducer: 'buy_now', path: '/api/buy', method: 'POST' } }),
  'call-accepted': finding('call-accepted', { action: 'buy', actor: 'guest', status: 200, required: 'refused' }),
  'call-error': finding('call-error', { action: 'buy', actor: 'guest', status: 500, required: 'refused',
    operation: null }),
  'concurrent-calls-mismatch': finding('concurrent-calls-mismatch', { action: 'checkout', expected: 1,
    accepted: 2, fired: 2, detail: DETAIL }),
  'interface-missing': finding('interface-missing', { control: 'admin-row', action: 'restock',
    attribute: 'data-action-input' }),
  'interface-invalid': finding('interface-invalid', { action: 'restock', attribute: 'data-action-input',
    missing: ['itemId'], detail: DETAIL }),
  'replay-accepted': finding('replay-accepted', { actor: 'customer', status: 200, action: 'restock' }),
  'replay-error': finding('replay-error', { status: 500 }),
  'forgery-accepted': finding('forgery-accepted', { field: 'userId', status: 201 }),
  'forgery-error': finding('forgery-error', { status: null }),
  'message-delivered': finding('message-delivered', { actor: 'other' }),
  'stock-interface-missing': finding('stock-interface-missing', { detail: DETAIL }),
  'assertion-without-action': finding('assertion-without-action', { action: 'replayAs' }),
  'unknown-action': finding('unknown-action', { action: 'refund' }),
  'action-without-parameters': finding('action-without-parameters', { action: 'refund' }),
  'no-session': finding('no-session', { actor: 'buyer', action: 'buy' }),
  'unresolved-action': finding('unresolved-action', { action: 'buy' }),
  'replay-unavailable': finding('replay-unavailable', { actor: 'customer', detail: DETAIL }),
  'forgery-unverifiable': finding('forgery-unverifiable', { actor: 'customer', detail: DETAIL }),
  'not-observed': finding('not-observed', { actor: 'owner' }),
  'nothing-contended': finding('nothing-contended', { detail: DETAIL }),
  'no-backend-control': finding('no-backend-control', { target: 'backend-runtime' }),
  'control-refused': finding('control-refused', { target: 'app-server' }),
  'database-write-failed': finding('database-write-failed', { detail: DETAIL }),
  'unsupported-backend': finding('unsupported-backend', { backend: 'stub' }),
  'app-directory-unknown': finding('app-directory-unknown', {}),
  'invalid-input': finding('invalid-input', { detail: DETAIL }),
};

test('every kind renders one agent-visible sentence and never its detail', () => {
  assert.deepEqual(Object.keys(SAMPLES).sort(), [...FINDING_KINDS]);
  for (const kind of FINDING_KINDS) {
    const sentence = renderFinding(SAMPLES[kind]);
    assert.ok(sentence.length > 8, kind);
    assert.doesNotMatch(sentence, /RAW-DETAIL|data-testid|Timeout|\d+ms|undefined/, kind);
    assert.doesNotThrow(() => assertAgentVisibleText(sentence), kind);
    assert.ok(isFinding(SAMPLES[kind]), kind);
  }
});

test('the catalog partitions into application failures and unmeasured outcomes', () => {
  assert.deepEqual([...FAILED_FINDING_KINDS, ...INCONCLUSIVE_FINDING_KINDS].sort(), [...FINDING_KINDS]);
  assert.equal(new Set(FINDING_KINDS).size, FINDING_KINDS.length);
  for (const kind of FAILED_FINDING_KINDS) assert.equal(findingStatus(SAMPLES[kind]), 'failed');
  for (const kind of INCONCLUSIVE_FINDING_KINDS) assert.equal(findingStatus(SAMPLES[kind]), 'inconclusive');
  assert.equal(isFinding({ kind: 'made-up', fields: {} }), false);
  assert.equal(isFinding({ kind: 'control-missing' }), false);
});

test('executors fail with a finding, and the message is its rendering', () => {
  assert.throws(() => fail('count-mismatch', { control: 'order-item', observed: 2, expected: 1 }),
    (error: unknown) => error instanceof ActionApplicationFailure
      && error.message === '2 order-item entries shown, expected 1'
      && isFinding(error.details.finding) && error.details.finding.kind === 'count-mismatch');
  assert.throws(() => inconclusive('not-observed', { actor: 'owner' }),
    (error: unknown) => error instanceof ActionInconclusive
      && error.message === 'the message could not be observed reaching owner'
      && isFinding(error.details.finding) && error.details.finding.kind === 'not-observed');
});

// The runtime helpers are the only way an executor fails. Two named
// exceptions wrap an existing finding rather than invent prose: the browser
// boundary classifies a Playwright error, and a nested step forwards its
// inner finding.
test('executors fail only through the runtime helpers', () => {
  const directory = join(STACK_BENCH_ROOT, 'src', 'actions');
  const allowed = new Set(['action-contract.ts', 'actor-action-runtime.ts',
    'browser-action-executors.ts', 'runtime-action-executors.ts']);
  for (const file of readdirSync(directory).filter(name => name.endsWith('.ts'))) {
    const source = readFileSync(join(directory, file), 'utf8');
    const direct = source.match(/new Action(?:ApplicationFailure|Inconclusive)\(/g) ?? [];
    if (allowed.has(file)) continue;
    assert.deepEqual(direct, [], `${file} constructs a failure outside the runtime helpers`);
  }
  const boundary = readFileSync(join(directory, 'browser-action-executors.ts'), 'utf8');
  assert.equal((boundary.match(/new ActionApplicationFailure\(/g) ?? []).length, 1);
  const nested = readFileSync(join(directory, 'runtime-action-executors.ts'), 'utf8');
  assert.equal((nested.match(/new Action(?:ApplicationFailure|Inconclusive)\(/g) ?? []).length, 2);
});

test('sample renderings read as behavior, not mechanics', () => {
  assert.equal(renderFinding(SAMPLES['number-mismatch']),
    'the order-total control reads 9, expected exactly 12');
  assert.equal(renderFinding(SAMPLES['call-refused']),
    'the buy action was refused for buyer (HTTP 404); the application interface names the buy_now reducer or POST /api/buy');
  assert.equal(renderFinding(SAMPLES['replay-accepted']),
    'a request replayed as customer, who must be refused, was accepted (HTTP 200)');
  assert.equal(renderFinding(SAMPLES['page-error']), 'the page did not behave as required');
  assert.equal(renderFinding(SAMPLES['forgery-error']),
    'the tampered request failed with no server response instead of a refusal');
});
